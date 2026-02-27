//! `TemplateMatchDetector` — OpenCV `matchTemplate` (TM_CCOEFF_NORMED).
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::capture::CapturedFrame;
use std::path::PathBuf;

/// Result of a single template match attempt.
#[derive(Debug, Clone, Copy)]
pub struct MatchResult {
    /// Top-left corner of the best match (monitor-relative pixels).
    pub x: u32,
    pub y: u32,
    /// Match quality score [0.0, 1.0].
    pub score: f32,
}

/// Template-matching detector using `opencv::imgproc::match_template`.
#[allow(dead_code)]
pub struct TemplateMatchDetector {
    template_paths: Vec<PathBuf>,
    threshold: f32,
    /// How many consecutive frames above threshold are required to confirm a match.
    confirm_frames: u32,
    consecutive_hits: u32,
}

impl TemplateMatchDetector {
    pub fn new(template_paths: Vec<PathBuf>, threshold: f32, confirm_frames: u32) -> Self {
        Self {
            template_paths,
            threshold,
            confirm_frames,
            consecutive_hits: 0,
        }
    }

    /// Attempt to match any reference template against `frame`.
    ///
    /// Returns `Some(MatchResult)` if a confirmed match is found.
    pub fn detect(&mut self, _frame: &CapturedFrame) -> Option<MatchResult> {
        // TODO(T026): load each template (once, cached), call opencv match_template,
        // apply confirmation counter logic.
        None
    }
}
