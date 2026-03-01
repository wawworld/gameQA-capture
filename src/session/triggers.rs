//! `TerminationTrigger` trait and built-in implementations.
//!
//! Triggers are pure observers/detectors — they MUST NOT modify session state.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use crate::hooks::InputEvent;
use std::sync::{Arc, Mutex};
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

/// Work item sent to the background matchTemplate thread.
struct TemplateWork {
    bgra: Vec<u8>,
    width: u32,
    height: u32,
}

/// Terminates the session when a game-over template is matched for `confirm_frames`
/// consecutive background-thread results above `threshold`.
///
/// The heavy `matchTemplate` work runs on a dedicated background thread.
/// The capture thread only does a non-blocking `try_send` + mutex read (~0.1 ms).
pub struct TemplateMatchTrigger {
    threshold: f32,
    confirm_frames: u32,
    /// Consecutive background results that exceed `threshold`.
    consecutive_hits: u32,
    /// Sender to the background worker. `None` when opencv feature is off.
    worker_tx: Option<crossbeam_channel::Sender<Option<TemplateWork>>>,
    /// Latest match score from the background worker.
    latest_score: Arc<Mutex<Option<f32>>>,
    /// Background thread handle (for clean shutdown).
    worker_thread: Option<std::thread::JoinHandle<()>>,
}

impl TemplateMatchTrigger {
    pub fn new(
        template_path: std::path::PathBuf,
        threshold: f32,
        confirm_frames: u32,
    ) -> Self {
        let latest_score: Arc<Mutex<Option<f32>>> = Arc::new(Mutex::new(None));

        let (worker_tx, worker_thread) = {
            let (tx, rx) = crossbeam_channel::bounded::<Option<TemplateWork>>(1);
            let score_arc = Arc::clone(&latest_score);

            let handle = std::thread::Builder::new()
                .name("tmpl-trigger-worker".to_string())
                .spawn(move || {
                    template_trigger_worker(rx, score_arc, template_path, threshold);
                })
                .ok();

            if handle.is_some() {
                (Some(tx), handle)
            } else {
                (None, None)
            }
        };

        Self {
            threshold,
            confirm_frames,
            consecutive_hits: 0,
            worker_tx,
            latest_score,
            worker_thread,
        }
    }
}

impl TerminationTrigger for TemplateMatchTrigger {
    fn check(
        &mut self,
        frame: &CapturedFrame,
        _frame_id: u64,
        _events: &[InputEvent],
    ) -> TriggerState {
        // Read the latest score from the worker (non-blocking).
        let score = self
            .latest_score
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());

        // Update consecutive_hits based on the latest result.
        match score {
            Some(s) if s >= self.threshold => {
                self.consecutive_hits += 1;
            }
            Some(_) | None => {
                // No result yet (first frame) or score below threshold.
                // Only reset if we actually got a "no match" result.
                if score.is_some() {
                    self.consecutive_hits = 0;
                }
            }
        }

        // Send the current frame to the worker (non-blocking).
        if let Some(tx) = &self.worker_tx {
            let work = TemplateWork {
                bgra: frame.data.clone(),
                width: frame.width,
                height: frame.height,
            };
            let _ = tx.try_send(Some(work));
        }

        if self.consecutive_hits >= self.confirm_frames {
            TriggerState::Terminate {
                reason: format!(
                    "Game-over template matched ({} consecutive, score≥{:.2})",
                    self.consecutive_hits, self.threshold
                ),
                requires_confirmation: false,
            }
        } else {
            TriggerState::Continue
        }
    }
}

impl Drop for TemplateMatchTrigger {
    fn drop(&mut self) {
        if let Some(tx) = self.worker_tx.take() {
            let _ = tx.send(None); // shutdown sentinel
        }
        if let Some(handle) = self.worker_thread.take() {
            let _ = handle.join();
        }
    }
}

// ─── TemplateMatchTrigger background worker ───────────────────────────────────

fn template_trigger_worker(
    rx: crossbeam_channel::Receiver<Option<TemplateWork>>,
    score: Arc<Mutex<Option<f32>>>,
    template_path: std::path::PathBuf,
    threshold: f32,
) {
    use crate::roi::template::TemplateMatchDetector;

    let mut detector = TemplateMatchDetector::new(vec![template_path], threshold);

    while let Ok(Some(item)) = rx.recv() {
        // Search full frame at 40% scale — cheap and sufficient for full-screen
        // game-over overlays.
        let result = detector.detect_raw(&item.bgra, item.width, item.height, None, 0.4);

        let new_score = result.map(|mr| mr.score).or(Some(0.0));

        if let Ok(mut guard) = score.lock() {
            *guard = new_score;
        }
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
