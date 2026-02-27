//! `PipelineMetrics` — lock-free runtime observability.
//!
//! All fields are atomics. Readable from any thread via `load(Ordering::Relaxed)`
//! without acquiring any lock.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// Lock-free pipeline metrics.
///
/// All fields are atomic so they can be read from any thread (observer, debug,
/// external CLI) without stopping or locking the capture pipeline.
#[derive(Debug, Default)]
pub struct PipelineMetrics {
    /// Rolling P95 of `dxgi_acquire` → `buffer_ready` latency (nanoseconds).
    pub capture_latency_p95_ns: AtomicU64,
    /// Rolling P95 of `capture_ts[i+1] - capture_ts[i]` (nanoseconds).
    pub frame_interval_p95_ns: AtomicU64,
    /// Rolling P95 of schedule offset — signed (nanoseconds).
    pub schedule_error_p95_ns: AtomicI64,
    /// Rolling P99 of `hook_recv` → `queue_push_done` latency (nanoseconds).
    pub hook_latency_p99_ns: AtomicU64,
    /// Total events with `null prev_frame_id` this session.
    pub orphan_event_count: AtomicU64,
    /// Total ring-buffer overwrites this session (drop-oldest).
    pub ring_buffer_drop_count: AtomicU64,
    /// Total sessions completed in this automation run.
    pub session_completion_count: AtomicU64,
    /// Current encoder queue depth (frames waiting for H.264 encoding).
    pub queue_depth: AtomicU64,
    /// Total frame-gap events (Δt > 66 ms) this session.
    pub gap_count: AtomicU64,
}

impl PipelineMetrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    // ── Accessor helpers (Relaxed load — appropriate for monitoring) ──

    pub fn capture_latency_p95_ns(&self) -> u64 {
        self.capture_latency_p95_ns.load(Ordering::Relaxed)
    }

    pub fn frame_interval_p95_ns(&self) -> u64 {
        self.frame_interval_p95_ns.load(Ordering::Relaxed)
    }

    pub fn schedule_error_p95_ns(&self) -> i64 {
        self.schedule_error_p95_ns.load(Ordering::Relaxed)
    }

    pub fn hook_latency_p99_ns(&self) -> u64 {
        self.hook_latency_p99_ns.load(Ordering::Relaxed)
    }

    pub fn orphan_event_count(&self) -> u64 {
        self.orphan_event_count.load(Ordering::Relaxed)
    }

    pub fn ring_buffer_drop_count(&self) -> u64 {
        self.ring_buffer_drop_count.load(Ordering::Relaxed)
    }

    pub fn session_completion_count(&self) -> u64 {
        self.session_completion_count.load(Ordering::Relaxed)
    }

    pub fn queue_depth(&self) -> u64 {
        self.queue_depth.load(Ordering::Relaxed)
    }

    pub fn gap_count(&self) -> u64 {
        self.gap_count.load(Ordering::Relaxed)
    }
}
