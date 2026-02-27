//! `CaptureBackend` trait — non-intrusive screen capture abstraction.
//!
//! All implementations MUST:
//! - Require zero WRITE/DEBUG access to any game process handle
//! - Return frames in BGRA pixel format
//! - Timestamp all frames using `MonotonicNs` relative to session start
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod dxgi;

use crate::MonotonicNs;

/// A single captured display frame.
#[derive(Debug)]
pub struct CapturedFrame {
    /// Raw pixel data in BGRA format, row-major, top-to-bottom.
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,

    /// Monotonic timestamp: set when GPU→CPU copy completes.
    pub capture_ts_ns: MonotonicNs,

    /// Diagnostics: DXGI `AcquireNextFrame` call completion time.
    /// `None` if diagnostics are disabled.
    pub dxgi_acquire_ts_ns: Option<MonotonicNs>,

    /// Diagnostics: memory arrival (= `capture_ts_ns` in most backends).
    /// `None` if diagnostics are disabled.
    /// Invariant: if `dxgi_acquire_ts_ns` is `Some`, this MUST also be `Some`.
    pub buffer_ready_ts_ns: Option<MonotonicNs>,
}

/// Monitor dimensions being captured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Errors that a `CaptureBackend` implementation may return.
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

/// A non-intrusive screen capture backend.
///
/// All implementations MUST:
/// - Require zero WRITE/DEBUG access to any game process handle
/// - Return frames in BGRA pixel format
/// - Timestamp all frames using `MonotonicNs` relative to session start
pub trait CaptureBackend: Send {
    /// Acquire the next available frame from the display.
    ///
    /// Blocks until a new frame is available (up to a backend-defined timeout).
    /// Returns `Ok(None)` if no new frame arrived within the timeout (not an error).
    ///
    /// # Timing contract
    /// The returned `CapturedFrame.capture_ts_ns` MUST be set immediately after
    /// the GPU→CPU transfer completes (= `buffer_ready_ts_ns` in diagnostics).
    fn acquire_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError>;

    /// Release frame resources back to the backend.
    ///
    /// MUST be called after the caller finishes reading pixel data.
    /// Some backends (e.g. DXGI) require explicit release before the next acquire.
    fn release_frame(&mut self, frame: CapturedFrame) -> Result<(), CaptureError>;

    /// Return the monitor dimensions the backend is capturing.
    fn monitor_rect(&self) -> Rect;
}
