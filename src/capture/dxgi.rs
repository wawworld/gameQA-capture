//! `DxgiBackend` — `IDXGIOutputDuplication`-based screen capture.
//!
//! Reads the compositor output. Zero game-process handle access.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::{CaptureBackend, CapturedFrame, CaptureError, Rect};
use crate::config::CaptureConfig;
use crate::MonotonicNs;
use std::time::Instant;
use win_desktop_duplication::{
    devices::AdapterFactory, errors::DDApiError, tex_reader::TextureReader, DesktopDuplicationApi,
    DuplicationApiOptions,
};

/// DXGI Desktop Duplication backend.
///
/// Captures the full monitor via `IDXGIOutputDuplication`.
/// No game-process handles are opened at any point.
pub struct DxgiBackend {
    /// Config snapshot used at construction.
    config: CaptureConfig,
    /// Session start instant — all timestamps are relative to this.
    session_start: Instant,
    /// Cached monitor rect.
    monitor_rect: Rect,
    /// DXGI Desktop Duplication API handle.
    dupl: DesktopDuplicationApi,
    /// CPU-readable staging texture reader.
    reader: TextureReader,
    /// Pixel buffer reused across frames (avoids per-frame allocation).
    pixel_buf: Vec<u8>,
}

impl DxgiBackend {
    /// Initialise the DXGI backend for the primary monitor.
    ///
    /// # Errors
    /// Returns `CaptureError::DeviceLost` if DXGI output duplication cannot be initialised
    /// (e.g. no WDDM driver, or another exclusive-mode D3D app is running).
    pub fn new(config: CaptureConfig, session_start: Instant) -> Result<Self, CaptureError> {
        // Must be called before any DXGI/D3D work on this thread.
        win_desktop_duplication::set_process_dpi_awareness();
        win_desktop_duplication::co_init();

        let adapter = AdapterFactory::new()
            .get_adapter_by_idx(0)
            .ok_or_else(|| CaptureError::DeviceLost("no DXGI adapter found".to_string()))?;

        let output = adapter
            .get_display_by_idx(0)
            .ok_or_else(|| CaptureError::DeviceLost("no display on adapter 0".to_string()))?;

        let mode = output
            .get_current_display_mode()
            .map_err(|e| CaptureError::DeviceLost(format!("get_current_display_mode: {e:?}")))?;

        let monitor_rect = Rect { x: 0, y: 0, width: mode.width, height: mode.height };

        let mut dupl = DesktopDuplicationApi::new(adapter, output)
            .map_err(|e| CaptureError::DeviceLost(format!("DesktopDuplicationApi::new: {e:?}")))?;

        // Skip cursor pre-rendering — we capture raw compositor BGRA output.
        dupl.configure(DuplicationApiOptions { skip_cursor: true });

        let (device, ctx) = dupl.get_device_and_ctx();
        let reader = TextureReader::new(device, ctx);

        Ok(Self {
            config,
            session_start,
            monitor_rect,
            dupl,
            reader,
            pixel_buf: Vec::new(),
        })
    }

    fn elapsed_ns(&self) -> MonotonicNs {
        MonotonicNs(self.session_start.elapsed().as_nanos() as u64)
    }
}

impl CaptureBackend for DxgiBackend {
    fn acquire_frame(&mut self) -> Result<Option<CapturedFrame>, CaptureError> {
        let acquire_ts = self.elapsed_ns();

        let tex = match self.dupl.acquire_next_frame_now() {
            Ok(t) => t,
            // Recoverable: desktop switch, resolution change, or secure desktop.
            // Caller may retry on the next pipeline tick.
            Err(DDApiError::AccessLost) | Err(DDApiError::AccessDenied) => {
                return Ok(None);
            }
            Err(e) => {
                return Err(CaptureError::DeviceLost(format!(
                    "acquire_next_frame_now: {e:?}"
                )));
            }
        };

        let buffer_ready_ts = self.elapsed_ns();

        self.reader
            .get_data(&mut self.pixel_buf, &tex)
            .map_err(|e| CaptureError::DeviceLost(format!("TextureReader::get_data: {e:?}")))?;

        let desc = tex.desc();

        let (dxgi_ts, buf_ts) = if self.config.diagnostics_enabled {
            (Some(acquire_ts), Some(buffer_ready_ts))
        } else {
            (None, None)
        };

        Ok(Some(CapturedFrame {
            data: self.pixel_buf.clone(),
            width: desc.width,
            height: desc.height,
            capture_ts_ns: buffer_ready_ts,
            dxgi_acquire_ts_ns: dxgi_ts,
            buffer_ready_ts_ns: buf_ts,
        }))
    }

    fn release_frame(&mut self, _frame: CapturedFrame) -> Result<(), CaptureError> {
        // win_desktop_duplication releases the DXGI frame internally inside
        // acquire_next_frame_now before returning. The CapturedFrame holds a
        // Vec<u8> copy, so dropping it here is sufficient.
        Ok(())
    }

    fn monitor_rect(&self) -> Rect {
        self.monitor_rect
    }
}
