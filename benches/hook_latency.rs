//! Criterion benchmark: hook-to-queue latency (SC-005).

use criterion::{Criterion, criterion_group, criterion_main};
use crossbeam_channel::bounded;
use game_qa::MonotonicNs;
use game_qa::hooks::InputEvent;

fn bench_hook_channel_push(c: &mut Criterion) {
    let (tx, rx) = bounded::<InputEvent>(4096);

    c.bench_function("hook_channel_push", |b| {
        b.iter(|| {
            let event = InputEvent::KeyPress {
                vk_code: 0x20,
                scan_code: 57,
                flags: 0,
                recv_ts_ns: MonotonicNs(0),
            };
            tx.send(event).unwrap();
            let _ = rx.recv().unwrap();
        });
    });
}

criterion_group!(benches, bench_hook_channel_push);
criterion_main!(benches);
