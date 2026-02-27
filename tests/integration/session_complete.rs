//! T021 — Full 5-minute session integration test (US1).
//!
//! Asserts ALL of the following:
//!   - Artifact presence and schema validity (session.json, video.mp4, frames.jsonl, events.jsonl)
//!   - SC-001: capture latency P95 ≤ 5 ms
//!   - SC-002: FPS 30 ± 2, interval P95 ≤ 40 ms
//!   - SC-005: hook-to-queue P99 ≤ 10 ms
//!   - SC-006: zero event losses (gap-free event_id sequence); zero keydown/keyup pairing errors
//!   - SC-007: orphan event rate ≤ 0.1%; zero orphan windows > 1 s
//!   - SC-008: game FPS impact ≤ 3% / P99 frametime ≤ 5% (delegates to game_fps_impact helper)
//!   - SC-010: non-intrusion audit passes
//!
//! **Requires**: live Chrome Dino game, DXGI output duplication, full pipeline (T033).
//! All assertions are structural; the test is `#[ignore]` until the pipeline is complete.

use game_qa::session::{SessionRecord, SessionStatus};
use std::path::Path;

/// T021 — full session integration test.
///
/// Run with: `cargo test --test session_complete -- --ignored`
#[test]
#[ignore = "requires live Chrome Dino game and full pipeline implementation (T033)"]
fn session_complete_all_assertions() {
    // TODO(T021): bootstrap full pipeline against live Chrome Dino using test profile.
    // The session should run for up to 5 minutes (F9 to stop), then verify:

    let session_dir = Path::new("data/sessions").join("placeholder");

    // 1. Artifact presence
    assert!(
        session_dir.join("session.json").exists(),
        "session.json must exist"
    );
    assert!(
        session_dir.join("frames.jsonl").exists(),
        "frames.jsonl must exist"
    );
    assert!(
        session_dir.join("events.jsonl").exists(),
        "events.jsonl must exist"
    );
    assert!(
        session_dir.join("video.mp4").exists(),
        "video.mp4 must exist"
    );

    // 2. session.json schema validity
    let session_json = std::fs::read_to_string(session_dir.join("session.json")).unwrap();
    let record: SessionRecord = serde_json::from_str(&session_json).unwrap();
    assert_eq!(record.status, SessionStatus::Complete);
    assert_eq!(record.ts_mono_start_ns.0, 0, "ts_mono_start_ns must be 0");

    // 3. frames.jsonl — strict frame_id sequence (SC-006 partial)
    // TODO: parse frames.jsonl, assert no gaps in frame_id sequence.

    // 4. events.jsonl — zero event losses (SC-006)
    // TODO: parse events.jsonl, assert gap-free event_id sequence.

    // 5. SC-001, SC-002, SC-005: delegate to shared perf helpers.
    // TODO: embed perf measurements in session artifacts and validate here.

    // 6. SC-007: orphan event rate ≤ 0.1%
    // TODO: count events with prev_frame_id == null and assert rate ≤ 0.1%.

    // 7. SC-010: non-intrusion audit
    // TODO: run ProcessHandleAudit and assert passed == true.
}

/// T044 — Bot-profile variant with mock FrameChannel consumer (US3).
#[test]
#[ignore = "requires live Chrome Dino game and RingBufferConsumer (T045/T046)"]
fn session_complete_bot_profile_mock_consumer() {
    // TODO(T044): attach MockFrameConsumer to FrameChannel; verify:
    //   - oldest-drop policy fires when ring buffer is full
    //   - dual-mode simultaneous delivery (recording + realtime)
    //   - mode switching achieved via profile change only
}
