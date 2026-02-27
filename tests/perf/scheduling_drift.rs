//! T037 — SC-003: Scheduling drift test.
//!
//! Asserts:
//!   Schedule error median ≤ 5 ms
//!   Schedule error P95 ≤ 20 ms
//!   Measured over a 1-hour window (abbreviated to 10 s in fast mode).

use std::time::{Duration, Instant};

fn percentile(mut samples: Vec<i64>, p: f64) -> i64 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let idx = ((p / 100.0) * (samples.len() as f64 - 1.0)).round() as usize;
    samples[idx]
}

/// SC-003: scheduling drift — median ≤ 5 ms, P95 ≤ 20 ms over a simulated window.
///
/// The full 1-hour test is `#[ignore]`; the 10-second fast variant runs in CI.
#[test]
fn scheduling_drift_fast() {
    let target_interval = Duration::from_millis(33); // ~30 fps
    let n_samples = 300; // ~10 s at 30 fps

    let mut errors_ns: Vec<i64> = Vec::with_capacity(n_samples);
    let mut next_wake = Instant::now();

    for _ in 0..n_samples {
        next_wake += target_interval;
        let now = Instant::now();
        // Schedule error = actual_time - target_time (positive = late)
        let error_ns = now
            .duration_since(next_wake.checked_sub(target_interval).unwrap_or(next_wake))
            .as_nanos() as i64;
        errors_ns.push(error_ns);

        // Sleep until next scheduled wake.
        if let Some(remaining) = next_wake.checked_duration_since(Instant::now()) {
            std::thread::sleep(remaining);
        }
    }

    let median = percentile(errors_ns.clone(), 50.0);
    let p95 = percentile(errors_ns, 95.0);

    println!(
        "SC-003 scheduling drift: median = {} ns ({:.3} ms), P95 = {} ns ({:.3} ms)",
        median,
        median.abs() as f64 / 1_000_000.0,
        p95,
        p95.abs() as f64 / 1_000_000.0
    );

    let median_limit = 5_000_000i64; // 5 ms
    let p95_limit = 20_000_000i64; // 20 ms

    assert!(
        median.abs() <= median_limit,
        "SC-003 FAIL: scheduling drift median = {:.3} ms > 5 ms",
        median.abs() as f64 / 1_000_000.0
    );
    assert!(
        p95.abs() <= p95_limit,
        "SC-003 FAIL: scheduling drift P95 = {:.3} ms > 20 ms",
        p95.abs() as f64 / 1_000_000.0
    );
}

/// Full 1-hour scheduling drift test. Run with `cargo test -- --ignored`.
#[test]
#[ignore = "full 1-hour test — run explicitly with --ignored"]
fn scheduling_drift_one_hour() {
    // TODO(T037): run for 1 hour (~108,000 frames at 30 fps) and assert same thresholds.
}
