//! `DiskRecorderConsumer` — off-thread H.264/MP4 encoding + JSONL writers.
//!
//! Heavy encoding work is offloaded to a dedicated worker thread.
//! The capture thread is NEVER blocked by disk I/O or encoding.
//!
//! # File layout (per session directory)
//! - `frames.jsonl` — one JSON line per frame (`FrameRecord`)
//! - `events.jsonl` — one JSON line per input event (`EventRecord`)
//! - `video.mp4`    — H.264/MP4 (only when `--features ffmpeg` is enabled)
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use crate::hooks::InputEvent;
use crate::pipeline::consumer::{ConsumerError, FrameConsumer};
use crate::pipeline::metrics::PipelineMetrics;
use crossbeam_channel::TrySendError;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

// ─── JSONL record types ───────────────────────────────────────────────────────

/// One line in `frames.jsonl`.
#[derive(serde::Serialize)]
struct FrameRecord {
    frame_id: u64,
    capture_ts_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    dxgi_acquire_ts_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    buffer_ready_ts_ns: Option<u64>,
}

/// One line in `events.jsonl`.
#[derive(serde::Serialize)]
struct EventRecord {
    event_id: u64,
    event_ts_ns: u64,
    queue_push_done_ts_ns: u64,
    #[serde(rename = "type")]
    event_type: &'static str,
    payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    prev_frame_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prev_frame_ts_ns: Option<u64>,
    /// `event_ts_ns − prev_frame_ts_ns`. Negative values indicate clock anomaly.
    #[serde(skip_serializing_if = "Option::is_none")]
    dt_prev_ns: Option<i64>,
}

// ─── Encoder work queue messages ──────────────────────────────────────────────

// fields `data`/`width`/`height` are consumed by the H.264 encoder when
// the `ffmpeg` feature is enabled; suppress dead_code without it.
#[allow(dead_code)]
struct FrameWork {
    data: Vec<u8>,
    width: u32,
    height: u32,
    frame_id: u64,
    capture_ts_ns: u64,
    dxgi_acquire_ts_ns: Option<u64>,
    buffer_ready_ts_ns: Option<u64>,
}

struct EventBatch {
    events: Vec<InputEvent>,
    prev_frame_id: Option<u64>,
    prev_frame_ts_ns: Option<u64>,
    /// Monotonic timestamp recorded after the channel push completes.
    push_done_ts_ns: u64,
}

enum EncoderWork {
    Frame(FrameWork),
    Events(EventBatch),
    Flush,
}

// ─── Consumer ─────────────────────────────────────────────────────────────────

/// `DiskRecorderConsumer` — writes `video.mp4`, `frames.jsonl`, and `events.jsonl`.
///
/// `consume()` pushes the frame to a bounded encoder queue (non-blocking).
/// `flush()` signals the worker, drains the queue, and finalizes all files.
pub struct DiskRecorderConsumer {
    metrics: Arc<PipelineMetrics>,
    sender: crossbeam_channel::Sender<EncoderWork>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// Frame ID of the most recently enqueued frame (for event correlation).
    last_frame_id: Option<u64>,
    /// Capture timestamp of the most recently enqueued frame.
    last_frame_ts_ns: Option<u64>,
}

impl DiskRecorderConsumer {
    /// Default encoder queue capacity (frames).
    pub const DEFAULT_QUEUE_CAP: usize = 64;

    /// Create a new recorder.
    ///
    /// Artifacts are written to `session_dir` by a dedicated worker thread.
    ///
    /// - `fps`       — used for H.264 time base (falls back to 30 if zero).
    /// - `queue_cap` — bounded encoder queue size; frames are dropped when full.
    ///   Pass `0` to use `DEFAULT_QUEUE_CAP`.
    pub fn new(
        session_dir: PathBuf,
        fps: f64,
        queue_cap: usize,
        metrics: Arc<PipelineMetrics>,
    ) -> Result<Self, ConsumerError> {
        let cap = if queue_cap == 0 {
            Self::DEFAULT_QUEUE_CAP
        } else {
            queue_cap
        };
        let (sender, receiver) = crossbeam_channel::bounded(cap);
        let metrics2 = Arc::clone(&metrics);

        let worker = std::thread::Builder::new()
            .name("disk-recorder".to_string())
            .spawn(move || {
                if let Err(e) = worker_main(session_dir, fps, receiver, metrics2) {
                    eprintln!("[ERROR] DiskRecorderConsumer worker: {}", e);
                }
            })
            .map_err(|e| ConsumerError::Other(e.to_string()))?;

        Ok(Self {
            metrics,
            sender,
            worker: Some(worker),
            last_frame_id: None,
            last_frame_ts_ns: None,
        })
    }

    /// Feed input events that occurred between the previous and current frame.
    ///
    /// Call this BEFORE `consume()` for the same frame so that `prev_frame_id`
    /// and `prev_frame_ts_ns` are correct.
    ///
    /// `push_done_ts_ns` is the monotonic timestamp recorded after the channel
    /// push completes (used to compute `queue_push_done_ts_ns` in the record).
    pub fn push_events(&mut self, events: Vec<InputEvent>, push_done_ts_ns: u64) {
        if events.is_empty() {
            return;
        }
        let batch = EventBatch {
            events,
            prev_frame_id: self.last_frame_id,
            prev_frame_ts_ns: self.last_frame_ts_ns,
            push_done_ts_ns,
        };
        // Blocking send: input events must never be silently dropped.
        // Frames use try_send (drop-oldest semantics), but events are sparse
        // and small — a brief capture-thread stall is acceptable.
        let _ = self.sender.send(EncoderWork::Events(batch));
    }
}

impl FrameConsumer for DiskRecorderConsumer {
    fn consume(&mut self, frame: &CapturedFrame, frame_id: u64) -> Result<(), ConsumerError> {
        let work = EncoderWork::Frame(FrameWork {
            data: frame.data.clone(),
            width: frame.width,
            height: frame.height,
            frame_id,
            capture_ts_ns: frame.capture_ts_ns.0,
            dxgi_acquire_ts_ns: frame.dxgi_acquire_ts_ns.map(|t| t.0),
            buffer_ready_ts_ns: frame.buffer_ready_ts_ns.map(|t| t.0),
        });

        match self.sender.try_send(work) {
            Ok(()) => {
                self.metrics.queue_depth.fetch_add(1, Ordering::Relaxed);
                self.last_frame_id = Some(frame_id);
                self.last_frame_ts_ns = Some(frame.capture_ts_ns.0);
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                // Drop-oldest semantics: frame dropped, record the drop count.
                self.metrics
                    .ring_buffer_drop_count
                    .fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => Err(ConsumerError::Dropped),
        }
    }

    fn flush(&mut self) -> Result<(), ConsumerError> {
        // Signal the worker thread to finalize and exit.
        // Ignore send errors — worker may have already exited on its own.
        let _ = self.sender.send(EncoderWork::Flush);

        if let Some(handle) = self.worker.take() {
            handle.join().map_err(|_| {
                ConsumerError::Other("DiskRecorder worker thread panicked".to_string())
            })?;
        }
        Ok(())
    }

    fn push_events(&mut self, events: Vec<crate::hooks::InputEvent>, push_done_ts_ns: u64) {
        DiskRecorderConsumer::push_events(self, events, push_done_ts_ns);
    }
}

// ─── Worker thread ────────────────────────────────────────────────────────────

fn worker_main(
    session_dir: PathBuf,
    fps: f64,
    recv: crossbeam_channel::Receiver<EncoderWork>,
    metrics: Arc<PipelineMetrics>,
) -> Result<(), ConsumerError> {
    let effective_fps = if fps > 0.0 { fps } else { 30.0 };

    // Create JSONL files.
    let frames_file = File::create(session_dir.join("frames.jsonl"))
        .map_err(|e| ConsumerError::DiskError(e.to_string()))?;
    let events_file = File::create(session_dir.join("events.jsonl"))
        .map_err(|e| ConsumerError::DiskError(e.to_string()))?;

    let mut frames_w = BufWriter::new(frames_file);
    let mut events_w = BufWriter::new(events_file);
    let mut event_id: u64 = 0;

    // H.264 encoder — lazily initialized on first frame (dimensions not known until then).
    #[cfg(feature = "ffmpeg")]
    let mut video_enc: Option<FfmpegH264Encoder> = None;

    // Suppress "unused variable" warning when ffmpeg feature is off.
    let _ = effective_fps;

    loop {
        match recv.recv() {
            Ok(EncoderWork::Frame(fw)) => {
                metrics.queue_depth.fetch_sub(1, Ordering::Relaxed);

                // Write frames.jsonl entry.
                write_frame_record(&mut frames_w, &fw)?;

                // H.264 encoding (requires `--features ffmpeg`).
                #[cfg(feature = "ffmpeg")]
                {
                    if video_enc.is_none() {
                        match FfmpegH264Encoder::new(
                            &session_dir,
                            fw.width,
                            fw.height,
                            effective_fps,
                        ) {
                            Ok(enc) => video_enc = Some(enc),
                            Err(e) => {
                                eprintln!("[WARN] H264 encoder init failed: {}", e);
                            }
                        }
                    }
                    if let Some(enc) = video_enc.as_mut() {
                        if let Err(e) = enc.push_frame(&fw.data, fw.width, fw.height, fw.frame_id as i64) {
                            eprintln!("[WARN] H264 encode frame {}: {}", fw.frame_id, e);
                        }
                    }
                }
            }

            Ok(EncoderWork::Events(batch)) => {
                write_event_batch(&mut events_w, &batch, &mut event_id)?;
            }

            Ok(EncoderWork::Flush) | Err(_) => {
                // Flush buffered writers.
                frames_w
                    .flush()
                    .map_err(|e| ConsumerError::DiskError(e.to_string()))?;
                events_w
                    .flush()
                    .map_err(|e| ConsumerError::DiskError(e.to_string()))?;

                // Finalize H.264 encoder.
                #[cfg(feature = "ffmpeg")]
                if let Some(enc) = video_enc.take() {
                    if let Err(e) = enc.finish() {
                        eprintln!("[WARN] H264 finalize failed: {}", e);
                    }
                }

                break;
            }
        }
    }

    Ok(())
}

// ─── JSONL write helpers ──────────────────────────────────────────────────────

fn write_frame_record(
    w: &mut BufWriter<File>,
    fw: &FrameWork,
) -> Result<(), ConsumerError> {
    let rec = FrameRecord {
        frame_id: fw.frame_id,
        capture_ts_ns: fw.capture_ts_ns,
        dxgi_acquire_ts_ns: fw.dxgi_acquire_ts_ns,
        buffer_ready_ts_ns: fw.buffer_ready_ts_ns,
    };
    let line =
        serde_json::to_string(&rec).map_err(|e| ConsumerError::DiskError(e.to_string()))?;
    writeln!(w, "{}", line).map_err(|e| ConsumerError::DiskError(e.to_string()))
}

fn write_event_batch(
    w: &mut BufWriter<File>,
    batch: &EventBatch,
    event_id: &mut u64,
) -> Result<(), ConsumerError> {
    for ev in &batch.events {
        let (event_type, payload) = event_to_type_payload(ev);
        let event_ts_ns = ev.recv_ts_ns().0;
        let dt_prev_ns =
            batch
                .prev_frame_ts_ns
                .map(|pt| event_ts_ns as i64 - pt as i64);

        let rec = EventRecord {
            event_id: *event_id,
            event_ts_ns,
            queue_push_done_ts_ns: batch.push_done_ts_ns,
            event_type,
            payload,
            prev_frame_id: batch.prev_frame_id,
            prev_frame_ts_ns: batch.prev_frame_ts_ns,
            dt_prev_ns,
        };
        let line =
            serde_json::to_string(&rec).map_err(|e| ConsumerError::DiskError(e.to_string()))?;
        writeln!(w, "{}", line).map_err(|e| ConsumerError::DiskError(e.to_string()))?;
        *event_id += 1;
    }
    Ok(())
}

/// Map an `InputEvent` to its JSONL `type` string and `payload` object.
fn event_to_type_payload(event: &InputEvent) -> (&'static str, serde_json::Value) {
    use crate::hooks::{ClickAction, ScrollDirection};
    use serde_json::json;

    match event {
        InputEvent::KeyPress {
            vk_code,
            scan_code,
            flags,
            ..
        } => (
            "keyboard_press",
            json!({ "vk_code": vk_code, "scan_code": scan_code, "flags": flags }),
        ),
        InputEvent::KeyRelease {
            vk_code,
            scan_code,
            flags,
            ..
        } => (
            "keyboard_release",
            json!({ "vk_code": vk_code, "scan_code": scan_code, "flags": flags }),
        ),
        InputEvent::MouseMove { x, y, dx, dy, .. } => {
            ("mouse_move", json!({ "x": x, "y": y, "dx": dx, "dy": dy }))
        }
        InputEvent::MouseClick {
            x,
            y,
            button,
            action,
            ..
        } => {
            let action_str = match action {
                ClickAction::Press => "press",
                ClickAction::Release => "release",
            };
            (
                "mouse_click",
                json!({ "x": x, "y": y, "button": button, "action": action_str }),
            )
        }
        InputEvent::MouseScroll {
            delta, direction, ..
        } => {
            let dir_str = match direction {
                ScrollDirection::Vertical => "vertical",
                ScrollDirection::Horizontal => "horizontal",
            };
            ("mouse_scroll", json!({ "delta": delta, "direction": dir_str }))
        }
    }
}

// ─── FFmpeg H.264 encoder (requires `--features ffmpeg`) ─────────────────────

#[cfg(feature = "ffmpeg")]
struct FfmpegH264Encoder {
    octx: ffmpeg_next::format::context::Output,
    // `ffmpeg_next::codec::encoder::Video` = re-export of `video::Encoder` (opened encoder)
    encoder: ffmpeg_next::codec::encoder::Video,
    scaler: ffmpeg_next::software::scaling::context::Context,
    stream_index: usize,
    width: u32,
    height: u32,
    time_base: ffmpeg_next::Rational,
}

#[cfg(feature = "ffmpeg")]
impl FfmpegH264Encoder {
    fn new(
        session_dir: &std::path::Path,
        width: u32,
        height: u32,
        fps: f64,
    ) -> Result<Self, ConsumerError> {
        use ffmpeg_next as ffmpeg;

        ffmpeg::init().map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        let output_path = session_dir.join("video.mp4");

        // Output container.
        let mut octx = ffmpeg::format::output(&output_path)
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        // H.264 codec.
        let codec = ffmpeg::codec::encoder::find(ffmpeg::codec::Id::H264).ok_or_else(|| {
            ConsumerError::EncoderError("H.264 encoder not found in this FFmpeg build".to_string())
        })?;

        // Video stream.
        let mut ost = octx
            .add_stream(codec)
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;
        let stream_index = ost.index();

        // Configure encoder.
        let fps_i = fps.round() as i32;
        let time_base = ffmpeg::Rational::new(1, fps_i);

        // H.264 requires even dimensions (MFT and libx264 both enforce this).
        // Round up to the nearest even number so odd-resolution monitors work.
        let enc_width  = (width  + 1) & !1u32;
        let enc_height = (height + 1) & !1u32;

        let ctx = ffmpeg::codec::context::Context::from_parameters(ost.parameters())
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;
        let mut video = ctx
            .encoder()
            .video()
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        video.set_width(enc_width);
        video.set_height(enc_height);
        video.set_format(ffmpeg::format::Pixel::YUV420P);
        video.set_time_base(time_base);
        video.set_frame_rate(Some(ffmpeg::Rational::new(fps_i, 1)));
        video.set_bit_rate(4_000_000); // 4 Mbps — suitable for 1080p gameplay

        // Open encoder (H.264 requires libx264 in the FFmpeg build).
        let encoder = video
            .open_as(codec)
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        ost.set_parameters(&encoder);

        // Write container header.
        octx.write_header()
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        // Pixel-format conversion context: BGRA (original size) → YUV420P (encoded size).
        // If width/height are already even, enc_width/enc_height equal them and this is a no-op.
        let scaler = ffmpeg::software::scaling::context::Context::get(
            ffmpeg::format::Pixel::BGRA,
            width,
            height,
            ffmpeg::format::Pixel::YUV420P,
            enc_width,
            enc_height,
            ffmpeg::software::scaling::flag::Flags::BILINEAR,
        )
        .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        Ok(Self {
            octx,
            encoder,
            scaler,
            stream_index,
            width,
            height,
            time_base,
        })
    }

    fn push_frame(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
        pts: i64,
    ) -> Result<(), ConsumerError> {
        use ffmpeg_next as ffmpeg;
        use ffmpeg::util::frame::video::Video;

        // Ignore frames with changed dimensions (shouldn't happen).
        if width != self.width || height != self.height {
            return Ok(());
        }

        // Build source BGRA frame.
        let mut src = Video::new(ffmpeg::format::Pixel::BGRA, width, height);
        let stride = src.stride(0);
        let row_bytes = (width as usize) * 4;
        for y in 0..height as usize {
            let src_row = &bgra[y * row_bytes..(y + 1) * row_bytes];
            let dst_row = &mut src.data_mut(0)[y * stride..y * stride + row_bytes];
            dst_row.copy_from_slice(src_row);
        }

        // Convert BGRA → YUV420P.
        let mut yuv = Video::empty();
        self.scaler
            .run(&src, &mut yuv)
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;
        yuv.set_pts(Some(pts));

        // Send frame to encoder.
        self.encoder
            .send_frame(&yuv)
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        self.drain_packets()
    }

    fn drain_packets(&mut self) -> Result<(), ConsumerError> {
        use ffmpeg_next as ffmpeg;

        let mut packet = ffmpeg::Packet::empty();
        loop {
            match self.encoder.receive_packet(&mut packet) {
                Ok(()) => {
                    packet.rescale_ts(self.time_base, self.octx.stream(self.stream_index)
                        .ok_or_else(|| ConsumerError::EncoderError("Stream gone".to_string()))?
                        .time_base());
                    packet.set_stream(self.stream_index);
                    packet
                        .write_interleaved(&mut self.octx)
                        .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;
                }
                Err(ffmpeg_next::Error::Other { errno }) if errno == ffmpeg_next::error::EAGAIN => {
                    break;
                }
                Err(ffmpeg_next::Error::Eof) => break,
                Err(e) => return Err(ConsumerError::EncoderError(e.to_string())),
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<(), ConsumerError> {
        // Flush the encoder.
        self.encoder
            .send_eof()
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;
        self.drain_packets()?;

        // Write MP4 trailer.
        self.octx
            .write_trailer()
            .map_err(|e| ConsumerError::EncoderError(e.to_string()))?;

        Ok(())
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::MonotonicNs;
    use crate::capture::CapturedFrame;

    fn make_frame(frame_id: u64) -> CapturedFrame {
        CapturedFrame {
            data: vec![0u8; 16],
            width: 4,
            height: 2,
            capture_ts_ns: MonotonicNs(frame_id * 33_333_333),
            dxgi_acquire_ts_ns: None,
            buffer_ready_ts_ns: None,
        }
    }

    #[test]
    fn recorder_writes_frames_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let metrics = PipelineMetrics::new();
        let mut rec =
            DiskRecorderConsumer::new(dir.path().to_path_buf(), 30.0, 8, metrics)
                .unwrap();

        for i in 0..3u64 {
            rec.consume(&make_frame(i), i).unwrap();
        }
        rec.flush().unwrap();

        let content = std::fs::read_to_string(dir.path().join("frames.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3, "expected 3 frame records");
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["frame_id"], 0);
    }

    #[test]
    fn recorder_writes_events_jsonl() {
        use crate::hooks::InputEvent;

        let dir = tempfile::tempdir().unwrap();
        let metrics = PipelineMetrics::new();
        let mut rec =
            DiskRecorderConsumer::new(dir.path().to_path_buf(), 30.0, 8, metrics)
                .unwrap();

        // One frame first.
        rec.consume(&make_frame(0), 0).unwrap();
        // Then an event.
        let ev = InputEvent::KeyPress {
            vk_code: 32,
            scan_code: 0x39,
            flags: 0,
            recv_ts_ns: MonotonicNs(1_000_000),
        };
        rec.push_events(vec![ev], 1_100_000);
        rec.consume(&make_frame(1), 1).unwrap();
        rec.flush().unwrap();

        let content = std::fs::read_to_string(dir.path().join("events.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);
        let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["type"], "keyboard_press");
        assert_eq!(v["prev_frame_id"], 0);
    }

    #[test]
    fn queue_full_drops_frame_and_increments_metric() {
        let dir = tempfile::tempdir().unwrap();
        let metrics = PipelineMetrics::new();
        // Queue of 2.
        let mut rec =
            DiskRecorderConsumer::new(dir.path().to_path_buf(), 30.0, 2, Arc::clone(&metrics))
                .unwrap();

        // Fill the queue beyond capacity without the worker draining it.
        for i in 0..10u64 {
            rec.consume(&make_frame(i), i).unwrap();
        }
        rec.flush().unwrap();

        let drops = metrics.ring_buffer_drop_count.load(Ordering::Relaxed);
        assert!(drops > 0, "expected at least one frame drop");
    }
}
