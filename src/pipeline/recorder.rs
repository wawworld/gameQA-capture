//! `DiskRecorderConsumer` — off-thread H.264/MP4 encoding + JSONL writers.
//!
//! Heavy encoding work is offloaded to a dedicated worker thread.
//! The capture thread is NEVER blocked by disk I/O or encoding.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use crate::pipeline::consumer::{ConsumerError, FrameConsumer};
use crate::pipeline::metrics::PipelineMetrics;
use std::path::PathBuf;
use std::sync::Arc;

/// `DiskRecorderConsumer` — writes `video.mp4`, `frames.jsonl`, and `events.jsonl`.
///
/// `consume()` pushes the frame to a bounded encoder queue (non-blocking).
/// `flush()` drains the queue and finalizes all files.
#[allow(dead_code)]
pub struct DiskRecorderConsumer {
    session_dir: PathBuf,
    metrics: Arc<PipelineMetrics>,
    // TODO(T032): hold encoder thread handle and sender here.
}

impl DiskRecorderConsumer {
    pub fn new(session_dir: PathBuf, metrics: Arc<PipelineMetrics>) -> Self {
        Self {
            session_dir,
            metrics,
        }
    }
}

impl FrameConsumer for DiskRecorderConsumer {
    fn consume(&mut self, _frame: &CapturedFrame, _frame_id: u64) -> Result<(), ConsumerError> {
        // TODO(T032): push frame to bounded encoder queue.
        // If queue is full, drop and increment metrics.queue_depth.
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ConsumerError> {
        // TODO(T032): drain encoder queue, finalize MP4, flush JSONL files.
        Ok(())
    }
}
