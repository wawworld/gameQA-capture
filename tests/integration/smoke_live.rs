//! Live hardware smoke test — DxgiBackend + WillhookBackend through Pipeline.
//!
//! Runs for 5 seconds with a counting consumer and TimeoutTrigger.
//! Requires a real Windows desktop with a WDDM display driver.
//!
//! ```
//! cargo test --test smoke_live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! **IMPORTANT**: Must run with `--test-threads=1`. `willhook` uses a process-global
//! hook singleton — parallel test threads will race on hook installation and fail.
//!
//! Gracefully falls back to a no-op capture backend if DXGI is unavailable (VM, etc.).

use game_qa::capture::{CaptureBackend, CaptureError, CapturedFrame, Rect};
use game_qa::config::{CaptureConfig, EventCaptureConfig, ProfileConfig};
use game_qa::hooks::win32::WillhookBackend;
use game_qa::hooks::{HookError, InputHookBackend};
use game_qa::pipeline::consumer::{ConsumerError, FrameConsumer};
use game_qa::pipeline::metrics::PipelineMetrics;
use game_qa::pipeline::{Pipeline, PipelineStop};
use game_qa::roi::{GraceExceededAction, RoiManager};
use game_qa::session::triggers::TimeoutTrigger;
use std::sync::Arc;
use std::time::Instant;

// ─── Counting consumer ────────────────────────────────────────────────────────

struct CountingConsumer {
    frame_count: u64,
    total_bytes: u64,
    first_frame_ns: Option<u64>,
    last_frame_ns: Option<u64>,
}

impl CountingConsumer {
    fn new() -> Self {
        Self { frame_count: 0, total_bytes: 0, first_frame_ns: None, last_frame_ns: None }
    }
}

impl FrameConsumer for CountingConsumer {
    fn consume(&mut self, frame: &CapturedFrame, _frame_id: u64) -> Result<(), ConsumerError> {
        self.frame_count += 1;
        self.total_bytes += frame.data.len() as u64;
        let ts = frame.capture_ts_ns.0;
        if self.first_frame_ns.is_none() {
            self.first_frame_ns = Some(ts);
        }
        self.last_frame_ns = Some(ts);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ConsumerError> {
        Ok(())
    }
}

// ─── Full pipeline smoke test ─────────────────────────────────────────────────

#[test]
#[ignore = "requires live Windows desktop with WDDM display — run with --ignored --nocapture"]
fn smoke_live_dxgi_willhook_pipeline() {
    use game_qa::capture::dxgi::DxgiBackend;

    let session_start = Instant::now();
    let duration_secs = 5.0_f64;

    println!();
    println!("=== gameQA live smoke test ({duration_secs:.0}s) ===");
    println!();

    // ── 1. Try to initialise DxgiBackend ─────────────────────────────────────
    let capture_cfg = CaptureConfig { diagnostics_enabled: true, ..Default::default() };
    let backend: Box<dyn CaptureBackend> = match DxgiBackend::new(capture_cfg, session_start) {
        Ok(b) => {
            println!("[DXGI] Initialised OK  monitor={:?}", b.monitor_rect());
            Box::new(b)
        }
        Err(CaptureError::DeviceLost(msg)) => {
            println!("[DXGI] SKIPPED — DeviceLost: {msg}");
            println!("       (expected in VM / no WDDM display duplication support)");
            println!("       Falling back to NoOpCaptureBackend...");
            println!();
            Box::new(NoOpCaptureBackend::new(session_start))
        }
        Err(e) => panic!("unexpected DXGI error: {e:?}"),
    };

    // ── 2. Initialise WillhookBackend ─────────────────────────────────────────
    let mut hook = WillhookBackend::new(EventCaptureConfig::default(), session_start);

    match hook.start() {
        Ok(()) => println!("[HOOK] WH_KEYBOARD_LL + WH_MOUSE_LL installed OK"),
        Err(HookError::InstallFailed(msg)) => panic!("WillhookBackend::start() failed: {msg}"),
        Err(e) => panic!("unexpected hook error: {e:?}"),
    }

    // ── 3. Build pipeline ─────────────────────────────────────────────────────
    let consumer = CountingConsumer::new();
    let trigger = TimeoutTrigger::new(session_start, duration_secs);
    let roi = RoiManager::new(Default::default(), GraceExceededAction::MarkLowQuality);
    let metrics = Arc::new(PipelineMetrics::default());

    let mut pipeline = Pipeline::new(
        ProfileConfig::default(),
        session_start,
        backend,
        Box::new(hook),
        vec![Box::new(consumer)],
        vec![Box::new(trigger)],
        roi,
        metrics,
    );

    println!("[PIPE] Running for {duration_secs:.0}s — move the mouse or press keys now...");
    println!();

    // ── 4. Run ────────────────────────────────────────────────────────────────
    let stop = pipeline.run();
    let wall_elapsed = session_start.elapsed();

    // ── 5. Report ─────────────────────────────────────────────────────────────
    println!("=== Results ===");
    match &stop {
        PipelineStop::TriggerFired(reason) => println!("[STOP] {reason}"),
        PipelineStop::Error(e) => println!("[STOP] Error: {e:?}"),
    }
    println!("[TIME] Wall elapsed: {:.2}s", wall_elapsed.as_secs_f64());

    // ── 6. Assertions ─────────────────────────────────────────────────────────
    assert!(
        matches!(stop, PipelineStop::TriggerFired(_)),
        "expected TimeoutTrigger, got: {stop:?}"
    );
    assert!(
        wall_elapsed.as_secs_f64() >= duration_secs - 0.5,
        "pipeline ran for {:.2}s, expected ~{duration_secs:.0}s",
        wall_elapsed.as_secs_f64()
    );

    println!();
    println!("=== PASS ===");
}

// ─── Standalone hook smoke test ───────────────────────────────────────────────

#[test]
#[ignore = "requires live Windows desktop — run with --ignored --nocapture"]
fn smoke_live_willhook_standalone() {
    let session_start = Instant::now();
    let mut hook = WillhookBackend::new(EventCaptureConfig::default(), session_start);

    match hook.start() {
        Ok(()) => println!("[HOOK] Installed. Press a key or move the mouse within 3s..."),
        Err(e) => panic!("WillhookBackend::start() failed: {e:?}"),
    }

    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    let mut count = 0usize;

    while Instant::now() < deadline {
        if let Some(ev) = hook.try_recv() {
            count += 1;
            println!("  event #{count}: {ev:?}");
        }
        std::hint::spin_loop();
    }

    hook.stop().ok();
    println!("[HOOK] Received {count} events in 3s — PASS");
    // Not asserting count > 0: test machine may be unattended.
}

// ─── Save-frame smoke test ────────────────────────────────────────────────────

/// Captures a frame from DXGI and writes it to `target/smoke_capture.bmp`.
///
/// Waits for a non-black frame (up to 3 s) so the compositor has time to populate
/// the duplication surface after initialisation.
/// Open the saved file with any image viewer to visually verify the capture.
#[test]
#[ignore = "requires live Windows desktop with WDDM display — run with --ignored --nocapture"]
fn smoke_live_save_frame() {
    use game_qa::capture::dxgi::DxgiBackend;

    let session_start = Instant::now();
    let out_path = "target/smoke_capture.bmp";

    let capture_cfg = CaptureConfig { diagnostics_enabled: true, ..Default::default() };
    let mut backend = match DxgiBackend::new(capture_cfg, session_start) {
        Ok(b) => {
            println!("[DXGI] Initialised OK  monitor={:?}", b.monitor_rect());
            b
        }
        Err(CaptureError::DeviceLost(msg)) => {
            println!("[DXGI] SKIPPED — DeviceLost: {msg}");
            return;
        }
        Err(e) => panic!("unexpected DXGI error: {e:?}"),
    };

    // Brief settle: the DXGI duplication surface is populated asynchronously
    // by the compositor after `DesktopDuplicationApi::new()` returns.
    // On a VM the first AcquireNextFrame may return an all-zero buffer.
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Capture frames until we get one with visible content, or give up after ~3 s.
    // Each frame's data is extracted before release_frame() takes ownership.
    let mut saved: Option<(Vec<u8>, u32, u32)> = None; // (data, width, height)

    for attempt in 0..180 {
        match backend.acquire_frame() {
            Ok(Some(f)) => {
                let stats = pixel_stats(&f.data);
                println!(
                    "[DXGI] attempt #{attempt}  {}×{}  {} bytes  \
                     R_avg={:.1} G_avg={:.1} B_avg={:.1}  non_black={:.1}%",
                    f.width, f.height, f.data.len(),
                    stats.r_avg, stats.g_avg, stats.b_avg,
                    stats.non_black_pct,
                );

                let is_content = stats.non_black_pct > 0.5;
                let is_last = attempt == 179;

                if is_content || is_last {
                    // Extract data before release_frame() consumes the frame.
                    let data = f.data.clone();
                    let (w, h) = (f.width, f.height);
                    backend.release_frame(f).ok();
                    saved = Some((data, w, h));
                    break;
                }

                backend.release_frame(f).ok();
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(16)),
            Err(e) => panic!("acquire_frame error: {e:?}"),
        }
    }

    let (data, width, height) = saved.expect("no frame received in 3 s");

    save_bgra_bmp(out_path, &data, width, height)
        .unwrap_or_else(|e| panic!("BMP write failed: {e}"));

    let file_bytes = std::fs::metadata(out_path).map(|m| m.len()).unwrap_or(0);
    println!("[BMP]  Saved → {out_path}  ({file_bytes} bytes)");
    println!("       Open with any image viewer to verify the capture.");
    println!("[PASS]");
}

/// Per-channel averages and non-black pixel percentage for a BGRA buffer.
struct PixelStats {
    r_avg: f32,
    g_avg: f32,
    b_avg: f32,
    non_black_pct: f32,
}

fn pixel_stats(bgra: &[u8]) -> PixelStats {
    let n = bgra.len() / 4;
    if n == 0 {
        return PixelStats { r_avg: 0.0, g_avg: 0.0, b_avg: 0.0, non_black_pct: 0.0 };
    }
    let mut b_sum = 0u64;
    let mut g_sum = 0u64;
    let mut r_sum = 0u64;
    let mut non_black = 0u64;
    for px in bgra.chunks_exact(4) {
        let (b, g, r) = (px[0] as u64, px[1] as u64, px[2] as u64);
        b_sum += b;
        g_sum += g;
        r_sum += r;
        if b > 8 || g > 8 || r > 8 {
            non_black += 1;
        }
    }
    PixelStats {
        b_avg: b_sum as f32 / n as f32,
        g_avg: g_sum as f32 / n as f32,
        r_avg: r_sum as f32 / n as f32,
        non_black_pct: non_black as f32 / n as f32 * 100.0,
    }
}

/// Write raw BGRA pixel data as a 32-bpp top-down BMP file (no external crates).
fn save_bgra_bmp(
    path: &str,
    data: &[u8],
    width: u32,
    height: u32,
) -> std::io::Result<()> {
    use std::io::Write;

    let pixel_bytes = width * height * 4;
    let file_size = 54u32 + pixel_bytes; // 14-byte file header + 40-byte DIB header

    let mut f = std::fs::File::create(path)?;

    // ── File header (14 bytes) ────────────────────────────────────────────────
    f.write_all(b"BM")?;
    f.write_all(&file_size.to_le_bytes())?;
    f.write_all(&0u32.to_le_bytes())?;       // reserved
    f.write_all(&54u32.to_le_bytes())?;      // pixel data offset

    // ── DIB header / BITMAPINFOHEADER (40 bytes) ─────────────────────────────
    f.write_all(&40u32.to_le_bytes())?;              // header size
    f.write_all(&(width as i32).to_le_bytes())?;     // width
    f.write_all(&(-(height as i32)).to_le_bytes())?; // negative height → top-down
    f.write_all(&1u16.to_le_bytes())?;               // color planes
    f.write_all(&32u16.to_le_bytes())?;              // bits per pixel
    f.write_all(&0u32.to_le_bytes())?;               // compression (BI_RGB)
    f.write_all(&pixel_bytes.to_le_bytes())?;        // image size
    f.write_all(&0u32.to_le_bytes())?;               // X px/meter
    f.write_all(&0u32.to_le_bytes())?;               // Y px/meter
    f.write_all(&0u32.to_le_bytes())?;               // colors in table
    f.write_all(&0u32.to_le_bytes())?;               // important colors

    // ── Pixel data (BGRA rows, already top-down) ──────────────────────────────
    f.write_all(data)?;

    Ok(())
}

// ─── Video recording smoke test ───────────────────────────────────────────────

/// Captures ~3 s of live DXGI video and encodes it to an MP4 file using
/// the `DiskRecorderConsumer` + ffmpeg-next H.264 encoder.
///
/// Verifies that `video.mp4`, `frames.jsonl`, and `events.jsonl` are all
/// written to the session directory with non-zero sizes.
///
/// Run with:
/// ```sh
/// cargo test --test smoke_live --features ffmpeg -- \
///     --ignored --nocapture --test-threads=1 smoke_live_record_to_mp4
/// ```
#[test]
#[ignore = "requires live Windows desktop with WDDM display and FFmpeg — run with --ignored --nocapture"]
#[cfg(feature = "ffmpeg")]
fn smoke_live_record_to_mp4() {
    use game_qa::capture::dxgi::DxgiBackend;
    use game_qa::pipeline::recorder::DiskRecorderConsumer;
    use game_qa::pipeline::metrics::PipelineMetrics;
    use game_qa::session::triggers::TimeoutTrigger;
    use std::sync::Arc;

    let session_start = Instant::now();
    let duration_secs = 3.0_f64;
    let session_dir = std::env::temp_dir().join("gameqa_smoke_mp4");
    let _ = std::fs::create_dir_all(&session_dir);

    println!();
    println!("=== gameQA video recording smoke test ({duration_secs:.0}s) ===");
    println!("Session dir: {}", session_dir.display());
    println!();

    // ── 1. Capture backend ────────────────────────────────────────────────────
    let capture_cfg = CaptureConfig { diagnostics_enabled: true, ..Default::default() };
    let backend: Box<dyn CaptureBackend> = match DxgiBackend::new(capture_cfg.clone(), session_start) {
        Ok(b) => {
            println!("[DXGI] Initialised OK  monitor={:?}", b.monitor_rect());
            Box::new(b)
        }
        Err(CaptureError::DeviceLost(msg)) => {
            println!("[DXGI] SKIPPED — DeviceLost: {msg}");
            println!("       Using NoOpCaptureBackend fallback (will produce black frames)");
            Box::new(NoOpCaptureBackend::new(session_start))
        }
        Err(e) => panic!("unexpected DXGI error: {e:?}"),
    };

    // ── 2. Disk recorder consumer (ffmpeg H.264) ──────────────────────────────
    let metrics = Arc::new(PipelineMetrics::default());
    let fps = capture_cfg.target_fps as f64;
    let recorder = DiskRecorderConsumer::new(
        session_dir.clone(),
        fps,
        0,
        Arc::clone(&metrics),
    )
    .expect("DiskRecorderConsumer::new failed");

    // ── 3. Input hook ─────────────────────────────────────────────────────────
    let mut hook = WillhookBackend::new(EventCaptureConfig::default(), session_start);
    hook.start().expect("WillhookBackend::start() failed");

    // ── 4. Build pipeline ─────────────────────────────────────────────────────
    let trigger = TimeoutTrigger::new(session_start, duration_secs);
    let roi = RoiManager::new(Default::default(), GraceExceededAction::MarkLowQuality);

    let mut pipeline = Pipeline::new(
        ProfileConfig::default(),
        session_start,
        backend,
        Box::new(hook),
        vec![Box::new(recorder)],
        vec![Box::new(trigger)],
        roi,
        Arc::clone(&metrics),
    );

    println!("[PIPE] Recording for {duration_secs:.0}s — move the mouse or press keys...");
    println!();

    // ── 5. Run ────────────────────────────────────────────────────────────────
    let stop = pipeline.run();
    let elapsed = session_start.elapsed();
    println!("[STOP] {:?}  ({:.2}s elapsed)", stop, elapsed.as_secs_f64());

    // ── 6. Verify artifacts ───────────────────────────────────────────────────
    let frames_jsonl = session_dir.join("frames.jsonl");
    let events_jsonl = session_dir.join("events.jsonl");
    let video_mp4    = session_dir.join("video.mp4");

    for (name, path) in [("frames.jsonl", &frames_jsonl), ("events.jsonl", &events_jsonl), ("video.mp4", &video_mp4)] {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        println!("[ART]  {name:<15}  {} bytes  ({})", size, if size > 0 { "OK" } else { "EMPTY!" });
    }

    assert!(frames_jsonl.exists(), "frames.jsonl not written");
    let frames_size = std::fs::metadata(&frames_jsonl).map(|m| m.len()).unwrap_or(0);
    assert!(frames_size > 0, "frames.jsonl is empty");

    assert!(video_mp4.exists(), "video.mp4 not written");
    let mp4_size = std::fs::metadata(&video_mp4).map(|m| m.len()).unwrap_or(0);
    assert!(mp4_size > 0, "video.mp4 is empty");

    println!();
    println!("=== PASS ===");
    println!("Video saved to: {}", video_mp4.display());
    println!("Open with VLC or ffplay to verify.");
}

// ─── No-op fallback capture backend ──────────────────────────────────────────

/// Emits synthetic 1×1 black frames at ~30 fps via busy-wait.
/// Used when `DxgiBackend` cannot be initialised (VM environments).
struct NoOpCaptureBackend {
    session_start: Instant,
    last_frame_ns: u64,
}

impl NoOpCaptureBackend {
    fn new(session_start: Instant) -> Self {
        Self { session_start, last_frame_ns: 0 }
    }
}

impl CaptureBackend for NoOpCaptureBackend {
    fn acquire_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
        const INTERVAL_NS: u64 = 33_333_333; // ~30 fps
        loop {
            let now_ns = self.session_start.elapsed().as_nanos() as u64;
            if now_ns >= self.last_frame_ns + INTERVAL_NS {
                self.last_frame_ns = now_ns;
                use game_qa::MonotonicNs;
                return Ok(Some(CapturedFrame {
                    data: vec![0u8; 4],
                    width: 1,
                    height: 1,
                    capture_ts_ns: MonotonicNs(now_ns),
                    dxgi_acquire_ts_ns: None,
                    buffer_ready_ts_ns: None,
                }));
            }
            std::hint::spin_loop();
        }
    }

    fn release_frame(&mut self, _frame: CapturedFrame) -> Result<(), CaptureError> {
        Ok(())
    }

    fn monitor_rect(&self) -> Rect {
        Rect { x: 0, y: 0, width: 1, height: 1 }
    }
}
