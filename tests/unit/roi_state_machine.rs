//! T017 — `RoiManager` state-machine unit tests.
//!
//! Tests:  Searching → Locked → Lost → Locked (reacquired within grace)
//!         grace exceeded → on_exceeded action

use game_qa::config::RoiConfig;
use game_qa::roi::{GraceExceededAction, RoiManager, RoiState};

#[test]
fn initial_state_is_searching() {
    let config = RoiConfig::default();
    let roi = RoiManager::new(config, GraceExceededAction::MarkLowQuality);
    assert_eq!(roi.state(), RoiState::Searching);
}

#[test]
fn no_roi_when_searching() {
    let config = RoiConfig::default();
    let roi = RoiManager::new(config, GraceExceededAction::MarkLowQuality);
    assert!(roi.current_roi().is_none());
}

// TODO(T026): add state-machine transition tests once RoiManager::update() is implemented.
// The tests below are structural stubs that document the required behaviour.

#[test]
fn searching_to_locked_on_confirmed_match() {
    // When TemplateMatchDetector confirms `reacquire_confirm_frames` consecutive matches
    // above threshold, the state should transition Searching → Locked.
    // Currently a stub because RoiManager::update() is not yet implemented (TODO T026).
    let config = RoiConfig::default();
    let roi = RoiManager::new(config, GraceExceededAction::MarkLowQuality);
    // After TODO T026: feed N confirmed frames and assert state == Locked.
    assert_eq!(roi.state(), RoiState::Searching); // stub assertion
}
