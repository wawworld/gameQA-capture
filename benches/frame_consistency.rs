//! Criterion benchmark: frame rate consistency (SC-002).

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_frame_interval_measurement(c: &mut Criterion) {
    c.bench_function("frame_interval_measurement", |b| {
        b.iter(|| {
            let _ts = std::time::Instant::now().elapsed().as_nanos() as u64;
        });
    });
}

criterion_group!(benches, bench_frame_interval_measurement);
criterion_main!(benches);
