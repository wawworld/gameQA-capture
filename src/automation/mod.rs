//! `SessionScheduler` — batch loop: start → await → restart within 3 s.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod quarantine;

use crate::config::AutomationConfig;

/// Result of a single session run.
#[derive(Debug)]
pub enum SessionOutcome {
    Success,
    Failure(String),
}

/// Batch session scheduler.
///
/// Drives the automation loop: start session → await completion → restart within
/// `restart_delay_seconds`, tracking consecutive failures and delegating quarantine
/// to `QuarantineManager`.
pub struct SessionScheduler {
    config: AutomationConfig,
    consecutive_failures: u32,
    sessions_completed: u32,
}

impl SessionScheduler {
    pub fn new(config: AutomationConfig) -> Self {
        Self {
            config,
            consecutive_failures: 0,
            sessions_completed: 0,
        }
    }

    /// Record a session outcome and decide whether to continue, quarantine, or halt.
    pub fn record_outcome(&mut self, outcome: SessionOutcome) -> SchedulerDecision {
        match outcome {
            SessionOutcome::Success => {
                self.consecutive_failures = 0;
                self.sessions_completed += 1;
                if self.sessions_completed >= self.config.target_session_count {
                    SchedulerDecision::Done
                } else {
                    SchedulerDecision::RestartAfterDelay(self.config.restart_delay_seconds)
                }
            }
            SessionOutcome::Failure(reason) => {
                self.consecutive_failures += 1;
                if self.consecutive_failures >= self.config.max_consecutive_failures
                    && self.config.quarantine_on_failure
                {
                    SchedulerDecision::Quarantine(reason)
                } else {
                    SchedulerDecision::RestartAfterDelay(self.config.restart_delay_seconds)
                }
            }
        }
    }

    pub fn sessions_completed(&self) -> u32 {
        self.sessions_completed
    }
}

/// What the scheduler decided after recording an outcome.
#[derive(Debug)]
pub enum SchedulerDecision {
    /// Restart the session after `delay_seconds`.
    RestartAfterDelay(f64),
    /// Quarantine the current session and advance.
    Quarantine(String),
    /// All target sessions are done.
    Done,
}
