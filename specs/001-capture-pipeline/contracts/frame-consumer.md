# Contract: FrameConsumer

**Interface**: `src/pipeline/consumer.rs` — `FrameConsumer` trait
**Implementors**: `DiskRecorderConsumer`, `RingBufferConsumer`
**Consumers**: `Pipeline` (fan-out to all active consumers each frame)

---

## Trait Definition

```rust
use crate::capture::CapturedFrame;

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
}

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
```

---

## DiskRecorderConsumer Behaviour

- `consume`: pushes frame to a bounded internal encoder queue (non-blocking).
  If the encoder queue is full, the frame is dropped and `PipelineMetrics.queue_depth`
  is updated. The capture thread is NEVER blocked.
- `flush`: drains the encoder queue, finalizes the MP4 file, and writes all remaining
  JSONL lines. Returns only after all I/O is complete.
- Writes `frames.jsonl` entries for every frame including dropped-encoder-queue frames
  (which still appear in the frame log with their timestamp — only their video data is absent).

## RingBufferConsumer Behaviour

- `consume`: attempts a non-blocking push to the `thingbuf` ring buffer. If full, the
  oldest frame is overwritten and `PipelineMetrics.ring_buffer_drop_count` is incremented.
- `flush`: no-op (ring buffer does not have persistent state to flush).
- The bot consumer polls the ring buffer via the `FrameChannel` reader handle (see below).

---

## FrameChannel (Bot Consumer Interface)

The `RingBufferConsumer` exposes a `FrameChannel` reader that the future bot consumer
will use. This is the **structural interface built in US3**:

```rust
/// Reader handle given to the bot consumer.
/// Obtained by calling `RingBufferConsumer::reader()` at session start.
pub struct FrameChannel {
    inner: thingbuf::mpsc::Receiver<CapturedFrame>,
}

impl FrameChannel {
    /// Non-blocking read. Returns None if no new frame is available.
    pub fn try_recv(&self) -> Option<CapturedFrame> { ... }

    /// Blocking read with timeout. Returns None on timeout.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<CapturedFrame> { ... }
}
```

**Bot integration contract**: A future bot consumer implements its inference loop by:
1. Receiving a `FrameChannel` at session start (dependency injection from `Pipeline`).
2. Calling `try_recv()` or `recv_timeout()` in its inference loop.
3. No changes to `Pipeline`, `RingBufferConsumer`, or any other component.

---

## Invariants

1. `consume` MUST NOT block for more than 1 ms (measured at the call site in Pipeline).
   Violations are logged as warnings and counted in metrics.
2. `flush` MAY block for as long as needed to ensure durability.
3. Consumer errors are logged at the session boundary (Principle IV: log once per boundary).
4. The pipeline continues even if one consumer fails (degraded mode); it logs the failure and
   marks the session `low_quality` if the DiskRecorderConsumer fails mid-session.
