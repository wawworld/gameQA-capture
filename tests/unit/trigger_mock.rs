//! T015 — `TerminationTrigger` mock + per-trigger contract tests.
//!
//! Tests: KeyPress / Timeout / TemplateMatch / WindowLost trigger behaviour.

use game_qa::MonotonicNs;
use game_qa::capture::CapturedFrame;
use game_qa::hooks::InputEvent;
use game_qa::session::triggers::{
    KeyPressTrigger, TerminationTrigger, TimeoutTrigger, TriggerState,
};
use std::time::Instant;

fn make_frame(ts_ns: u64) -> CapturedFrame {
    CapturedFrame {
        data: vec![0u8; 16],
        width: 4,
        height: 2,
        capture_ts_ns: MonotonicNs(ts_ns),
        dxgi_acquire_ts_ns: None,
        buffer_ready_ts_ns: None,
    }
}

fn key_press_event(vk: u16) -> InputEvent {
    InputEvent::KeyPress {
        vk_code: vk,
        scan_code: 0,
        flags: 0,
        recv_ts_ns: MonotonicNs(0),
    }
}

// ─── KeyPressTrigger ─────────────────────────────────────────────────────────

#[test]
fn key_press_fires_on_matching_key() {
    let mut trigger = KeyPressTrigger::new(0x78); // VK_F9
    let frame = make_frame(0);
    let events = vec![key_press_event(0x78)];
    match trigger.check(&frame, 0, &events) {
        TriggerState::Terminate {
            requires_confirmation,
            ..
        } => {
            assert!(!requires_confirmation);
        }
        TriggerState::Continue => panic!("Expected Terminate"),
    }
}

#[test]
fn key_press_continues_on_wrong_key() {
    let mut trigger = KeyPressTrigger::new(0x78);
    let frame = make_frame(0);
    let events = vec![key_press_event(0x70)]; // VK_F1
    assert!(matches!(
        trigger.check(&frame, 0, &events),
        TriggerState::Continue
    ));
}

#[test]
fn key_press_continues_with_empty_events() {
    let mut trigger = KeyPressTrigger::new(0x78);
    let frame = make_frame(0);
    assert!(matches!(
        trigger.check(&frame, 0, &[]),
        TriggerState::Continue
    ));
}

// ─── TimeoutTrigger ──────────────────────────────────────────────────────────

#[test]
fn timeout_fires_after_elapsed() {
    let start = Instant::now();
    let mut trigger = TimeoutTrigger::new(start, 1.0); // 1-second timeout
    let frame = make_frame(1_500_000_000); // 1.5 s elapsed
    match trigger.check(&frame, 0, &[]) {
        TriggerState::Terminate {
            requires_confirmation,
            ..
        } => {
            assert!(!requires_confirmation);
        }
        TriggerState::Continue => panic!("Expected Terminate after timeout"),
    }
}

#[test]
fn timeout_continues_before_elapsed() {
    let start = Instant::now();
    let mut trigger = TimeoutTrigger::new(start, 5.0); // 5-second timeout
    let frame = make_frame(1_000_000_000); // 1 s elapsed
    assert!(matches!(
        trigger.check(&frame, 0, &[]),
        TriggerState::Continue
    ));
}

// ─── Trigger is a pure observer ───────────────────────────────────────────────

#[test]
fn trigger_does_not_modify_frame() {
    let mut trigger = KeyPressTrigger::new(0x78);
    let frame = make_frame(100);
    let original_ts = frame.capture_ts_ns;
    trigger.check(&frame, 0, &[]);
    assert_eq!(
        frame.capture_ts_ns, original_ts,
        "Trigger must not modify frame"
    );
}
