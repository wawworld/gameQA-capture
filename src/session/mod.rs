//! Session types: `SessionRecord`, `SessionStatus`, `SessionMode`.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod lifecycle;
pub mod storage;
pub mod triggers;

use crate::{MonotonicNs, WallNs};
use serde::{Deserialize, Serialize};

/// Unique session identifier.
///
/// Format: `{YYYYMMDDTHHMMSSZ}-{game}-{profile}`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session lifecycle status — written to `session.json` at commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Complete,
    Incomplete,
    LowQuality,
}

/// Capture mode — which consumers are active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    Recording,
    Realtime,
    Both,
}

/// Per-session metadata record — serialized to `session.json`.
///
/// Written atomically as the final step of the two-phase commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub ts_wall_start_ns: WallNs,
    /// Always 0 — all frame/event timestamps are relative to session start.
    pub ts_mono_start_ns: MonotonicNs,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts_wall_end_ns: Option<WallNs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts_mono_end_ns: Option<MonotonicNs>,
    pub game_profile: String,
    pub active_profile: String,
    pub mode: SessionMode,
    /// `true` if any frame interval Δt > 66 ms occurred during the session.
    pub gap_flag: bool,
    pub status: SessionStatus,
}

impl SessionRecord {
    pub fn new(
        session_id: String,
        ts_wall_start_ns: WallNs,
        game_profile: String,
        active_profile: String,
        mode: SessionMode,
    ) -> Self {
        Self {
            session_id,
            ts_wall_start_ns,
            ts_mono_start_ns: MonotonicNs(0),
            ts_wall_end_ns: None,
            ts_mono_end_ns: None,
            game_profile,
            active_profile,
            mode,
            gap_flag: false,
            status: SessionStatus::Incomplete,
        }
    }
}
