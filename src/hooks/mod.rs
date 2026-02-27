//! `InputHookBackend` trait — OS-level input event capture.
//!
//! Uses WH_KEYBOARD_LL / WH_MOUSE_LL hooks (LL variant = no DLL injection).
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod win32;

use crate::MonotonicNs;

/// A raw input event from the OS hook.
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Key press (WM_KEYDOWN / WM_SYSKEYDOWN).
    KeyPress {
        vk_code: u16,
        scan_code: u16,
        flags: u32,
        recv_ts_ns: MonotonicNs,
    },
    /// Key release (WM_KEYUP / WM_SYSKEYUP).
    KeyRelease {
        vk_code: u16,
        scan_code: u16,
        flags: u32,
        recv_ts_ns: MonotonicNs,
    },
    /// Mouse move.
    MouseMove {
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        recv_ts_ns: MonotonicNs,
    },
    /// Mouse button press or release.
    MouseClick {
        x: i32,
        y: i32,
        button: MouseButton,
        action: ClickAction,
        recv_ts_ns: MonotonicNs,
    },
    /// Mouse wheel scroll.
    MouseScroll {
        delta: i32,
        direction: ScrollDirection,
        recv_ts_ns: MonotonicNs,
    },
}

impl InputEvent {
    /// Hook-receipt timestamp (set in the callback, before channel push).
    pub fn recv_ts_ns(&self) -> MonotonicNs {
        match self {
            Self::KeyPress { recv_ts_ns, .. } => *recv_ts_ns,
            Self::KeyRelease { recv_ts_ns, .. } => *recv_ts_ns,
            Self::MouseMove { recv_ts_ns, .. } => *recv_ts_ns,
            Self::MouseClick { recv_ts_ns, .. } => *recv_ts_ns,
            Self::MouseScroll { recv_ts_ns, .. } => *recv_ts_ns,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClickAction {
    Press,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScrollDirection {
    Vertical,
    Horizontal,
}

/// Errors from an `InputHookBackend`.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("Hook installation failed: {0}")]
    InstallFailed(String),
    #[error("Hook thread crashed: {0}")]
    ThreadCrash(String),
    #[error("Other hook error: {0}")]
    Other(String),
}

/// OS-level input hook backend.
///
/// Implementors MUST:
/// - Use only low-level hook variants (WH_KEYBOARD_LL / WH_MOUSE_LL).
///   These run in the installing process — NO DLL injection into other processes.
/// - Deliver events via a crossbeam channel (non-blocking push from hook callback).
/// - Apply `MouseMoveFilter` before pushing mouse-move events.
pub trait InputHookBackend: Send {
    /// Install the OS hook and start the dedicated message-loop thread.
    fn start(&mut self) -> Result<(), HookError>;

    /// Remove the OS hook and stop the message-loop thread.
    fn stop(&mut self) -> Result<(), HookError>;

    /// Receive the next event from the hook channel (non-blocking).
    ///
    /// Returns `None` if no event is pending.
    fn try_recv(&self) -> Option<InputEvent>;
}
