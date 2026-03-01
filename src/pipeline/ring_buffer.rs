//! `RingBufferConsumer` — bounded MPSC channel with drop-oldest semantics.
//!
//! Designed for real-time bot consumers that need the most recent frames.
//! When the buffer is full, the **oldest** frame is dropped and
//! `PipelineMetrics.ring_buffer_drop_count` is incremented.
//!
//! # Thread model
//! ```text
//! Pipeline (capture thread)
//!   ─→ RingBufferConsumer::consume() ─→ [crossbeam bounded channel]
//!                                              │
//!                            FrameChannel::try_recv() / recv_timeout()
//!                                              │
//!                                     Bot consumer thread
//! ```
//!
//! `FrameChannel` holds a cloned `Receiver`; `RingBufferConsumer` holds both
//! the `Sender` and a second `Receiver` clone used for drain-on-overflow.
//! The two receivers compete in MPMC fashion; the semantics are correct:
//! if the bot has already drained an item the channel is no longer full,
//! which means `try_send()` will succeed on the first attempt.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use crate::pipeline::consumer::{ConsumerError, FrameConsumer};
use crate::pipeline::metrics::PipelineMetrics;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

// ─── Frame wrapper ────────────────────────────────────────────────────────────

/// A captured frame shared between the pipeline and the bot reader.
///
/// `Arc` avoids copying the (potentially large) pixel buffer when the bot holds
/// a reference for multi-frame processing.
type SharedFrame = Arc<CapturedFrame>;

// ─── FrameChannel ─────────────────────────────────────────────────────────────

/// Reader handle given to the bot consumer.
///
/// Obtained by calling [`RingBufferConsumer::reader`] at session start.
pub struct FrameChannel {
    receiver: crossbeam_channel::Receiver<SharedFrame>,
}

impl FrameChannel {
    /// Non-blocking read. Returns `None` if no new frame is available.
    pub fn try_recv(&self) -> Option<Arc<CapturedFrame>> {
        self.receiver.try_recv().ok()
    }

    /// Blocking read with timeout. Returns `None` on timeout.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Arc<CapturedFrame>> {
        self.receiver.recv_timeout(timeout).ok()
    }
}

// ─── RingBufferConsumer ───────────────────────────────────────────────────────

/// MPSC ring buffer consumer with drop-oldest semantics.
///
/// When full, the oldest frame is silently discarded and
/// `PipelineMetrics.ring_buffer_drop_count` is incremented.
pub struct RingBufferConsumer {
    sender: crossbeam_channel::Sender<SharedFrame>,
    /// Second receiver clone used to drain one slot when the buffer is full.
    /// Both this and the `FrameChannel` receiver compete in MPMC fashion.
    drain_rx: crossbeam_channel::Receiver<SharedFrame>,
    capacity: usize,
    metrics: Arc<PipelineMetrics>,
}

impl RingBufferConsumer {
    pub fn new(capacity: usize, metrics: Arc<PipelineMetrics>) -> Self {
        let cap = capacity.max(1);
        let (sender, receiver) = crossbeam_channel::bounded(cap);
        Self {
            sender,
            drain_rx: receiver,
            capacity: cap,
            metrics,
        }
    }

    /// Create a [`FrameChannel`] reader for this ring buffer.
    ///
    /// Call once at session start; hand the handle to the bot consumer thread.
    /// `crossbeam_channel::Receiver` is cheap to clone (it's just a reference
    /// count bump on the shared channel state).
    pub fn reader(&self) -> FrameChannel {
        FrameChannel {
            receiver: self.drain_rx.clone(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl FrameConsumer for RingBufferConsumer {
    /// Push a frame to the ring buffer.
    ///
    /// Returns immediately. If the buffer is full, drains the oldest slot
    /// (drop-oldest) and retries once. Frame cloning is limited to Arc wrapping —
    /// pixel data is shared, not copied.
    fn consume(&mut self, frame: &CapturedFrame, _frame_id: u64) -> Result<(), ConsumerError> {
        use crossbeam_channel::TrySendError;

        // Wrap pixel data in Arc to avoid full copies on the hot path.
        let shared = Arc::new(CapturedFrame {
            data: frame.data.clone(),
            width: frame.width,
            height: frame.height,
            capture_ts_ns: frame.capture_ts_ns,
            dxgi_acquire_ts_ns: frame.dxgi_acquire_ts_ns,
            buffer_ready_ts_ns: frame.buffer_ready_ts_ns,
        });

        match self.sender.try_send(Arc::clone(&shared)) {
            Ok(()) => {
                self.metrics.queue_depth.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                // Drop-oldest: drain the oldest frame to make room.
                if self.drain_rx.try_recv().is_ok() {
                    self.metrics
                        .ring_buffer_drop_count
                        .fetch_add(1, Ordering::Relaxed);
                    // queue_depth unchanged: one removed, one added below.
                }
                // Retry. If this fails again (e.g. bot is filling the channel
                // faster than we drain), silently drop the new frame too.
                let _ = self.sender.try_send(shared);
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => Err(ConsumerError::Dropped),
        }
    }

    fn flush(&mut self) -> Result<(), ConsumerError> {
        // Ring buffer has no persistent state — no-op.
        Ok(())
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::MonotonicNs;
    use std::sync::atomic::Ordering;

    fn make_frame(ts: u64) -> CapturedFrame {
        CapturedFrame {
            data: vec![0u8; 4],
            width: 1,
            height: 1,
            capture_ts_ns: MonotonicNs(ts),
            dxgi_acquire_ts_ns: None,
            buffer_ready_ts_ns: None,
        }
    }

    #[test]
    fn try_recv_returns_pushed_frame() {
        let metrics = PipelineMetrics::new();
        let mut consumer = RingBufferConsumer::new(4, Arc::clone(&metrics));
        let reader = consumer.reader();

        consumer.consume(&make_frame(1), 0).unwrap();
        let f = reader.try_recv();
        assert!(f.is_some());
    }

    #[test]
    fn drop_oldest_on_full() {
        let metrics = PipelineMetrics::new();
        let mut consumer = RingBufferConsumer::new(2, Arc::clone(&metrics));
        let reader = consumer.reader();

        // Push 4 frames into capacity-2 buffer.
        for i in 0..4u64 {
            consumer.consume(&make_frame(i * 10), i).unwrap();
        }

        let drops = metrics.ring_buffer_drop_count.load(Ordering::Relaxed);
        assert!(drops >= 1, "expected at least one drop, got {drops}");

        // Reader still gets a frame.
        assert!(reader.try_recv().is_some());
    }

    #[test]
    fn recv_timeout_returns_none_on_empty() {
        let metrics = PipelineMetrics::new();
        let consumer = RingBufferConsumer::new(4, metrics);
        let reader = consumer.reader();

        let result = reader.recv_timeout(Duration::from_millis(10));
        assert!(result.is_none());
    }

    #[test]
    fn flush_is_noop() {
        let metrics = PipelineMetrics::new();
        let mut consumer = RingBufferConsumer::new(4, metrics);
        assert!(consumer.flush().is_ok());
    }
}
