//! Configuration structs and YAML three-tier merge loading.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod merge;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ─── CaptureConfig ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CropMode {
    ClientArea,
    FullWindow,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureBackendType {
    #[default]
    Dxgi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureConfig {
    pub target_fps: u32,
    pub crop_to_window: bool,
    pub crop_mode: CropMode,
    pub diagnostics_enabled: bool,
    pub backend: CaptureBackendType,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            target_fps: 30,
            crop_to_window: true,
            crop_mode: CropMode::ClientArea,
            diagnostics_enabled: false,
            backend: CaptureBackendType::Dxgi,
        }
    }
}

// ─── RoiConfig ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoiConfig {
    pub reference_images: Vec<PathBuf>,
    pub match_threshold: f32,
    pub min_width_px: u32,
    pub min_height_px: u32,
    pub padding_px: u32,
    pub clamp_to_monitor: bool,
    pub lock_after_first_success: bool,
    pub max_roi_jump_px: u32,
    pub reacquire_confirm_frames: u32,
    pub reacquire_grace_seconds: f64,
}

impl Default for RoiConfig {
    fn default() -> Self {
        Self {
            reference_images: Vec::new(),
            match_threshold: 0.75,
            min_width_px: 400,
            min_height_px: 150,
            padding_px: 20,
            clamp_to_monitor: true,
            lock_after_first_success: true,
            max_roi_jump_px: 30,
            reacquire_confirm_frames: 2,
            reacquire_grace_seconds: 5.0,
        }
    }
}

// ─── SessionConfig ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub require_roi_detected: bool,
    pub roi_stable_seconds: f64,
    pub detection_timeout_seconds: f64,
    pub preroll_seconds: f64,
    pub require_window_foreground: bool,
    pub pause_when_background: bool,
    pub background_grace_seconds: f64,
    pub record_epoch_perf_pair_on_end: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            require_roi_detected: true,
            roi_stable_seconds: 1.0,
            detection_timeout_seconds: 30.0,
            preroll_seconds: 2.0,
            require_window_foreground: true,
            pause_when_background: true,
            background_grace_seconds: 3.0,
            record_epoch_perf_pair_on_end: false,
        }
    }
}

// ─── TriggerConfig ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemplateTriggerConfig {
    pub template_path: PathBuf,
    pub threshold: f32,
    pub confirm_frames: u32,
}

impl Default for TemplateTriggerConfig {
    fn default() -> Self {
        Self {
            template_path: PathBuf::from("assets/game_over.png"),
            threshold: 0.80,
            confirm_frames: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TriggerConfig {
    /// Virtual-key code for manual stop key (default: VK_F9 = 0x78).
    pub stop_key: u16,
    pub timeout_seconds: f64,
    pub game_over_template: Option<TemplateTriggerConfig>,
    pub window_lost_grace_seconds: f64,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            stop_key: 0x78, // VK_F9
            timeout_seconds: 300.0,
            game_over_template: None,
            window_lost_grace_seconds: 5.0,
        }
    }
}

// ─── EventCaptureConfig ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseMoveFilterConfig {
    pub max_hz: u32,
    pub min_delta_px: u32,
}

impl Default for MouseMoveFilterConfig {
    fn default() -> Self {
        Self {
            max_hz: 20,
            min_delta_px: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    KeyboardPress,
    KeyboardRelease,
    MouseMove,
    MouseClick,
    MouseScroll,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EventCaptureConfig {
    pub enabled_event_types: Vec<EventType>,
    pub mouse_move: Option<MouseMoveFilterConfig>,
}

impl Default for EventCaptureConfig {
    fn default() -> Self {
        Self {
            enabled_event_types: vec![
                EventType::KeyboardPress,
                EventType::KeyboardRelease,
                EventType::MouseMove,
                EventType::MouseClick,
                EventType::MouseScroll,
            ],
            mouse_move: Some(MouseMoveFilterConfig::default()),
        }
    }
}

// ─── ConsumerConfig ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsumerConfig {
    pub recording_enabled: bool,
    pub realtime_enabled: bool,
    pub ring_buffer_capacity: usize,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            recording_enabled: true,
            realtime_enabled: false,
            ring_buffer_capacity: 3,
        }
    }
}

// ─── AutomationConfig ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationMode {
    #[default]
    BatchRecording,
    RealtimeBot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    pub enabled: bool,
    pub mode: AutomationMode,
    pub restart_delay_seconds: f64,
    pub max_consecutive_failures: u32,
    pub quarantine_on_failure: bool,
    pub target_session_count: u32,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AutomationMode::BatchRecording,
            restart_delay_seconds: 3.0,
            max_consecutive_failures: 3,
            quarantine_on_failure: true,
            target_session_count: 600,
        }
    }
}

// ─── DiskProtectionConfig ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiskProtectionConfig {
    pub enabled: bool,
    pub check_interval_seconds: u64,
    pub warning_threshold_gb: u64,
    pub halt_threshold_gb: u64,
}

impl Default for DiskProtectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_seconds: 60,
            warning_threshold_gb: 20,
            halt_threshold_gb: 10,
        }
    }
}

// ─── DebugConfig ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugConfig {
    /// If `true`, spawn an independent preview window thread.
    pub enabled: bool,
}

// ─── Root profile config ──────────────────────────────────────────────────────

/// Fully-merged configuration for one pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProfileConfig {
    pub capture: CaptureConfig,
    pub roi: RoiConfig,
    pub session: SessionConfig,
    pub trigger: TriggerConfig,
    pub events: EventCaptureConfig,
    pub consumer: ConsumerConfig,
    pub automation: AutomationConfig,
    pub disk_protection: DiskProtectionConfig,
    pub debug: DebugConfig,
}

/// Load and merge the three-tier YAML config:
///   `base.yaml` ← `games/{game}.yaml` ← `profiles/{profile}.override.yaml`
///
/// Uses dict-merge, list-replace, and null-remove semantics per FR-037, FR-038, FR-039.
pub fn load_merged_profile(
    config_dir: &std::path::Path,
    game: &str,
    profile: &str,
) -> Result<ProfileConfig, Box<dyn std::error::Error>> {
    let base_path = config_dir.join("base.yaml");
    let game_path = config_dir.join("games").join(format!("{}.yaml", game));
    let profile_path = config_dir
        .join("profiles")
        .join(format!("{}.override.yaml", profile));

    let base_str = std::fs::read_to_string(&base_path)
        .map_err(|e| format!("Cannot read {}: {}", base_path.display(), e))?;
    let base: serde_yaml::Value = serde_yaml::from_str(&base_str)?;

    let merged = if game_path.exists() {
        let game_str = std::fs::read_to_string(&game_path)
            .map_err(|e| format!("Cannot read {}: {}", game_path.display(), e))?;
        let game_val: serde_yaml::Value = serde_yaml::from_str(&game_str)?;
        merge::merge_values(base, game_val)
    } else {
        base
    };

    let merged = if profile_path.exists() {
        let profile_str = std::fs::read_to_string(&profile_path)
            .map_err(|e| format!("Cannot read {}: {}", profile_path.display(), e))?;
        let profile_val: serde_yaml::Value = serde_yaml::from_str(&profile_str)?;
        merge::merge_values(merged, profile_val)
    } else {
        merged
    };

    let config: ProfileConfig = serde_yaml::from_value(merged)?;
    Ok(config)
}
