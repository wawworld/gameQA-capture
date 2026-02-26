# Tasks: 비침투적 게임 데이터 수집 파이프라인

**Feature**: `001-capture-pipeline`
**Input**: Design documents from `/specs/001-capture-pipeline/`
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓
**Generated**: 2026-02-26

---

## Format: `[ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: Which user story this task belongs to (US1 / US2 / US3)
- Every path is relative to the repository root

---

## Phase 1: Setup (Project Initialization)

**Purpose**: Rust project scaffold and all dependency declarations.

- [ ] T001 Initialize Rust 2024 edition project and declare all dependencies in `Cargo.toml` (windows, win_desktop_duplication, willhook, ffmpeg-next, opencv, thingbuf, serde_yaml, serde, serde_json, crossbeam-channel, thiserror, criterion)
- [ ] T002 [P] Create full `src/` directory tree per plan.md: `capture/`, `config/`, `roi/`, `hooks/`, `session/`, `pipeline/`, `automation/`, `debug/`, `audit/`
- [ ] T003 [P] Create `config/` YAML files: `config/base.yaml` (all tunables with defaults), `config/games/chrome_dino.yaml`, `config/profiles/test.override.yaml`, `config/profiles/ops.override.yaml`, `config/profiles/bot.override.yaml`
- [ ] T004 [P] Configure `rustfmt.toml` and `.clippy.toml`; add `deny(clippy::unwrap_used, clippy::expect_used)` for production paths

**Checkpoint**: `cargo check` passes on empty stubs; YAML config files validate

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types, traits, and infrastructure that EVERY user story depends on.

**⚠️ CRITICAL**: No user story work can begin until all Phase 2 tasks are complete.

- [ ] T005 Define **newtype** wrappers `struct MonotonicNs(pub u64)` and `struct WallNs(pub u64)` in `src/lib.rs` per `data-model.md §1`; derive Serialize/Deserialize; add `deny(clippy::unwrap_used)` to enforce no panic paths; verify that passing a `WallNs` where `MonotonicNs` is expected is a compile error (do NOT use `type` aliases — they collapse to the same type and allow silent mixing)
- [ ] T006 [P] Implement `CaptureBackend` trait + `CapturedFrame` struct + `CaptureError` enum + `Rect` struct in `src/capture/mod.rs` per `contracts/capture-backend.md`
- [ ] T007 [P] Implement `FrameConsumer` trait + `ConsumerError` enum in `src/pipeline/consumer.rs` per `contracts/frame-consumer.md`
- [ ] T008 [P] Implement `TerminationTrigger` trait + `TriggerState` enum in `src/session/triggers.rs` per `contracts/trigger.md`
- [ ] T009 [P] Implement `InputHookBackend` trait + `InputEvent` enum + `HookError` enum in `src/hooks/mod.rs`
- [ ] T010 [P] Implement all config structs (`CaptureConfig`, `RoiConfig`, `SessionConfig`, `TriggerConfig`, `EventCaptureConfig`, `ConsumerConfig`, `AutomationConfig`, `DiskProtectionConfig`, `DebugConfig`) in `src/config/mod.rs` per `data-model.md §6`
- [ ] T011 [P] Implement YAML three-tier merge logic (`load_merged_profile()`, dict-merge / list-replace / null-remove) in `src/config/merge.rs` per FR-037, FR-038, FR-039
- [ ] T012 [P] Implement `PipelineMetrics` struct (all `Atomic*` fields) + `load(Ordering::Relaxed)` accessors in `src/pipeline/metrics.rs` per `data-model.md §7`

**Checkpoint**: `cargo build` succeeds; all trait definitions compile; config merge logic is importable

---

## Phase 3: User Story 1 — Pipeline Validation Run (Priority: P1) 🎯 MVP

**Goal**: Full end-to-end single-session capture pipeline: DXGI → ROI → disk record → input events → session artifacts.

**Independent Test**: Run `cargo test --test session_complete` against a 5-minute test-profile session on Chrome Dino, stop with F9, and verify all four session artifacts exist with correct schemas, SC-001 through SC-008 pass, and SC-010 (non-intrusion audit) passes.

### Unit & Contract Tests for US1

- [ ] T013 [P] [US1] Implement `CaptureBackend` mock + full trait contract tests (acquire/release/timestamps/invariants) in `tests/unit/capture_backend_mock.rs`
- [ ] T014 [P] [US1] Implement `FrameConsumer` mock + consume/flush/error propagation contract tests in `tests/unit/frame_consumer_mock.rs`
- [ ] T015 [P] [US1] Implement `TerminationTrigger` mock + per-trigger contract tests (KeyPress / Timeout / TemplateMatch / WindowLost) in `tests/unit/trigger_mock.rs`
- [ ] T016 [P] [US1] Implement YAML merge unit tests (dict deep-merge, list full-replace, null-removes-key, three-tier composition) in `tests/unit/config_merge.rs`
- [ ] T017 [P] [US1] Implement `RoiManager` state-machine unit tests (Searching → Locked → Lost → Locked, grace exceeded → on_exceeded) in `tests/unit/roi_state_machine.rs`

### Performance Regression Tests for US1

- [ ] T018 [P] [US1] Implement capture latency P95 ≤ 5 ms regression test (SC-001) in `tests/perf/capture_latency.rs`
- [ ] T019 [P] [US1] Implement frame rate consistency test: avg 30 ± 2 fps, interval P95 ≤ 40 ms, gap counts (SC-002) in `tests/perf/frame_consistency.rs`
- [ ] T020 [P] [US1] Implement hook-to-queue latency P99 ≤ 10 ms regression test (SC-005) in `tests/perf/hook_latency.rs`
- [ ] T055 [P] [US1] Implement game FPS impact benchmark (SC-008): measure game average FPS and P99 frametime with capture ON vs. OFF; assert ≤ 3% FPS reduction and ≤ 5% P99 frametime increase in `tests/perf/game_fps_impact.rs`; note: requires the game process to be running and measurable via a frame-time query API (e.g., DXGI Present timing or external frame-time log) — document measurement method in the test

### Integration Tests for US1

- [ ] T021 [US1] Implement full 5-minute session integration test in `tests/integration/session_complete.rs`; must assert ALL of the following explicitly:
  - Artifact presence and schema validity (`session.json`, `video.mp4`, `frames.jsonl`, `events.jsonl`)
  - SC-001: capture latency P95 ≤ 5 ms; SC-002: FPS 30 ± 2, interval P95 ≤ 40 ms
  - SC-005: hook-to-queue P99 ≤ 10 ms
  - **SC-006**: zero event losses (verify event_id sequence is gap-free); zero keydown/keyup pairing errors (every `keyboard_press` has a matching `keyboard_release` within session)
  - **SC-007**: orphan event rate (`prev_frame_id == null`) ≤ 0.1% of total events; zero orphan windows exceeding 1 second
  - SC-008: game FPS impact ≤ 3% / P99 frametime ≤ 5% (delegate to `tests/perf/game_fps_impact.rs` via shared helper)
  - SC-010: non-intrusion audit passes
- [ ] T022 [US1] Implement non-intrusion audit integration test: confirms zero WRITE/DEBUG process handles during live session (SC-010) in `tests/integration/non_intrusion.rs`
- [ ] T023 [US1] Implement session integrity tests: abnormal shutdown, disk-full, thread panic — verify `data/sessions/` has no corrupt data, `data/incomplete/` captures interrupted sessions in `tests/integration/session_integrity.rs`

### Implementation for US1

- [ ] T024 [P] [US1] Implement `DxgiBackend` (IDXGIOutputDuplication acquire/release/monitor_rect, BGRA frames, MonotonicNs timestamps) in `src/capture/dxgi.rs`
- [ ] T025 [P] [US1] Implement `WillhookBackend` (WH_KEYBOARD_LL + WH_MOUSE_LL, dedicated message-loop threads, crossbeam-channel delivery) in `src/hooks/win32.rs`; include `MouseMoveFilter` (FR-022: max 20 Hz rate-limit, min delta 5 px threshold, both configurable from `EventCaptureConfig.mouse_move`) — filter applied inside the hook backend before events are pushed to the channel
- [ ] T026 [US1] Implement `RoiManager` + `RoiState` enum (Searching/Locked/Lost) + `TemplateMatchDetector` (opencv matchTemplate, TM_CCOEFF_NORMED, confirm_frames logic) in `src/roi/mod.rs` and `src/roi/template.rs`
- [ ] T027 [US1] Implement `SessionRecord`, `SessionStatus` enum, `SessionMode` enum in `src/session/mod.rs` per `data-model.md §2` and `contracts/session-schema.md`
- [ ] T028 [US1] Implement `SessionWriter` two-phase commit (write to `data/.wip/{id}/` → rename to `data/sessions/{id}/`; on failure move to `data/incomplete/{id}/`) in `src/session/storage.rs`
- [ ] T029 [US1] Implement session lifecycle state machine in `src/session/lifecycle.rs`: preroll discard, ROI stabilization (wait `roi_stable_seconds`), active capture start, graceful stop; include **FR-015** window-foreground detection — when game window loses foreground beyond `background_grace_seconds` pause capture; when window regains foreground resume capture (governed by `SessionConfig.require_window_foreground`, `pause_when_background`, `background_grace_seconds`)
- [ ] T030 [P] [US1] Implement `KeyPressTrigger` + `TimeoutTrigger` in `src/session/triggers.rs` per `contracts/trigger.md`
- [ ] T031 [US1] Implement `TemplateMatchTrigger` (opencv, consecutive-frame confirmation counter) + `WindowLostTrigger` (Win32 IsWindow) in `src/session/triggers.rs` (depends on T030 — same file, cannot run in parallel)
- [ ] T032 [US1] Implement `DiskRecorderConsumer`: off-thread bounded encoder queue, ffmpeg-next H.264/MP4 writer, `frames.jsonl` line writer, `events.jsonl` writer, `flush()` drain in `src/pipeline/recorder.rs`
- [ ] T033 [US1] Implement `Pipeline` orchestrator: capture loop → ROI evaluation → consumer fan-out → metrics update → trigger polling in `src/pipeline/mod.rs`
- [ ] T034 [P] [US1] Implement `DebugPreview` (independent thread, Win32 preview window, profile-flag gated) in `src/debug/mod.rs`
- [ ] T035 [P] [US1] Implement `ProcessHandleAudit` (GetProcessAccessFlags inspection during live session) in `src/audit/mod.rs`
- [ ] T036 [US1] Implement `main.rs`: CLI arg parsing (profile selection, game selection), YAML merge load, pipeline bootstrap, signal handler for F9

**Checkpoint**: `cargo test --test session_complete` passes; single test-profile session on Chrome Dino produces all four artifacts; SC-001, SC-002, SC-005, SC-010 pass

---

## Phase 4: User Story 2 — Automated Batch Collection (Priority: P2)

**Goal**: Unattended 600+ session automation with quarantine, disk protection, and ≥ 99% completion rate.

**Independent Test**: Run `cargo test --test batch_automation` — configure ops profile with `target_session_count = 10` (smoke test), verify ≥ 99% completion, `data/sessions/` contains no incomplete entries, quarantine fires on 3 consecutive same failures, disk halt fires correctly.

### Performance Regression Tests for US2

- [ ] T037 [P] [US2] Implement scheduling drift test: median ≤ 5 ms, P95 ≤ 20 ms over 1-hour window (SC-003) in `tests/perf/scheduling_drift.rs`

### Integration Tests for US2

- [ ] T038 [US2] Implement batch automation integration test: session restart within 3 s, quarantine on 3 consecutive failures, disk-halt at 10 GB, incomplete session isolation in `tests/integration/batch_automation.rs`

### Implementation for US2

- [ ] T039 [US2] Implement `SessionScheduler` (batch loop: start session → await completion → restart within 3 s, track consecutive failures) in `src/automation/mod.rs`
- [ ] T040 [US2] Implement `QuarantineManager` (isolate session ID after 3 consecutive failures, log quarantine reason, auto-advance to next) in `src/automation/quarantine.rs`
- [ ] T041 [US2] Extend `SessionWriter` / `DiskProtectionConfig` watcher: 60-second interval check, halt at `halt_threshold_gb`, log at `warning_threshold_gb`, safe session close in `src/session/storage.rs` (depends on T028 — extends the same file)
- [ ] T042 [US2] Wire `AutomationConfig` into `main.rs` bootstrap: enable batch loop when `automation.enabled = true`, pass `SessionScheduler` + `QuarantineManager` to pipeline

**Checkpoint**: `cargo test --test batch_automation` passes; automation loop handles failure and disk-full scenarios correctly

---

## Phase 5: User Story 3 — Bot Interface Readiness (Priority: P3 / Deferred)

**Goal**: Ring buffer consumer interface and `FrameChannel` reader contract built and verified via mock consumer. No live bot implemented.

**Independent Test**: Run `cargo test --test session_complete` with bot profile + mock consumer attached to `FrameChannel`; verify E2E frame delivery P95 ≤ 8 ms (SC-004), oldest-drop policy fires, mode switching (recording-only / realtime-only / both) requires zero code changes.

### Performance Regression Tests for US3

- [ ] T043 [P] [US3] Implement ring buffer E2E capture-to-consumer P95 ≤ 8 ms test with mock consumer (SC-004) in `tests/perf/realtime_e2e.rs`

### Integration Tests for US3

- [ ] T044 [US3] Extend `tests/integration/session_complete.rs` with bot-profile variant: mock `FrameChannel` consumer, verify oldest-drop policy, dual-mode simultaneous delivery, mode switching via profile only (depends on T021 — extends the same file)

### Implementation for US3

- [ ] T045 [US3] Implement `RingBufferConsumer` (thingbuf MPSC, drop-oldest on full, increment `ring_buffer_drop_count`) in `src/pipeline/ring_buffer.rs`
- [ ] T046 [US3] Implement `FrameChannel` reader (`try_recv()` + `recv_timeout()`) and `RingBufferConsumer::reader()` factory in `src/pipeline/ring_buffer.rs` per `contracts/frame-consumer.md`
- [ ] T047 [US3] Wire `ConsumerConfig` (recording_enabled / realtime_enabled / ring_buffer_capacity) into `Pipeline` fan-out: activate `DiskRecorderConsumer`, `RingBufferConsumer`, or both based on profile in `src/pipeline/mod.rs`

**Checkpoint**: `cargo test --test session_complete` bot-profile variant passes, SC-004 perf test passes; mode switching verified profile-only

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Observability validation, lint clean-up, quickstart verification, final threshold sweep.

- [ ] T048 [P] Run `cargo clippy --all-targets --all-features -- -D warnings`; fix all warnings (no `unwrap`/`expect` in production paths)
- [ ] T049 [P] Run `cargo fmt --check`; apply `cargo fmt` where needed
- [ ] T050 [P] Run full perf regression suite (`cargo bench`): confirm SC-001 through SC-008 and SC-011 (debug mode parity) all pass
- [ ] T051 Validate all session artifacts from integration tests conform to `contracts/session-schema.md` (schema field presence, ordering, null rules)
- [ ] T052 Verify `DebugPreview` ON vs. OFF metric parity: run `tests/perf/capture_latency.rs` with `debug.enabled = true`; confirm all thresholds hold (SC-011)
- [ ] T053 [P] Follow `specs/001-capture-pipeline/quickstart.md` end-to-end on a clean checkout; confirm all described commands succeed
- [ ] T054 Update `CLAUDE.md` with any new commands, new libraries, or structural notes added during implementation

**Checkpoint**: `cargo test`, `cargo clippy`, `cargo bench` all pass; quickstart succeeds on clean checkout

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies — start immediately
- **Phase 2 (Foundational)**: Depends on Phase 1 — **BLOCKS all user stories**
- **Phase 3 (US1)**: Depends on Phase 2 — P1 priority, unblocks Phase 4 and 5
- **Phase 4 (US2)**: Implementation tasks (T039–T041) depend on Phase 2 only; the integration test (`batch_automation`) requires Phase 3 (US1 pipeline) to be functionally complete before it can be run end-to-end
- **Phase 5 (US3)**: Depends on Phase 2 (interface work); can start in parallel with Phase 4 once Phase 2 is done
- **Phase 6 (Polish)**: Depends on all desired stories complete

### User Story Dependencies

- **US1 (P1)**: Starts after Phase 2 — no dependency on US2/US3
- **US2 (P2)**: Starts after Phase 3 (requires verified US1 pipeline — `SessionScheduler` in T039 wraps `Pipeline` from T033); test US2 independently via batch loop only, no additional US3 coupling
- **US3 (P3)**: Starts after Phase 2; adds `RingBufferConsumer` layer on top of existing `Pipeline` — independently testable via mock consumer

### Within Each User Story

- Tests → Traits/Models → Services → Orchestration → CLI wiring
- Parallel tasks (marked [P]) can be assigned to separate developers simultaneously
- Commit after each logical task group

---

## Parallel Execution Examples

### Phase 2 — Foundational (all [P], assign to 4 developers)

```
Dev A: T006 CaptureBackend trait + T007 FrameConsumer trait
Dev B: T008 TerminationTrigger trait + T009 InputHookBackend trait
Dev C: T010 all config structs + T011 YAML merge
Dev D: T005 newtypes (MonotonicNs/WallNs) + T012 PipelineMetrics
```

### Phase 3 — US1 Tests (all [P], run before implementation)

```
Dev A: T013 CaptureBackend mock + T014 FrameConsumer mock
Dev B: T015 Trigger mocks + T017 RoiState machine
Dev C: T016 Config merge tests + T018–T020 perf regression stubs
```

### Phase 3 — US1 Implementation (all [P] where marked)

```
Dev A: T024 DxgiBackend + T026 RoiManager
Dev B: T025 WillhookBackend + T030 Triggers (KeyPress/Timeout) → T031 Triggers (Template/Window, sequential after T030)
Dev C: T027 SessionRecord types + T028 SessionWriter
Dev D: T034 DebugPreview + T035 ProcessHandleAudit
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational — **CRITICAL gate**
3. Complete Phase 3: US1 (tests first, then implementation)
4. **STOP and VALIDATE**: `cargo test`, single 5-min Chrome Dino session end-to-end
5. Deploy / demo MVP

### Incremental Delivery

1. Phase 1 + 2 → Foundation ready
2. Phase 3 → US1 complete → MVP ✅ (single-session pipeline verified)
3. Phase 4 → US2 complete → Batch automation ✅ (600-session overnight run)
4. Phase 5 → US3 complete → Bot interface ready ✅ (future bot can integrate without code changes)
5. Phase 6 → Polish → Release candidate ✅

---

## Summary

| Phase | Tasks | Parallel Opportunities | Blocks |
|-------|-------|----------------------|--------|
| 1 Setup | T001–T004 | T002, T003, T004 | Phase 2 |
| 2 Foundational | T005–T012 | T006–T012 | All stories |
| 3 US1 (P1) | T013–T036, T055 | T013–T020, T024–T025, T030, T034–T035, T055 | US2 integration |
| 4 US2 (P2) | T037–T042 | T037 | — |
| 5 US3 (P3) | T043–T047 | T043 | — |
| 6 Polish | T048–T054 | T048, T049, T050, T052, T053 | Release |
| **Total** | **55 tasks** | **~30 parallelizable** | |

> **F3 fix**: T031 is NOT parallelizable — it writes to the same file (`src/session/triggers.rs`)
> as T030. T031 depends on T030 and must run sequentially after it.
>
> **F2 fix**: T055 (game_fps_impact) covers SC-008, which had no corresponding perf test.

**Suggested MVP scope**: Phase 1 + Phase 2 + Phase 3 (US1 only) = T001–T036, T055
