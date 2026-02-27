//! Criterion benchmark: ring buffer E2E latency (SC-004).

use criterion::{Criterion, criterion_group, criterion_main};
use game_qa::MonotonicNs;
use game_qa::capture::CapturedFrame;
use game_qa::pipeline::consumer::FrameConsumer;
use game_qa::pipeline::metrics::PipelineMetrics;
use game_qa::pipeline::ring_buffer::RingBufferConsumer;

fn bench_ring_buffer_consume(c: &mut Criterion) {
    let metrics = PipelineMetrics::new();
    let mut consumer = RingBufferConsumer::new(3, metrics);

    let frame = CapturedFrame {
        data: vec![0u8; 64],
        width: 8,
        height: 4,
        capture_ts_ns: MonotonicNs(0),
        dxgi_acquire_ts_ns: None,
        buffer_ready_ts_ns: None,
    };

    let mut frame_id = 0u64;
    c.bench_function("ring_buffer_consume", |b| {
        b.iter(|| {
            consumer.consume(&frame, frame_id).unwrap();
            frame_id += 1;
        });
    });
}

criterion_group!(benches, bench_ring_buffer_consume);
criterion_main!(benches);
