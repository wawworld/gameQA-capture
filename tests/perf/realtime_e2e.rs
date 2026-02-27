//! T043 — SC-004: Ring buffer E2E capture-to-consumer P95 ≤ 8 ms.
//!
//! Uses a mock consumer attached to `FrameChannel`.
//! This test verifies the structural interface (US3) not a live pipeline.

use game_qa::MonotonicNs;
use game_qa::capture::CapturedFrame;
use game_qa::pipeline::consumer::FrameConsumer;
use game_qa::pipeline::metrics::PipelineMetrics;
use game_qa::pipeline::ring_buffer::RingBufferConsumer;
use std::time::Instant;

fn dummy_frame(ts_ns: u64) -> CapturedFrame {
    CapturedFrame {
        data: vec![0u8; 64],
        width: 8,
        height: 4,
        capture_ts_ns: MonotonicNs(ts_ns),
        dxgi_acquire_ts_ns: None,
        buffer_ready_ts_ns: None,
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

/// SC-004: E2E capture→ring-buffer→consumer P95 ≤ 8 ms.
///
/// NOTE: Currently tests the stub `RingBufferConsumer`. After TODO T045/T046 are implemented,
/// this test will measure real `thingbuf` round-trip latency.
#[test]
fn realtime_e2e_p95_le_8ms() {
    let metrics = PipelineMetrics::new();
    let mut consumer = RingBufferConsumer::new(3, metrics.clone());
    let _reader = consumer.reader();

    let mut latencies_ns: Vec<u64> = Vec::with_capacity(1000);
    let start = Instant::now();

    for i in 0..1000u64 {
        let send_ts = start.elapsed().as_nanos() as u64;
        let frame = dummy_frame(send_ts);
        consumer.consume(&frame, i).unwrap();
        let recv_ts = start.elapsed().as_nanos() as u64;
        latencies_ns.push(recv_ts.saturating_sub(send_ts));
    }

    let p95 = percentile(latencies_ns, 95.0);
    let target_ns = 8_000_000u64; // 8 ms

    println!(
        "SC-004 ring buffer E2E P95 = {} ns ({:.3} ms)",
        p95,
        p95 as f64 / 1_000_000.0
    );
    assert!(
        p95 <= target_ns,
        "SC-004 FAIL: E2E P95 = {:.3} ms > 8 ms target",
        p95 as f64 / 1_000_000.0
    );
}
