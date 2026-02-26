# Contract: CaptureBackend

**Interface**: `src/capture/mod.rs` — `CaptureBackend` trait
**Implementors**: `DxgiBackend` (current); future: `BitBltBackend`, `WgcBackend`
**Consumers**: `Pipeline` (orchestrator)

---

## Trait Definition

```rust
use crate::config::CaptureConfig;

/// A non-intrusive screen capture backend.
///
/// All implementations MUST:
/// - Require zero WRITE/DEBUG access to any game process handle
/// - Return frames in BGRA pixel format
/// - Timestamp all frames using MonotonicNs relative to session start
pub trait CaptureBackend: Send {
    /// Acquire the next available frame from the display.
    ///
    /// Blocks until a new frame is available (up to a backend-defined timeout).
    /// Returns `Ok(None)` if no new frame arrived within the timeout (not an error).
    ///
    /// # Timing contract
    /// The returned `CapturedFrame.capture_ts_ns` MUST be set immediately after
    /// the GPU→CPU transfer completes (= buffer_ready_ts_ns in diagnostics).
    fn acquire_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError>;

    /// Release frame resources back to the backend.
    ///
    /// MUST be called after the caller finishes reading pixel data.
    /// Some backends (e.g. DXGI) require explicit release before the next acquire.
    fn release_frame(&mut self, frame: CapturedFrame) -> Result<(), CaptureError>;

    /// Return the monitor dimensions the backend is capturing.
    fn monitor_rect(&self) -> Rect;
}
```

---

## Associated Types

```rust
/// A single captured display frame.
pub struct CapturedFrame {
    /// Raw pixel data in BGRA format, row-major, top-to-bottom.
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,

    /// Monotonic timestamp: set when GPU→CPU copy completes.
    pub capture_ts_ns: MonotonicNs,

    /// Diagnostics: DXGI AcquireNextFrame call completion time. None if diagnostics off.
    pub dxgi_acquire_ts_ns: Option<MonotonicNs>,

    /// Diagnostics: memory arrival (= capture_ts_ns in most backends). None if diagnostics off.
    pub buffer_ready_ts_ns: Option<MonotonicNs>,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("Device lost: {0}")]
    DeviceLost(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Timeout waiting for frame")]
    Timeout,
    #[error("Backend error: {0}")]
    Other(String),
}

pub struct Rect { pub x: i32, pub y: i32, pub width: u32, pub height: u32 }
```

---

## Invariants

1. **Non-intrusion**: `acquire_frame` MUST NOT open any handle to a game process. Only
   desktop/display-level OS handles are permitted.
2. **Monotonic timestamps**: All `*_ts_ns` fields MUST use the same `Instant` session origin.
3. **Schema compatibility**: If `dxgi_acquire_ts_ns` is `None`, `buffer_ready_ts_ns` MUST
   also be `None`. Both null or both non-null — never mixed.
4. **Release obligation**: Callers MUST call `release_frame` after processing. Not calling
   `release_frame` is a logic error; backends MAY stall subsequent `acquire_frame` calls.

---

## Backend Swap Contract

Swapping backends requires:
1. Implement `CaptureBackend` for the new type.
2. Update `CaptureBackendType` enum in config schema.
3. Zero changes to `Pipeline`, `RoiManager`, `DiskRecorderConsumer`, or any other caller.

This contract is verified by `tests/unit/capture_backend_mock.rs` which runs the full
pipeline against a mock backend.
