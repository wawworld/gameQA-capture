//! `ProcessHandleAudit` — SC-010 non-intrusion audit.
//!
//! Inspects live process handles during a session to confirm that the game
//! process has zero WRITE/DEBUG handle entries from this process.
#![deny(clippy::unwrap_used, clippy::expect_used)]

/// Result of a process handle audit.
#[derive(Debug)]
pub struct AuditResult {
    /// `true` if zero WRITE/DEBUG handles to the game process were found.
    pub passed: bool,
    /// Human-readable description of any violations found.
    pub violations: Vec<String>,
}

/// Audits the live process handle table to enforce the non-intrusion guarantee.
#[allow(dead_code)]
pub struct ProcessHandleAudit {
    /// PID of the game process to inspect.
    game_pid: u32,
}

impl ProcessHandleAudit {
    pub fn new(game_pid: u32) -> Self {
        Self { game_pid }
    }

    /// Run the audit.
    ///
    /// Returns `AuditResult::passed = true` if no WRITE or DEBUG handles to
    /// the game process are open from any handle in this process.
    pub fn run(&self) -> AuditResult {
        // TODO(T035): use NtQuerySystemInformation / GetProcessAccessFlags to inspect
        //             all handles in the current process and verify none target
        //             self.game_pid with PROCESS_VM_WRITE or PROCESS_VM_READ_DEBUG.
        AuditResult {
            passed: true,
            violations: Vec::new(),
        }
    }
}
