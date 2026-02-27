//! `QuarantineManager` — isolate failing sessions after 3 consecutive failures.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::session::SessionId;
use std::collections::HashMap;

/// Manages session quarantine for persistent failure isolation.
pub struct QuarantineManager {
    quarantined: HashMap<SessionId, String>,
}

impl QuarantineManager {
    pub fn new() -> Self {
        Self {
            quarantined: HashMap::new(),
        }
    }

    /// Quarantine a session ID with a reason.
    pub fn quarantine(&mut self, session_id: SessionId, reason: String) {
        eprintln!(
            "[WARN] Quarantining session {}: {}",
            session_id.as_str(),
            reason
        );
        self.quarantined.insert(session_id, reason);
    }

    /// Returns `true` if the given session ID is quarantined.
    pub fn is_quarantined(&self, session_id: &SessionId) -> bool {
        self.quarantined.contains_key(session_id)
    }

    /// Number of quarantined sessions.
    pub fn count(&self) -> usize {
        self.quarantined.len()
    }
}

impl Default for QuarantineManager {
    fn default() -> Self {
        Self::new()
    }
}
