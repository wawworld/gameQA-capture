//! T014 — `FrameConsumer` mock + consume/flush/error propagation contract tests.

use game_qa::MonotonicNs;
use game_qa::capture::CapturedFrame;
use game_qa::pipeline::consumer::{ConsumerError, FrameConsumer};

// ─── Mock consumer ────────────────────────────────────────────────────────────

pub struct MockFrameConsumer {
    pub consumed_frames: Vec<u64>,
    pub flush_called: bool,
    pub inject_error_on_frame: Option<u64>,
}

impl Default for MockFrameConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl MockFrameConsumer {
    pub fn new() -> Self {
        Self {
            consumed_frames: Vec::new(),
            flush_called: false,
            inject_error_on_frame: None,
        }
    }

    pub fn inject_error_on(&mut self, frame_id: u64) {
        self.inject_error_on_frame = Some(frame_id);
    }
}

fn dummy_frame() -> CapturedFrame {
    CapturedFrame {
        data: vec![0u8; 16],
        width: 4,
        height: 2,
        capture_ts_ns: MonotonicNs(0),
        dxgi_acquire_ts_ns: None,
        buffer_ready_ts_ns: None,
    }
}

impl FrameConsumer for MockFrameConsumer {
    fn consume(&mut self, _frame: &CapturedFrame, frame_id: u64) -> Result<(), ConsumerError> {
        if self.inject_error_on_frame == Some(frame_id) {
            return Err(ConsumerError::DiskError("injected error".to_string()));
        }
        self.consumed_frames.push(frame_id);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ConsumerError> {
        self.flush_called = true;
        Ok(())
    }
}

// ─── Contract tests ───────────────────────────────────────────────────────────

#[test]
fn consume_records_frame_ids_in_order() {
    let mut consumer = MockFrameConsumer::new();
    let frame = dummy_frame();
    consumer.consume(&frame, 0).unwrap();
    consumer.consume(&frame, 1).unwrap();
    consumer.consume(&frame, 2).unwrap();
    assert_eq!(consumer.consumed_frames, vec![0, 1, 2]);
}

#[test]
fn flush_called_after_session_end() {
    let mut consumer = MockFrameConsumer::new();
    assert!(!consumer.flush_called);
    consumer.flush().unwrap();
    assert!(consumer.flush_called);
}

#[test]
fn error_is_propagated() {
    let mut consumer = MockFrameConsumer::new();
    consumer.inject_error_on(5);
    let frame = dummy_frame();
    let result = consumer.consume(&frame, 5);
    assert!(result.is_err());
}

#[test]
fn consume_continues_after_single_error() {
    let mut consumer = MockFrameConsumer::new();
    consumer.inject_error_on(1);
    let frame = dummy_frame();
    consumer.consume(&frame, 0).unwrap();
    let _ = consumer.consume(&frame, 1); // error
    consumer.consume(&frame, 2).unwrap();
    assert!(consumer.consumed_frames.contains(&0));
    assert!(consumer.consumed_frames.contains(&2));
}
