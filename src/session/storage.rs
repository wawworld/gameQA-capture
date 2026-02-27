//! `SessionWriter` — two-phase commit for session artifacts.
//!
//! Write flow:
//!   1. Write all artifacts to `data/.wip/{session_id}/`
//!   2. Write `session.json` last (commit marker)
//!   3. Rename `data/.wip/{session_id}/` → `data/sessions/{session_id}/` (atomic on NTFS)
//!   4. On failure before step 3: move to `data/incomplete/{session_id}/`
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::session::{SessionId, SessionRecord};
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Returns free space in bytes.  Returns `None` if the check fails.
    pub fn check_disk_free_bytes(_data_root: &Path) -> Option<u64> {
        // TODO(T041): use winapi GetDiskFreeSpaceEx or std::fs::metadata approach.
        None
    }
}
