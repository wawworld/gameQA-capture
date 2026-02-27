//! `RoiManager` — template-match-based region-of-interest detection and tracking.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod template;

use crate::capture::CapturedFrame;
use crate::config::RoiConfig;

/// Runtime ROI region (monitor-relative pixels).
#[derive(Debug, Clone, Copy)]
pub struct RoiRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub match_score: f32,
    pub locked_at_frame: u64,
}

/// ROI state machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoiState {
    /// No lock established yet (or re-acquisition in progress).
    Searching,
    /// ROI is locked and stable.
    Locked,
    /// Lock was lost; re-acquisition in progress within grace period.
    Lost,
}

/// What to do when the reacquisition grace period expires.
pub enum GraceExceededAction {
    /// Mark the session as `low_quality` and continue.
    MarkLowQuality,
    /// Stop the bot immediately.
    StopBot,
}

/// ROI state machine.
///
/// Transitions:
/// ```text
/// Searching ──score ≥ threshold, confirmed──► Locked
/// Locked ──shift > max_roi_jump_px──► Lost
/// Lost ──reacquired within grace──► Locked
/// Lost ──grace exceeded──► on_exceeded action
/// ```
#[allow(dead_code)]
pub struct RoiManager {
    config: RoiConfig,
    state: RoiState,
    current_roi: Option<RoiRegion>,
    lost_at_frame: Option<u64>,
    on_exceeded: GraceExceededAction,
}

impl RoiManager {
    pub fn new(config: RoiConfig, on_exceeded: GraceExceededAction) -> Self {
        Self {
            config,
            state: RoiState::Searching,
            current_roi: None,
            lost_at_frame: None,
            on_exceeded,
        }
    }

    pub fn state(&self) -> RoiState {
        self.state
    }

    pub fn current_roi(&self) -> Option<RoiRegion> {
        self.current_roi
    }

    /// Update the ROI state based on the latest captured frame.
    pub fn update(&mut self, _frame: &CapturedFrame) {
        // TODO(T026): run TemplateMatchDetector, transition state machine.
    }

    /// Returns `true` if the ROI has been locked for at least `stable_seconds`.
    pub fn is_stable(&self, _frame_id: u64, _stable_seconds: f64, _fps: f64) -> bool {
        matches!(self.state, RoiState::Locked)
    }
}
