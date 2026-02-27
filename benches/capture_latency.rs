//! Criterion benchmark: capture latency (SC-001).

use criterion::{Criterion, criterion_group, criterion_main};
use game_qa::capture::CaptureBackend;

fn bench_acquire_frame(c: &mut Criterion) {
    // Use mock backend for benchmarking framework validation.
    // Replace with DxgiBackend for real hardware benchmarks.
    c.bench_function("acquire_frame_mock", |b| {
        use std::time::Instant;
        struct MockBackend {
            start: Instant,
        }
        impl game_qa::capture::CaptureBackend for MockBackend {
            fn acquire_frame(
                &mut self,
            ) -> Result<Option<game_qa::capture::CapturedFrame>, game_qa::capture::CaptureError>
            {
                Ok(Some(game_qa::capture::CapturedFrame {
                    data: vec![0u8; 64],
                    width: 8,
                    height: 4,
                    capture_ts_ns: game_qa::MonotonicNs(self.start.elapsed().as_nanos() as u64),
                    dxgi_acquire_ts_ns: None,
                    buffer_ready_ts_ns: None,
                }))
            }
            fn release_frame(
                &mut self,
                _f: game_qa::capture::CapturedFrame,
            ) -> Result<(), game_qa::capture::CaptureError> {
                Ok(())
            }
            fn monitor_rect(&self) -> game_qa::capture::Rect {
                game_qa::capture::Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                }
            }
        }
        let mut backend = MockBackend {
            start: Instant::now(),
        };
        b.iter(|| {
            let frame = backend.acquire_frame().unwrap().unwrap();
            backend.release_frame(frame).unwrap();
        });
    });
}

criterion_group!(benches, bench_acquire_frame);
criterion_main!(benches);
