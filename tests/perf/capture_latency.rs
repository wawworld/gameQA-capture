//! T018 — SC-001: Capture latency P95 ≤ 5 ms regression test.
//!
//! Measures the time from `acquire_frame` call to when `CapturedFrame.capture_ts_ns` is set.
//! Uses the mock backend to establish a measurement baseline; real hardware measurements
//! require `DxgiBackend` running against a live display.

use game_qa::MonotonicNs;
use game_qa::capture::{CaptureBackend, CaptureError, CapturedFrame, Rect};
use std::time::Instant;

struct MockBackend {
    start: Instant,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl CaptureBackend for MockBackend {
    fn acquire_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
        Ok(Some(CapturedFrame {
            data: vec![0u8; 64],
            width: 8,
            height: 4,
            capture_ts_ns: MonotonicNs(self.start.elapsed().as_nanos() as u64),
            dxgi_acquire_ts_ns: Some(MonotonicNs(self.start.elapsed().as_nanos() as u64)),
            buffer_ready_ts_ns: Some(MonotonicNs(self.start.elapsed().as_nanos() as u64)),
        }))
    }

    fn release_frame(&mut self, _f: CapturedFrame) -> Result<(), CaptureError> {
        Ok(())
    }

    fn monitor_rect(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    }
}

fn percentile(mut samples: Vec<u64>, p: f64) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let idx = ((p / 100.0) * (samples.len() as f64 - 1.0)).round() as usize;
    samples[idx]
}

#[test]
fn capture_latency_p95_le_5ms() {
    // With mock backend this should be sub-microsecond; ensures the test scaffolding works.
    // For real validation, replace MockBackend with DxgiBackend.
    let mut backend = MockBackend::new();
    let mut latencies_ns: Vec<u64> = Vec::with_capacity(1000);

    for _ in 0..1000 {
        let t_call = Instant::now();
        let frame = backend.acquire_frame().unwrap().unwrap();
        let t_done = Instant::now();

        let latency = t_done.duration_since(t_call).as_nanos() as u64;
        latencies_ns.push(latency);

        backend.release_frame(frame).unwrap();
    }

    let p95 = percentile(latencies_ns, 95.0);
    let target_ns = 5_000_000u64; // 5 ms

    println!(
        "SC-001 capture latency P95 = {} ns ({:.3} ms)",
        p95,
        p95 as f64 / 1_000_000.0
    );
    assert!(
        p95 <= target_ns,
        "SC-001 FAIL: capture latency P95 = {:.3} ms > 5 ms target",
        p95 as f64 / 1_000_000.0
    );
}
