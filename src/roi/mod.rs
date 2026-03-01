//! `RoiManager` — template-match-based region-of-interest detection and tracking.
//!
//! The heavy matchTemplate work runs on a dedicated background thread.
//! The capture thread only does a non-blocking `try_send` + atomic read (~0.1 ms).
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod template;

use crate::capture::CapturedFrame;
use crate::config::RoiConfig;
use std::sync::{Arc, Mutex};
use template::{MatchResult, TemplateMatchDetector};

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

// ─── Background worker message types ─────────────────────────────────────────

/// Work item sent from the capture thread to the ROI worker.
struct WorkItem {
    /// BGRA frame data (cloned; capture thread must not block on copy).
    bgra: Vec<u8>,
    width: u32,
    height: u32,
    frame_id: u64,
    /// Current ROI state at send time — determines search strategy.
    state: RoiState,
    /// Current locked ROI (used for crop in `Locked` state re-checks).
    current_roi: Option<RoiRegion>,
}

/// Result returned by the worker to the capture thread.
#[derive(Debug, Clone, Copy)]
pub struct WorkResult {
    pub frame_id: u64,
    pub match_result: Option<MatchResult>,
}

// ─── RoiManager ───────────────────────────────────────────────────────────────

/// ROI state machine with background matchTemplate worker.
///
/// Transitions:
/// ```text
/// Searching ─── confirm_frames consecutive matches ──► Locked
/// Locked    ─── ROI shift > max_roi_jump_px         ──► Lost
/// Lost      ─── reacquired within grace period       ──► Locked
/// Lost      ─── grace exceeded                       ──► on_exceeded action
/// ```
#[allow(dead_code)]
pub struct RoiManager {
    config: RoiConfig,
    state: RoiState,
    current_roi: Option<RoiRegion>,
    lost_at_frame: Option<u64>,
    on_exceeded: GraceExceededAction,
    /// Consecutive successful matches (Searching → Locked counter).
    consecutive_hits: u32,
    /// Sender to the background worker. `None` when the opencv feature is
    /// disabled or no templates are configured.
    worker_tx: Option<crossbeam_channel::Sender<Option<WorkItem>>>,
    /// Latest result from the background worker.
    latest_result: Arc<Mutex<Option<WorkResult>>>,
    /// Background worker thread handle (kept to join on drop).
    worker_thread: Option<std::thread::JoinHandle<()>>,
}

impl RoiManager {
    pub fn new(config: RoiConfig, on_exceeded: GraceExceededAction) -> Self {
        let latest_result: Arc<Mutex<Option<WorkResult>>> = Arc::new(Mutex::new(None));

        // Only spawn the background thread when templates are configured.
        let (worker_tx, worker_thread) = if config.reference_images.is_empty() {
            (None, None)
        } else {
            let (tx, rx) = crossbeam_channel::bounded::<Option<WorkItem>>(1);
            let result_arc = Arc::clone(&latest_result);
            let threshold = config.match_threshold;
            let template_paths = config.reference_images.clone();

            let handle = std::thread::Builder::new()
                .name("roi-worker".to_string())
                .spawn(move || {
                    roi_worker(rx, result_arc, template_paths, threshold);
                })
                .ok();

            if handle.is_some() {
                (Some(tx), handle)
            } else {
                (None, None)
            }
        };

        Self {
            config,
            state: RoiState::Searching,
            current_roi: None,
            lost_at_frame: None,
            on_exceeded,
            consecutive_hits: 0,
            worker_tx,
            latest_result,
            worker_thread,
        }
    }

    pub fn state(&self) -> RoiState {
        self.state
    }

    pub fn current_roi(&self) -> Option<RoiRegion> {
        self.current_roi
    }

    /// Update the ROI state based on the latest captured frame.
    ///
    /// Called every frame on the capture thread. Non-blocking: sends work to
    /// the background thread and reads the latest result without waiting.
    pub fn update(&mut self, frame: &CapturedFrame, frame_id: u64) {
        // Read the latest result from the background worker (non-blocking).
        let maybe_result = self
            .latest_result
            .lock()
            .ok()
            .and_then(|mut guard| guard.take());

        if let Some(result) = maybe_result {
            self.apply_result(result, frame_id);
        }

        // Send the current frame to the worker (non-blocking — skip if busy).
        if let Some(tx) = &self.worker_tx {
            let item = WorkItem {
                bgra: frame.data.clone(),
                width: frame.width,
                height: frame.height,
                frame_id,
                state: self.state,
                current_roi: self.current_roi,
            };
            // `try_send` returns Err if the channel is full — intentional drop.
            let _ = tx.try_send(Some(item));
        }
    }

    /// Process a result from the background worker.
    fn apply_result(&mut self, result: WorkResult, current_frame_id: u64) {
        match self.state {
            RoiState::Searching | RoiState::Lost => {
                if let Some(mr) = result.match_result {
                    self.consecutive_hits += 1;
                    if self.consecutive_hits >= self.config.reacquire_confirm_frames {
                        // Transition → Locked.
                        self.consecutive_hits = 0;
                        let roi = build_roi(&mr, &self.config, current_frame_id);
                        self.current_roi = Some(roi);
                        self.state = RoiState::Locked;
                        self.lost_at_frame = None;
                        eprintln!(
                            "[ROI] Locked at ({}, {}) score={:.3} frame={}",
                            roi.x, roi.y, roi.match_score, current_frame_id
                        );
                    }
                } else {
                    self.consecutive_hits = 0;
                }
            }

            RoiState::Locked => {
                // Re-check if the ROI has shifted significantly.
                if let (Some(mr), Some(current)) = (result.match_result, self.current_roi) {
                    let dx = (mr.x as i64 - current.x as i64).unsigned_abs() as u32;
                    let dy = (mr.y as i64 - current.y as i64).unsigned_abs() as u32;
                    if dx > self.config.max_roi_jump_px || dy > self.config.max_roi_jump_px {
                        // Large shift — transition → Lost.
                        self.state = RoiState::Lost;
                        self.lost_at_frame = Some(current_frame_id);
                        self.consecutive_hits = 0;
                        eprintln!(
                            "[ROI] Lost at frame {} (shift dx={} dy={})",
                            current_frame_id, dx, dy
                        );
                    } else {
                        // Small shift — update position.
                        self.current_roi =
                            Some(build_roi_at_frame(&mr, &self.config, current.locked_at_frame));
                    }
                } else if result.match_result.is_none() {
                    // Template no longer visible — transition → Lost.
                    self.state = RoiState::Lost;
                    self.lost_at_frame = Some(current_frame_id);
                    self.consecutive_hits = 0;
                }
            }
        }
    }

    /// Returns `true` if the ROI has been locked for at least `stable_seconds`.
    pub fn is_stable(&self, frame_id: u64, stable_seconds: f64, fps: f64) -> bool {
        if let (RoiState::Locked, Some(roi)) = (self.state, self.current_roi) {
            let frames_since_lock = frame_id.saturating_sub(roi.locked_at_frame);
            let stable_frames = (stable_seconds * fps) as u64;
            frames_since_lock >= stable_frames
        } else {
            false
        }
    }
}

impl Drop for RoiManager {
    fn drop(&mut self) {
        // Signal the worker to shut down.
        if let Some(tx) = self.worker_tx.take() {
            let _ = tx.send(None); // None = shutdown sentinel
        }
        if let Some(handle) = self.worker_thread.take() {
            let _ = handle.join();
        }
    }
}

// ─── ROI geometry helpers ─────────────────────────────────────────────────────

fn build_roi(mr: &MatchResult, cfg: &RoiConfig, locked_at_frame: u64) -> RoiRegion {
    build_roi_at_frame(mr, cfg, locked_at_frame)
}

fn build_roi_at_frame(mr: &MatchResult, cfg: &RoiConfig, locked_at_frame: u64) -> RoiRegion {
    // Expand match top-left by padding on all sides.
    let x = mr.x.saturating_sub(cfg.padding_px);
    let y = mr.y.saturating_sub(cfg.padding_px);
    let width = cfg.min_width_px + cfg.padding_px * 2;
    let height = cfg.min_height_px + cfg.padding_px * 2;
    RoiRegion { x, y, width, height, match_score: mr.score, locked_at_frame }
}

// ─── Background worker ────────────────────────────────────────────────────────

fn roi_worker(
    rx: crossbeam_channel::Receiver<Option<WorkItem>>,
    result: Arc<Mutex<Option<WorkResult>>>,
    template_paths: Vec<std::path::PathBuf>,
    threshold: f32,
) {
    let mut detector = TemplateMatchDetector::new(template_paths, threshold);

    while let Ok(Some(item)) = rx.recv() {
        let (downsample, search_roi) = match item.state {
            RoiState::Locked => {
                // In Locked state, search only within the padded ROI area.
                let roi_crop = item.current_roi.map(|r| (r.x, r.y, r.width, r.height));
                (1.0f64, roi_crop)
            }
            RoiState::Searching | RoiState::Lost => {
                // Full-frame search at 40% scale to reduce cost.
                (0.4f64, None)
            }
        };

        let match_result =
            detector.detect_raw(&item.bgra, item.width, item.height, search_roi, downsample);

        // Write result (overwrite any unconsumed previous result — capture
        // thread always wants the freshest data).
        if let Ok(mut guard) = result.lock() {
            *guard = Some(WorkResult { frame_id: item.frame_id, match_result });
        }
    }
}
