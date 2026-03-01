//! `FrameConsumer` trait — sink for captured frames.
//!
//! Multiple consumers can be active simultaneously (recording + real-time).
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use crate::hooks::InputEvent;

/// Errors a `FrameConsumer` implementation may return.
#[derive(Debug, thiserror::Error)]
pub enum ConsumerError {
    #[error("Disk write failed: {0}")]
    DiskError(String),
    #[error("Encoder error: {0}")]
    EncoderError(String),
    #[error("Consumer dropped: channel closed")]
    Dropped,
    #[error("Other: {0}")]
    Other(String),
}

/// A sink that receives captured frames for processing or distribution.
///
/// The pipeline calls `consume` for every frame in capture order.
/// Multiple consumers can be active simultaneously (recording + real-time).
pub trait FrameConsumer: Send {
    /// Process one captured frame.
    ///
    /// MUST return quickly (ideally < 0.5 ms) to avoid blocking the capture thread.
    /// Heavy work (disk I/O, encoding) MUST be offloaded to a worker thread inside
    /// the consumer implementation.
    ///
    /// `frame_id` is the pipeline-assigned monotonic sequence number for this frame.
    fn consume(&mut self, frame: &CapturedFrame, frame_id: u64) -> Result<(), ConsumerError>;

    /// Flush any buffered data and finalize output.
    ///
    /// Called once when the session ends (normal or error). MUST complete all
    /// pending writes before returning.
    fn flush(&mut self) -> Result<(), ConsumerError>;

    /// Forward input events that arrived since the previous frame.
    ///
    /// `push_done_ts_ns` is the monotonic nanosecond timestamp at which the
    /// push completes (used to compute queue-push latency in event records).
    ///
    /// Default implementation is a no-op — only recording consumers override this.
    fn push_events(&mut self, _events: Vec<InputEvent>, _push_done_ts_ns: u64) {}
}
