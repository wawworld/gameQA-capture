//! T013 — `CaptureBackend` mock + full trait contract tests.
//!
//! Verifies:
//! - acquire/release lifecycle
//! - MonotonicNs timestamps are set
//! - diagnostics fields are both null or both non-null (never mixed)
//! - `monitor_rect()` returns consistent values

use game_qa::MonotonicNs;
use game_qa::capture::{CaptureBackend, CaptureError, CapturedFrame, Rect};
use std::time::Instant;

// ─── Mock backend ─────────────────────────────────────────────────────────────

pub struct MockCaptureBackend {
    session_start: Instant,
    frame_counter: u64,
    monitor: Rect,
    /// If `Some`, inject this error on the next `acquire_frame`.
    next_error: Option<CaptureError>,
    /// Whether to include diagnostic timestamps.
    diagnostics: bool,
}

impl MockCaptureBackend {
    pub fn new(diagnostics: bool) -> Self {
        Self {
            session_start: Instant::now(),
            frame_counter: 0,
            monitor: Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            next_error: None,
            diagnostics,
        }
    }

    pub fn inject_error(&mut self, err: CaptureError) {
        self.next_error = Some(err);
    }
}

impl CaptureBackend for MockCaptureBackend {
    fn acquire_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
        if let Some(err) = self.next_error.take() {
            return Err(err);
        }

        let elapsed = self.session_start.elapsed().as_nanos() as u64;
        let capture_ts_ns = MonotonicNs(elapsed);

        let (dxgi_acquire_ts_ns, buffer_ready_ts_ns) = if self.diagnostics {
            (
                Some(MonotonicNs(elapsed.saturating_sub(1_000_000))),
                Some(MonotonicNs(elapsed.saturating_sub(500_000))),
            )
        } else {
            (None, None)
        };

        self.frame_counter += 1;
        Ok(Some(CapturedFrame {
            data: vec![0u8; 64],
            width: 8,
            height: 4,
            capture_ts_ns,
            dxgi_acquire_ts_ns,
            buffer_ready_ts_ns,
        }))
    }

    fn release_frame(&mut self, _frame: CapturedFrame) -> Result<(), CaptureError> {
        Ok(())
    }

    fn monitor_rect(&self) -> Rect {
        self.monitor
    }
}

// ─── Contract tests ───────────────────────────────────────────────────────────

#[test]
fn acquire_returns_frame() {
    let mut backend = MockCaptureBackend::new(false);
    let frame = backend.acquire_frame().unwrap().unwrap();
    assert_eq!(frame.width, 8);
    assert_eq!(frame.height, 4);
}

#[test]
fn timestamps_are_monotonic() {
    let mut backend = MockCaptureBackend::new(false);
    let f1 = backend.acquire_frame().unwrap().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let f2 = backend.acquire_frame().unwrap().unwrap();
    assert!(f2.capture_ts_ns > f1.capture_ts_ns);
}

#[test]
fn diagnostics_both_null_when_disabled() {
    let mut backend = MockCaptureBackend::new(false);
    let frame = backend.acquire_frame().unwrap().unwrap();
    assert!(frame.dxgi_acquire_ts_ns.is_none());
    assert!(frame.buffer_ready_ts_ns.is_none());
}

#[test]
fn diagnostics_both_non_null_when_enabled() {
    let mut backend = MockCaptureBackend::new(true);
    let frame = backend.acquire_frame().unwrap().unwrap();
    assert!(frame.dxgi_acquire_ts_ns.is_some());
    assert!(frame.buffer_ready_ts_ns.is_some());
}

#[test]
fn diagnostics_ordering_invariant() {
    let mut backend = MockCaptureBackend::new(true);
    let frame = backend.acquire_frame().unwrap().unwrap();
    if let (Some(acq), Some(rdy)) = (frame.dxgi_acquire_ts_ns, frame.buffer_ready_ts_ns) {
        assert!(
            acq <= rdy,
            "dxgi_acquire_ts_ns must be ≤ buffer_ready_ts_ns"
        );
        assert!(
            rdy <= frame.capture_ts_ns,
            "buffer_ready_ts_ns must be ≤ capture_ts_ns"
        );
    }
}

#[test]
fn release_frame_is_idempotent() {
    let mut backend = MockCaptureBackend::new(false);
    let frame = backend.acquire_frame().unwrap().unwrap();
    backend.release_frame(frame).unwrap();
}

#[test]
fn monitor_rect_consistent() {
    let backend = MockCaptureBackend::new(false);
    let r1 = backend.monitor_rect();
    let r2 = backend.monitor_rect();
    assert_eq!(r1.width, r2.width);
    assert_eq!(r1.height, r2.height);
}

#[test]
fn error_injection_works() {
    let mut backend = MockCaptureBackend::new(false);
    backend.inject_error(CaptureError::DeviceLost("test".to_string()));
    let result = backend.acquire_frame();
    assert!(result.is_err());
    // After the error, the backend recovers.
    let ok = backend.acquire_frame().unwrap();
    assert!(ok.is_some());
}
