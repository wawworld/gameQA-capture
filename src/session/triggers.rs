//! `TerminationTrigger` trait and built-in implementations.
//!
//! Triggers are pure observers/detectors — they MUST NOT modify session state.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use crate::hooks::InputEvent;
use std::time::Instant;

/// Decision returned by a trigger each frame.
pub enum TriggerState {
    /// Session continues normally.
    Continue,
    /// Session should end with the given reason.
    Terminate {
        reason: String,
        requires_confirmation: bool,
    },
}

/// A session termination detector.
///
/// Called once per captured frame by the `SessionManager`.
/// Implementations MUST be fast (< 0.5 ms per call).
pub trait TerminationTrigger: Send {
    /// Evaluate whether the session should end.
    ///
    /// `events` contains all input events received since the previous call.
    fn check(
        &mut self,
        frame: &CapturedFrame,
        frame_id: u64,
        events: &[InputEvent],
    ) -> TriggerState;
}

// ─── KeyPressTrigger ─────────────────────────────────────────────────────────

/// Terminates the session when the configured virtual key is pressed.
///
/// Default key: VK_F9 (0x78).
pub struct KeyPressTrigger {
    /// Virtual-key code that triggers termination.
    stop_vk: u16,
}

impl KeyPressTrigger {
    pub fn new(stop_vk: u16) -> Self {
        Self { stop_vk }
    }
}

impl TerminationTrigger for KeyPressTrigger {
    fn check(
        &mut self,
        _frame: &CapturedFrame,
        _frame_id: u64,
        events: &[InputEvent],
    ) -> TriggerState {
        for event in events {
            if let InputEvent::KeyPress { vk_code, .. } = event {
                if *vk_code == self.stop_vk {
                    return TriggerState::Terminate {
                        reason: format!("Key VK_{} pressed", self.stop_vk),
                        requires_confirmation: false,
                    };
                }
            }
        }
        TriggerState::Continue
    }
}

// ─── TimeoutTrigger ──────────────────────────────────────────────────────────

/// Terminates the session after a configured duration elapses.
#[allow(dead_code)]
pub struct TimeoutTrigger {
    /// Session start instant (same origin as frame timestamps).
    session_start: Instant,
    /// Maximum session duration in nanoseconds.
    timeout_ns: u64,
}

impl TimeoutTrigger {
    pub fn new(session_start: Instant, timeout_seconds: f64) -> Self {
        Self {
            session_start,
            timeout_ns: (timeout_seconds * 1_000_000_000.0) as u64,
        }
    }
}

impl TerminationTrigger for TimeoutTrigger {
    fn check(
        &mut self,
        frame: &CapturedFrame,
        _frame_id: u64,
        _events: &[InputEvent],
    ) -> TriggerState {
        if frame.capture_ts_ns.0 >= self.timeout_ns {
            return TriggerState::Terminate {
                reason: format!(
                    "Session timeout ({:.1}s)",
                    self.timeout_ns as f64 / 1_000_000_000.0
                ),
                requires_confirmation: false,
            };
        }
        TriggerState::Continue
    }
}

// ─── TemplateMatchTrigger ────────────────────────────────────────────────────

/// Terminates the session when a game-over template is matched for `confirm_frames`
/// consecutive frames above `threshold`.
///
/// Uses `opencv::imgproc::match_template` (TM_CCOEFF_NORMED).
#[allow(dead_code)]
pub struct TemplateMatchTrigger {
    /// Path to the game-over reference image.
    template_path: std::path::PathBuf,
    /// Match score threshold [0.0, 1.0].
    threshold: f32,
    /// Number of consecutive matches required before firing.
    confirm_frames: u32,
    /// Current consecutive match counter.
    consecutive_hits: u32,
}

impl TemplateMatchTrigger {
    pub fn new(template_path: std::path::PathBuf, threshold: f32, confirm_frames: u32) -> Self {
        Self {
            template_path,
            threshold,
            confirm_frames,
            consecutive_hits: 0,
        }
    }
}

impl TerminationTrigger for TemplateMatchTrigger {
    fn check(
        &mut self,
        _frame: &CapturedFrame,
        _frame_id: u64,
        _events: &[InputEvent],
    ) -> TriggerState {
        // TODO(T031): call opencv match_template on frame.data against self.template_path.
        // For now, never match.
        self.consecutive_hits = 0;
        TriggerState::Continue
    }
}

// ─── WindowLostTrigger ───────────────────────────────────────────────────────

/// Terminates the session after the game window has been absent for
/// `window_lost_grace_seconds`.
#[allow(dead_code)]
pub struct WindowLostTrigger {
    /// Win32 HWND as a usize (null = window not yet found or already lost).
    hwnd: usize,
    /// How long the window can be absent before firing (nanoseconds).
    grace_ns: u64,
    /// When the window was first detected as absent (`None` = window currently present).
    absent_since_ns: Option<u64>,
}

impl WindowLostTrigger {
    pub fn new(hwnd: usize, grace_seconds: f64) -> Self {
        Self {
            hwnd,
            grace_ns: (grace_seconds * 1_000_000_000.0) as u64,
            absent_since_ns: None,
        }
    }
}

impl TerminationTrigger for WindowLostTrigger {
    fn check(
        &mut self,
        frame: &CapturedFrame,
        _frame_id: u64,
        _events: &[InputEvent],
    ) -> TriggerState {
        // TODO(T031): call Win32 IsWindow(self.hwnd as HWND).
        // For now, assume window is always present.
        let window_present = true;
        let now_ns = frame.capture_ts_ns.0;

        if window_present {
            self.absent_since_ns = None;
            return TriggerState::Continue;
        }

        match self.absent_since_ns {
            None => {
                self.absent_since_ns = Some(now_ns);
                TriggerState::Continue
            }
            Some(start_ns) => {
                if now_ns.saturating_sub(start_ns) >= self.grace_ns {
                    TriggerState::Terminate {
                        reason: format!(
                            "Game window absent for >{:.1}s",
                            self.grace_ns as f64 / 1_000_000_000.0
                        ),
                        requires_confirmation: false,
                    }
                } else {
                    TriggerState::Continue
                }
            }
        }
    }
}
