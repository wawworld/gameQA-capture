//! Session lifecycle state machine.
//!
//! Phases: preroll discard → ROI stabilization → active capture → graceful stop.
//! FR-015: window-foreground detection (pause/resume on background).
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::config::ProfileConfig;
use crate::roi::RoiManager;

/// Lifecycle phase of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    /// Waiting for ROI detection (preroll discard active).
    WaitingForRoi,
    /// ROI detected; waiting for `roi_stable_seconds` of stability before recording.
    RoiStabilizing,
    /// Active capture: frames and events are being recorded.
    Capturing,
    /// Session is paused because the game window lost foreground (FR-015).
    Paused,
    /// Terminal states.
    Complete,
    Incomplete,
}

/// Session lifecycle manager.
///
/// Drives the pipeline through preroll, ROI stabilization, active capture,
/// and graceful shutdown — including window-foreground pause/resume (FR-015).
#[allow(dead_code)]
pub struct SessionLifecycle {
    config: ProfileConfig,
    phase: LifecyclePhase,
}

impl SessionLifecycle {
    pub fn new(config: ProfileConfig) -> Self {
        Self {
            config,
            phase: LifecyclePhase::WaitingForRoi,
        }
    }

    pub fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Advance the lifecycle state machine.
    ///
    /// Called from the pipeline loop each frame.
    /// Returns `true` when capture should begin (ROI locked and stable).
    pub fn advance(&mut self, _roi_manager: &RoiManager, _frame_id: u64) -> bool {
        // TODO(T029): implement preroll discard, ROI stabilization wait,
        // window-foreground detection (FR-015), and active capture start.
        matches!(self.phase, LifecyclePhase::Capturing)
    }
}
