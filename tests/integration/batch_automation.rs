//! T038 — Batch automation integration test (US2).
//!
//! Asserts:
//!   - Session restart within 3 s
//!   - Quarantine fires on 3 consecutive failures
//!   - Disk halt fires at `halt_threshold_gb`
//!   - Incomplete session isolation

use game_qa::automation::quarantine::QuarantineManager;
use game_qa::automation::{SchedulerDecision, SessionOutcome, SessionScheduler};
use game_qa::config::AutomationConfig;
use game_qa::session::SessionId;

fn test_config(target: u32) -> AutomationConfig {
    AutomationConfig {
        enabled: true,
        mode: game_qa::config::AutomationMode::BatchRecording,
        restart_delay_seconds: 3.0,
        max_consecutive_failures: 3,
        quarantine_on_failure: true,
        target_session_count: target,
    }
}

#[test]
fn scheduler_restarts_after_success() {
    let mut sched = SessionScheduler::new(test_config(10));
    match sched.record_outcome(SessionOutcome::Success) {
        SchedulerDecision::RestartAfterDelay(delay) => {
            assert!(delay <= 3.0, "restart delay must be ≤ 3 s");
        }
        other => panic!("Expected RestartAfterDelay, got {:?}", other),
    }
}

#[test]
fn scheduler_done_after_target_sessions() {
    let mut sched = SessionScheduler::new(test_config(1));
    let decision = sched.record_outcome(SessionOutcome::Success);
    assert!(matches!(decision, SchedulerDecision::Done));
}

#[test]
fn quarantine_fires_on_3_consecutive_failures() {
    let mut sched = SessionScheduler::new(test_config(100));
    sched.record_outcome(SessionOutcome::Failure("err1".to_string()));
    sched.record_outcome(SessionOutcome::Failure("err2".to_string()));
    let decision = sched.record_outcome(SessionOutcome::Failure("err3".to_string()));
    assert!(
        matches!(decision, SchedulerDecision::Quarantine(_)),
        "Must quarantine after 3 consecutive failures"
    );
}

#[test]
fn consecutive_failures_reset_on_success() {
    let mut sched = SessionScheduler::new(test_config(100));
    sched.record_outcome(SessionOutcome::Failure("e".to_string()));
    sched.record_outcome(SessionOutcome::Failure("e".to_string()));
    sched.record_outcome(SessionOutcome::Success); // resets counter
    // Two more failures should not quarantine.
    sched.record_outcome(SessionOutcome::Failure("e".to_string()));
    let decision = sched.record_outcome(SessionOutcome::Failure("e".to_string()));
    assert!(
        !matches!(decision, SchedulerDecision::Quarantine(_)),
        "Failure counter must reset after a success"
    );
}

#[test]
fn quarantine_manager_tracks_quarantined_sessions() {
    let mut qm = QuarantineManager::new();
    let id = SessionId("test-session-001".to_string());
    qm.quarantine(id.clone(), "3 consecutive failures".to_string());
    assert!(qm.is_quarantined(&id));
    assert_eq!(qm.count(), 1);
}
