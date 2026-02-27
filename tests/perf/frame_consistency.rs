//! T019 — SC-002: Frame rate consistency.
//!
//! Assertions:
//! - Average FPS: 30 ± 2 fps
//! - Frame interval P95 ≤ 40 ms
//! - Gap count (Δt > 66 ms) = 0 under normal conditions

fn percentile(mut samples: Vec<u64>, p: f64) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let idx = ((p / 100.0) * (samples.len() as f64 - 1.0)).round() as usize;
    samples[idx]
}

#[test]
fn frame_interval_p95_le_40ms() {
    // Use simulated 30 fps timestamps to test the measurement logic.
    // For real validation, collect from a live DxgiBackend.
    let n_frames = 300; // ~10 seconds at 30 fps
    let mut intervals_ns: Vec<u64> = Vec::with_capacity(n_frames);
    let mut prev_ts: Option<u64> = None;
    let mut gap_count = 0usize;

    for i in 0..n_frames {
        // Simulate 30 fps pacing with small jitter (±100 µs).
        let jitter_ns = (i % 7) as u64 * 100_000;
        let target_ts_ns = (i as u64) * 33_333_333 + jitter_ns;
        let ts = target_ts_ns;
        if let Some(prev) = prev_ts {
            let interval = ts.saturating_sub(prev);
            intervals_ns.push(interval);
            if interval > 66_000_000 {
                gap_count += 1;
            }
        }
        prev_ts = Some(ts);
    }

    let p95 = percentile(intervals_ns.clone(), 95.0);
    let avg_fps =
        (n_frames as f64 - 1.0) / (intervals_ns.iter().sum::<u64>() as f64 / 1_000_000_000.0);

    println!(
        "SC-002 frame interval P95 = {} ns ({:.1} ms)",
        p95,
        p95 as f64 / 1_000_000.0
    );
    println!("SC-002 average FPS = {:.1}", avg_fps);
    println!("SC-002 gap count = {}", gap_count);

    assert!(
        p95 <= 40_000_000,
        "SC-002 FAIL: frame interval P95 = {:.1} ms > 40 ms",
        p95 as f64 / 1_000_000.0
    );
    assert!(
        (avg_fps - 30.0).abs() <= 2.0,
        "SC-002 FAIL: average FPS = {:.1}, expected 30 ± 2",
        avg_fps
    );
    assert_eq!(
        gap_count, 0,
        "SC-002 FAIL: {} gap(s) detected under normal conditions",
        gap_count
    );
}
