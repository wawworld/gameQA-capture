//! Criterion benchmark: game FPS impact (SC-008).
//! Stub — real measurement requires a live game process.

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_fps_impact_stub(c: &mut Criterion) {
    // Stub benchmark — real implementation in tests/perf/game_fps_impact.rs
    c.bench_function("fps_impact_stub", |b| {
        b.iter(|| {
            // Simulate 1 frame interval measurement.
            let _ts = std::time::Instant::now().elapsed().as_nanos();
        });
    });
}

criterion_group!(benches, bench_fps_impact_stub);
criterion_main!(benches);
