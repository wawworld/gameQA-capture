//! `SessionWriter` — two-phase commit for session artifacts.
//!
//! Write flow:
//!   1. Write all artifacts to `data/.wip/{session_id}/`
//!   2. Write `session.json` last (commit marker)
//!   3. Rename `data/.wip/{session_id}/` → `data/sessions/{session_id}/` (atomic on NTFS)
//!   4. On failure before step 3: move to `data/incomplete/{session_id}/`
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::config::DiskProtectionConfig;
use crate::session::{SessionId, SessionRecord};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ─── Session artifact validation ──────────────────────────────────────────────

/// Validation report produced by [`validate_session_artifacts`].
#[derive(Debug)]
pub struct ValidationReport {
    /// Session ID extracted from `session.json`.
    pub session_id: String,
    /// Number of frames in `frames.jsonl`.
    pub frame_count: u64,
    /// Number of events in `events.jsonl`.
    pub event_count: u64,
    /// `true` if `video.mp4` is present and non-empty.
    pub has_video: bool,
    /// Schema violations found. Empty ⟹ all checks passed.
    pub errors: Vec<String>,
}

impl ValidationReport {
    /// `true` when no schema violations were found.
    pub fn passed(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Validate all artifacts in a committed session directory against
/// `contracts/session-schema.md`.
///
/// Checks performed:
/// - `session.json` — required fields present, `ts_mono_start_ns == 0`, status = "complete"
/// - `frames.jsonl` — parseable JSON, `frame_id` strictly increasing from 0, `capture_ts_ns` present
/// - `events.jsonl` — parseable JSON, `event_id` strictly increasing, `event_ts_ns ≤ queue_push_done_ts_ns`
/// - `video.mp4`    — if present, must be non-empty
pub fn validate_session_artifacts(session_dir: &Path) -> Result<ValidationReport, String> {
    let mut errors: Vec<String> = Vec::new();
    let mut session_id = String::new();
    let mut frame_count = 0u64;
    let mut event_count = 0u64;
    let mut has_video = false;

    // ── 1. session.json ───────────────────────────────────────────────────────
    let session_json_path = session_dir.join("session.json");
    match fs::read_to_string(&session_json_path) {
        Err(e) => errors.push(format!("session.json missing or unreadable: {e}")),
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Err(e) => errors.push(format!("session.json is not valid JSON: {e}")),
            Ok(v) => {
                for field in &[
                    "session_id", "ts_wall_start_ns", "ts_mono_start_ns",
                    "game_profile", "active_profile", "mode", "gap_flag", "status",
                ] {
                    if v.get(field).is_none() {
                        errors.push(format!("session.json: missing required field '{field}'"));
                    }
                }
                if v.get("ts_mono_start_ns").and_then(|n| n.as_u64()) != Some(0) {
                    errors.push("session.json: ts_mono_start_ns must be 0".to_string());
                }
                if let Some(status) = v.get("status").and_then(|s| s.as_str()) {
                    if status != "complete" {
                        errors.push(format!(
                            "session.json: status must be 'complete', got '{status}'"
                        ));
                    }
                }
                if let Some(id) = v.get("session_id").and_then(|s| s.as_str()) {
                    session_id = id.to_string();
                }
            }
        },
    }

    // ── 2. frames.jsonl ───────────────────────────────────────────────────────
    let frames_path = session_dir.join("frames.jsonl");
    match fs::read_to_string(&frames_path) {
        Err(e) => errors.push(format!("frames.jsonl missing or unreadable: {e}")),
        Ok(content) => {
            let mut expected_id: u64 = 0;
            for (i, line) in content.lines().enumerate() {
                if line.is_empty() { continue; }
                match serde_json::from_str::<serde_json::Value>(line) {
                    Err(e) => {
                        errors.push(format!("frames.jsonl line {i}: invalid JSON: {e}"));
                        break;
                    }
                    Ok(v) => {
                        match v.get("frame_id").and_then(|n| n.as_u64()) {
                            None => errors.push(format!("frames.jsonl line {i}: missing frame_id")),
                            Some(id) if id != expected_id => {
                                errors.push(format!(
                                    "frames.jsonl: frame_id gap at line {i}: expected {expected_id}, got {id}"
                                ));
                                expected_id = id + 1;
                            }
                            Some(_) => expected_id += 1,
                        }
                        if v.get("capture_ts_ns").is_none() {
                            errors.push(format!("frames.jsonl line {i}: missing capture_ts_ns"));
                        }
                        frame_count += 1;
                    }
                }
            }
        }
    }

    // ── 3. events.jsonl ───────────────────────────────────────────────────────
    let events_path = session_dir.join("events.jsonl");
    match fs::read_to_string(&events_path) {
        Err(e) => errors.push(format!("events.jsonl missing or unreadable: {e}")),
        Ok(content) => {
            let mut expected_id: u64 = 0;
            for (i, line) in content.lines().enumerate() {
                if line.is_empty() { continue; }
                match serde_json::from_str::<serde_json::Value>(line) {
                    Err(e) => {
                        errors.push(format!("events.jsonl line {i}: invalid JSON: {e}"));
                        break;
                    }
                    Ok(v) => {
                        match v.get("event_id").and_then(|n| n.as_u64()) {
                            None => errors.push(format!("events.jsonl line {i}: missing event_id")),
                            Some(id) if id != expected_id => {
                                errors.push(format!(
                                    "events.jsonl: event_id gap at line {i}: expected {expected_id}, got {id}"
                                ));
                                expected_id = id + 1;
                            }
                            Some(_) => expected_id += 1,
                        }
                        // event_ts_ns ≤ queue_push_done_ts_ns (hook latency invariant)
                        if let (Some(ts), Some(push_done)) = (
                            v.get("event_ts_ns").and_then(|n| n.as_u64()),
                            v.get("queue_push_done_ts_ns").and_then(|n| n.as_u64()),
                        ) {
                            if ts > push_done {
                                errors.push(format!(
                                    "events.jsonl line {i}: event_ts_ns ({ts}) > queue_push_done_ts_ns ({push_done})"
                                ));
                            }
                        }
                        event_count += 1;
                    }
                }
            }
        }
    }

    // ── 4. video.mp4 (optional, non-empty if present) ─────────────────────────
    let video_path = session_dir.join("video.mp4");
    if video_path.exists() {
        let size = fs::metadata(&video_path).map(|m| m.len()).unwrap_or(0);
        if size == 0 {
            errors.push("video.mp4 exists but is empty (zero bytes)".to_string());
        } else {
            has_video = true;
        }
    }

    Ok(ValidationReport { session_id, frame_count, event_count, has_video, errors })
}

/// Errors from session storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Commit rename failed: {0}")]
    CommitFailed(String),
}

/// Manages artifact writing and two-phase commit for one session.
pub struct SessionWriter {
    session_id: SessionId,
    data_root: PathBuf,
    wip_dir: PathBuf,
}

impl SessionWriter {
    /// Create a new writer; the `.wip` directory is created immediately.
    pub fn new(data_root: PathBuf, session_id: SessionId) -> Result<Self, StorageError> {
        let wip_dir = data_root.join(".wip").join(session_id.as_str());
        fs::create_dir_all(&wip_dir)?;
        Ok(Self {
            session_id,
            data_root,
            wip_dir,
        })
    }

    /// The working directory where artifacts are written during the session.
    pub fn session_dir(&self) -> &Path {
        &self.wip_dir
    }

    /// Path to write an artifact file within the wip directory.
    pub fn artifact_path(&self, filename: &str) -> PathBuf {
        self.wip_dir.join(filename)
    }

    /// Commit the session: write `session.json`, then rename wip → sessions.
    ///
    /// On success, the wip directory is gone and the session directory is live.
    pub fn commit(self, record: &SessionRecord) -> Result<PathBuf, StorageError> {
        // Write session.json last (commit marker).
        let json = serde_json::to_string_pretty(record)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        let session_json_path = self.wip_dir.join("session.json");
        fs::write(&session_json_path, json)?;

        // Atomic rename: .wip/{id}/ → sessions/{id}/
        let sessions_dir = self.data_root.join("sessions");
        fs::create_dir_all(&sessions_dir)?;
        let dest = sessions_dir.join(self.session_id.as_str());

        fs::rename(&self.wip_dir, &dest).map_err(|e| {
            StorageError::CommitFailed(format!(
                "{} → {}: {}",
                self.wip_dir.display(),
                dest.display(),
                e
            ))
        })?;

        Ok(dest)
    }

    /// Abort the session: move wip → incomplete.
    ///
    /// Called on any error before commit.
    pub fn abort(self) -> Result<(), StorageError> {
        let incomplete_dir = self
            .data_root
            .join("incomplete")
            .join(self.session_id.as_str());
        let incomplete_parent = incomplete_dir.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(incomplete_parent)?;

        if self.wip_dir.exists() {
            fs::rename(&self.wip_dir, &incomplete_dir).map_err(|e| {
                StorageError::CommitFailed(format!(
                    "{} → {}: {}",
                    self.wip_dir.display(),
                    incomplete_dir.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    /// Check disk free space on the volume containing `data_root`.
    ///
    /// Returns free bytes, or `None` if the check fails.
    pub fn check_disk_free_bytes(data_root: &Path) -> Option<u64> {
        disk_free_bytes(data_root)
    }
}

// ─── Disk-free query ──────────────────────────────────────────────────────────

/// Query free bytes on the volume that contains `path`.
///
/// Uses `GetDiskFreeSpaceExW` on Windows; falls back to `None` on error.
fn disk_free_bytes(path: &Path) -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        use windows::core::PCWSTR;

        // Build a NUL-terminated wide string for the path.
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0u16))
            .collect();

        let mut free_bytes_caller: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut free_bytes_total: u64 = 0;

        // SAFETY: `wide` is a valid NUL-terminated UTF-16 path on the heap.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(wide.as_ptr()),
                Some(&mut free_bytes_caller),
                Some(&mut total_bytes),
                Some(&mut free_bytes_total),
            )
        };
        if ok.is_ok() {
            Some(free_bytes_caller)
        } else {
            None
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Non-Windows stub — not used in production.
        let _ = path;
        None
    }
}

// ─── DiskWatcher ─────────────────────────────────────────────────────────────

/// Disk-space watcher that fires a halt signal when free space drops too low.
///
/// Spawns a background thread that checks every `check_interval_seconds`.
/// Sets `halt_flag` to `true` when the halt threshold is crossed.
pub struct DiskWatcher {
    /// Set to `true` by the watcher thread when free space < `halt_threshold_gb`.
    pub halt_flag: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl DiskWatcher {
    /// Start the watcher background thread.
    ///
    /// Returns `None` if `config.enabled` is false.
    pub fn start(data_root: PathBuf, config: DiskProtectionConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let halt_flag = Arc::new(AtomicBool::new(false));
        let halt_flag2 = Arc::clone(&halt_flag);
        let interval = Duration::from_secs(config.check_interval_seconds.max(1));
        let warn_bytes = config.warning_threshold_gb * 1_073_741_824; // GiB → bytes
        let halt_bytes = config.halt_threshold_gb * 1_073_741_824;

        let handle = std::thread::Builder::new()
            .name("disk-watcher".to_string())
            .spawn(move || {
                let mut last_check = Instant::now() - interval; // check immediately on start
                loop {
                    if last_check.elapsed() >= interval {
                        last_check = Instant::now();
                        match disk_free_bytes(&data_root) {
                            Some(free) => {
                                if free < halt_bytes {
                                    eprintln!(
                                        "[HALT] Disk free {:.1} GiB < halt threshold {:.1} GiB — stopping session",
                                        free as f64 / 1e9,
                                        config.halt_threshold_gb as f64
                                    );
                                    halt_flag2.store(true, Ordering::Relaxed);
                                    break;
                                } else if free < warn_bytes {
                                    eprintln!(
                                        "[WARN] Disk free {:.1} GiB < warning threshold {:.1} GiB",
                                        free as f64 / 1e9,
                                        config.warning_threshold_gb as f64
                                    );
                                }
                            }
                            None => {
                                // Check failed — log but continue.
                                eprintln!("[WARN] DiskWatcher: could not read free space");
                            }
                        }
                    }
                    // Sleep in small slices so the thread can exit promptly.
                    std::thread::sleep(Duration::from_millis(500));

                    // Exit if halt already set by an external source.
                    if halt_flag2.load(Ordering::Relaxed) {
                        break;
                    }
                }
            })
            .ok()?;

        Some(Self {
            halt_flag,
            handle: Some(handle),
        })
    }

    /// Signal the watcher to stop and wait for it to exit.
    pub fn stop(&mut self) {
        // Signal stop via halt_flag (watcher exits on next sleep cycle).
        self.halt_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for DiskWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::session::{SessionId, SessionMode, SessionRecord, SessionStatus};
    use crate::{MonotonicNs, WallNs};

    fn make_session_id() -> SessionId {
        SessionId("test_session_001".to_string())
    }

    fn make_record() -> SessionRecord {
        SessionRecord {
            session_id: "test_session_001".to_string(),
            ts_wall_start_ns: WallNs(0),
            ts_mono_start_ns: MonotonicNs(0),
            ts_wall_end_ns: Some(WallNs(1_000_000_000)),
            ts_mono_end_ns: Some(MonotonicNs(1_000_000_000)),
            game_profile: "chrome_dino".to_string(),
            active_profile: "test".to_string(),
            mode: SessionMode::Recording,
            gap_flag: false,
            status: SessionStatus::Complete,
        }
    }

    #[test]
    fn two_phase_commit_creates_session_dir() {
        let root = tempfile::tempdir().unwrap();
        let writer = SessionWriter::new(root.path().to_path_buf(), make_session_id()).unwrap();
        let wip = writer.wip_dir.clone();

        assert!(wip.exists());
        writer.commit(&make_record()).unwrap();
        assert!(!wip.exists());
        assert!(root.path().join("sessions").join("test_session_001").exists());
    }

    #[test]
    fn abort_moves_to_incomplete() {
        let root = tempfile::tempdir().unwrap();
        let writer = SessionWriter::new(root.path().to_path_buf(), make_session_id()).unwrap();
        writer.abort().unwrap();
        assert!(root.path().join("incomplete").join("test_session_001").exists());
    }

    #[test]
    fn check_disk_free_bytes_returns_some_on_existing_path() {
        let root = tempfile::tempdir().unwrap();
        let free = SessionWriter::check_disk_free_bytes(root.path());
        // On Windows this should return a value; on other OSes it's None (stub).
        #[cfg(target_os = "windows")]
        assert!(free.is_some(), "expected disk free bytes on Windows");
        #[cfg(not(target_os = "windows"))]
        let _ = free; // Stub returns None — just ensure no panic
    }

    #[test]
    fn disk_watcher_starts_and_stops() {
        let root = tempfile::tempdir().unwrap();
        let config = DiskProtectionConfig {
            enabled: true,
            check_interval_seconds: 60, // long interval — won't actually check during test
            warning_threshold_gb: 1,
            halt_threshold_gb: 0, // threshold of 0 = never halt
        };
        let mut watcher = DiskWatcher::start(root.path().to_path_buf(), config);
        assert!(watcher.is_some());
        if let Some(ref mut w) = watcher {
            w.stop();
        }
    }

    #[test]
    fn disk_watcher_disabled_returns_none() {
        let root = tempfile::tempdir().unwrap();
        let config = DiskProtectionConfig {
            enabled: false,
            ..Default::default()
        };
        let watcher = DiskWatcher::start(root.path().to_path_buf(), config);
        assert!(watcher.is_none());
    }

    // ── validate_session_artifacts tests ─────────────────────────────────────

    fn write_valid_session(dir: &std::path::Path) {
        let json = serde_json::json!({
            "session_id": "20260101T000000Z-chrome_dino-test",
            "ts_wall_start_ns": 0u64,
            "ts_mono_start_ns": 0u64,
            "game_profile": "chrome_dino",
            "active_profile": "test",
            "mode": "recording",
            "gap_flag": false,
            "status": "complete"
        });
        fs::write(dir.join("session.json"), json.to_string()).unwrap();

        let frames: String = (0..5u64)
            .map(|i| {
                serde_json::json!({ "frame_id": i, "capture_ts_ns": i * 33_333_333u64 })
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(dir.join("frames.jsonl"), frames).unwrap();
        fs::write(dir.join("events.jsonl"), "").unwrap();
    }

    #[test]
    fn validate_passes_on_minimal_valid_session() {
        let dir = tempfile::tempdir().unwrap();
        write_valid_session(dir.path());
        let report = validate_session_artifacts(dir.path()).unwrap();
        assert!(report.passed(), "unexpected errors: {:?}", report.errors);
        assert_eq!(report.frame_count, 5);
        assert_eq!(report.event_count, 0);
        assert!(!report.has_video);
    }

    #[test]
    fn validate_detects_missing_session_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("frames.jsonl"),
            r#"{"frame_id":0,"capture_ts_ns":0}"#,
        )
        .unwrap();
        fs::write(dir.path().join("events.jsonl"), "").unwrap();

        let report = validate_session_artifacts(dir.path()).unwrap();
        assert!(!report.passed());
        assert!(report.errors.iter().any(|e| e.contains("session.json")));
    }

    #[test]
    fn validate_detects_frame_id_gap() {
        let dir = tempfile::tempdir().unwrap();
        write_valid_session(dir.path()); // writes sequential frame IDs 0-4
        // Overwrite with a gap: 0, 1, 3 (skip 2)
        let frames = [0u64, 1, 3]
            .iter()
            .map(|i| {
                serde_json::json!({ "frame_id": i, "capture_ts_ns": i * 33_333_333u64 })
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(dir.path().join("frames.jsonl"), frames).unwrap();

        let report = validate_session_artifacts(dir.path()).unwrap();
        assert!(!report.passed());
        assert!(report.errors.iter().any(|e| e.contains("frame_id gap")));
    }

    #[test]
    fn validate_detects_missing_required_field_in_session_json() {
        let dir = tempfile::tempdir().unwrap();
        // session.json missing the `status` field
        let json = serde_json::json!({
            "session_id": "x",
            "ts_wall_start_ns": 0u64,
            "ts_mono_start_ns": 0u64,
            "game_profile": "chrome_dino",
            "active_profile": "test",
            "mode": "recording",
            "gap_flag": false
            // no "status"
        });
        fs::write(dir.path().join("session.json"), json.to_string()).unwrap();
        fs::write(dir.path().join("frames.jsonl"), "").unwrap();
        fs::write(dir.path().join("events.jsonl"), "").unwrap();

        let report = validate_session_artifacts(dir.path()).unwrap();
        assert!(!report.passed());
        assert!(report.errors.iter().any(|e| e.contains("status")));
    }

    #[test]
    fn validate_passes_with_valid_video_mp4() {
        let dir = tempfile::tempdir().unwrap();
        write_valid_session(dir.path());
        // Write a fake non-empty video.mp4
        fs::write(dir.path().join("video.mp4"), b"fake mp4 data").unwrap();

        let report = validate_session_artifacts(dir.path()).unwrap();
        assert!(report.passed(), "unexpected errors: {:?}", report.errors);
        assert!(report.has_video);
    }

    #[test]
    fn validate_detects_empty_video_mp4() {
        let dir = tempfile::tempdir().unwrap();
        write_valid_session(dir.path());
        fs::write(dir.path().join("video.mp4"), b"").unwrap(); // empty

        let report = validate_session_artifacts(dir.path()).unwrap();
        assert!(!report.passed());
        assert!(report.errors.iter().any(|e| e.contains("video.mp4")));
    }
}
