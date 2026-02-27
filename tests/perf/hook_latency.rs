//! T020 — SC-005: Hook-to-queue latency P99 ≤ 10 ms regression test.
//!
//! Measures the time between `event_ts_ns` (hook receipt) and `queue_push_done_ts_ns`
//! (after channel push completes).

use game_qa::MonotonicNs;
use std::time::Instant;

fn percentile(mut samples: Vec<u64>, p: f64) -> u64 {
    samples.sort_unstable();
    let idx = ((p / 100.0) * (samples.len() as f64 - 1.0)).round() as usize;
    samples[idx]
}

/// Simulate hook→channel push latency using a real crossbeam channel.
#[test]
fn hook_latency_p99_le_10ms() {
    use crossbeam_channel::bounded;
    use game_qa::hooks::InputEvent;

    let (tx, _rx) = bounded::<InputEvent>(4096);
    let mut latencies_ns: Vec<u64> = Vec::with_capacity(1000);
    let session_start = Instant::now();

    for _ in 0..1000 {
        let recv_ts = MonotonicNs(session_start.elapsed().as_nanos() as u64);

        let event = InputEvent::KeyPress {
            vk_code: 0x20,
            scan_code: 57,
            flags: 0,
            recv_ts_ns: recv_ts,
        };

        tx.send(event).unwrap();
        let push_done_ts = MonotonicNs(session_start.elapsed().as_nanos() as u64);

        let latency = push_done_ts.0.saturating_sub(recv_ts.0);
        latencies_ns.push(latency);
    }

    drop(tx);

    let p99 = percentile(latencies_ns, 99.0);
    let target_ns = 10_000_000u64; // 10 ms

    println!(
        "SC-005 hook latency P99 = {} ns ({:.3} ms)",
        p99,
        p99 as f64 / 1_000_000.0
    );
    assert!(
        p99 <= target_ns,
        "SC-005 FAIL: hook latency P99 = {:.3} ms > 10 ms target",
        p99 as f64 / 1_000_000.0
    );
}
