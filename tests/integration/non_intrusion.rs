//! T022 — Non-intrusion audit integration test (SC-010).
//!
//! Confirms zero WRITE/DEBUG process handles to the game process during a live session.
//!
//! Requires: Chrome Dino running in the background.

use game_qa::audit::ProcessHandleAudit;

/// SC-010: non-intrusion audit during active session.
///
/// Run with: `cargo test --test non_intrusion -- --ignored`
/// NOTE: Chrome Dino must be running during this test.
#[test]
#[ignore = "requires Chrome Dino running in the background"]
fn non_intrusion_audit_passes() {
    // TODO(T022, T035): find Chrome Dino PID using process enumeration,
    // then run ProcessHandleAudit during an active capture session.

    // Stub: test with a known non-game PID (current process) to verify the audit runs.
    let our_pid = std::process::id();
    let audit = ProcessHandleAudit::new(our_pid);
    let result = audit.run();

    // TODO(T035): when GetProcessAccessFlags is implemented, this should assert:
    // assert!(result.passed, "SC-010 FAIL: violations = {:?}", result.violations);
    println!(
        "SC-010 audit result: passed={}, violations={:?}",
        result.passed, result.violations
    );
}
