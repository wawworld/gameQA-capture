//! `RingBufferConsumer` — lock-free MPSC ring buffer for real-time bot consumers.
//!
//! Uses `thingbuf` with drop-oldest semantics.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use crate::pipeline::consumer::{ConsumerError, FrameConsumer};
use crate::pipeline::metrics::PipelineMetrics;
use std::sync::Arc;
use std::time::Duration;

/// Reader handle given to the bot consumer.
///
/// Obtained by calling `RingBufferConsumer::reader()` at session start.
pub struct FrameChannel {
    // TODO(T046): hold thingbuf::mpsc::Receiver<CapturedFrame> here.
}

impl FrameChannel {
    /// Non-blocking read. Returns `None` if no new frame is available.
    pub fn try_recv(&self) -> Option<CapturedFrame> {
        // TODO(T046): delegate to thingbuf receiver.
        None
    }

    /// Blocking read with timeout. Returns `None` on timeout.
    pub fn recv_timeout(&self, _timeout: Duration) -> Option<CapturedFrame> {
        // TODO(T046): delegate to thingbuf receiver with timeout.
        None
    }
}

/// MPSC ring buffer consumer with drop-oldest semantics.
///
/// When full, the oldest frame is overwritten and
/// `PipelineMetrics.ring_buffer_drop_count` is incremented.
#[allow(dead_code)]
pub struct RingBufferConsumer {
    capacity: usize,
    metrics: Arc<PipelineMetrics>,
    // TODO(T045): hold thingbuf::mpsc::Sender<CapturedFrame> here.
}

impl RingBufferConsumer {
    pub fn new(capacity: usize, metrics: Arc<PipelineMetrics>) -> Self {
        Self { capacity, metrics }
    }

    /// Create a `FrameChannel` reader for this ring buffer.
    ///
    /// Call once at session start; hand the handle to the bot consumer.
    pub fn reader(&self) -> FrameChannel {
        // TODO(T046): return the thingbuf receiver wrapped in FrameChannel.
        FrameChannel {}
    }
}

impl FrameConsumer for RingBufferConsumer {
    fn consume(&mut self, _frame: &CapturedFrame, _frame_id: u64) -> Result<(), ConsumerError> {
        // TODO(T045): non-blocking push to ring buffer; drop-oldest on full.
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ConsumerError> {
        // Ring buffer has no persistent state — no-op.
        Ok(())
    }
}
