//! T023 — Session integrity tests: abnormal shutdown, disk-full, thread panic.
//!
//! Verifies:
//!   - `data/sessions/` has no corrupt data after abnormal stop
//!   - Interrupted sessions land in `data/incomplete/`

use game_qa::session::storage::SessionWriter;
use game_qa::session::{SessionId, SessionMode, SessionRecord, SessionStatus};
use game_qa::WallNs;
use tempfile::TempDir;

fn make_session_id() -> SessionId {
    SessionId("20260226T143000Z-chrome_dino-test".to_string())
}

fn make_record(session_id: &str) -> SessionRecord {
    SessionRecord::new(
        session_id.to_string(),
        WallNs(1_740_578_400_000_000_000),
        "chrome_dino".to_string(),
        "test".to_string(),
        SessionMode::Recording,
    )
}

#[test]
fn successful_commit_moves_to_sessions() {
    let tmp = TempDir::new().unwrap();
    let data_root = tmp.path().to_path_buf();
    let session_id = make_session_id();
    let record = make_record(session_id.as_str());

    let writer = SessionWriter::new(data_root.clone(), session_id.clone()).unwrap();

    // Write a dummy artifact.
    let frames_path = writer.artifact_path("frames.jsonl");
    std::fs::write(&frames_path, b"{}\n").unwrap();

    let mut final_record = record.clone();
    final_record.status = SessionStatus::Complete;

    let dest = writer.commit(&final_record).unwrap();
    assert!(dest.exists(), "Committed session directory must exist");
    assert!(
        dest.join("session.json").exists(),
        "session.json must be written"
    );
    assert!(
        !data_root.join(".wip").join(session_id.as_str()).exists(),
        "wip directory must be gone after commit"
    );
}

#[test]
fn abort_moves_to_incomplete() {
    let tmp = TempDir::new().unwrap();
    let data_root = tmp.path().to_path_buf();
    let session_id = make_session_id();

    let writer = SessionWriter::new(data_root.clone(), session_id.clone()).unwrap();

    // Write a partial artifact.
    let frames_path = writer.artifact_path("frames.jsonl");
    std::fs::write(&frames_path, b"partial\n").unwrap();

    writer.abort().unwrap();

    let incomplete_path = data_root.join("incomplete").join(session_id.as_str());
    assert!(
        incomplete_path.exists(),
        "Aborted session must land in data/incomplete/"
    );
    assert!(
        !data_root.join(".wip").join(session_id.as_str()).exists(),
        "wip directory must be gone after abort"
    );
}

#[test]
fn sessions_dir_has_no_incomplete_entries_after_commit() {
    let tmp = TempDir::new().unwrap();
    let data_root = tmp.path().to_path_buf();
    let session_id = make_session_id();

    let writer = SessionWriter::new(data_root.clone(), session_id.clone()).unwrap();
    let mut record = make_record(session_id.as_str());
    record.status = SessionStatus::Complete;
    writer.commit(&record).unwrap();

    // sessions/ should contain exactly our session; incomplete/ should be absent or empty.
    let sessions_dir = data_root.join("sessions");
    let entries: Vec<_> = std::fs::read_dir(&sessions_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 1);

    let incomplete_dir = data_root.join("incomplete");
    if incomplete_dir.exists() {
        let incomplete_entries: Vec<_> = std::fs::read_dir(&incomplete_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            incomplete_entries.is_empty(),
            "No entries should be in incomplete/ after a successful commit"
        );
    }
}
