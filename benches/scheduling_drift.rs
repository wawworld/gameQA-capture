//! Criterion benchmark: scheduling drift measurement (SC-003).

use criterion::{Criterion, criterion_group, criterion_main};
use std::time::{Duration, Instant};

fn bench_sleep_drift(c: &mut Criterion) {
    let target = Duration::from_millis(1);
    c.bench_function("sleep_1ms_drift", |b| {
        b.iter(|| {
            let before = Instant::now();
            std::thread::sleep(target);
            let actual = before.elapsed();
            actual.as_nanos() as i64 - target.as_nanos() as i64
        });
    });
}

criterion_group!(benches, bench_sleep_drift);
criterion_main!(benches);
