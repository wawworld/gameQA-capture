# Implementation Plan: 비침투적 게임 데이터 수집 파이프라인

**Branch**: `001-capture-pipeline` | **Date**: 2026-02-26 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `/specs/001-capture-pipeline/spec.md`

## Summary

Build a non-intrusive game data collection pipeline in Rust that captures Chrome Dino gameplay
via Windows Desktop Duplication API and logs keyboard/mouse events via OS-level low-level hooks
— with zero game-process access at any point. The pipeline supports three operational modes
(test / ops / bot profile) governed by a three-tier YAML config hierarchy. US1 (single-session
pipeline) and US2 (600+ session automation) are in scope for this iteration. US3 (bot interface)
delivers the ring buffer interface and consumer contract; the live bot consumer is deferred.

---

## Technical Context

**Language/Version**: Rust 2024 edition (stable, ≥ 1.82)
**Primary Dependencies**:
- `windows` crate (windows-rs) — DXGI Desktop Duplication, WinAPI bindings
- `win_desktop_duplication` — high-level DXGI capture wrapper
- `willhook` — WH_KEYBOARD_LL / WH_MOUSE_LL hooks with dedicated message-loop threads
- `ffmpeg-next` — H.264/MP4 off-thread video encoding
- `opencv` — template matching for ROI detection (matchTemplate / TM_CCOEFF_NORMED)
- `thingbuf` — lock-free MPSC ring buffer with drop-oldest semantics
- `serde_yaml` — YAML parsing for config hierarchy
- `serde`, `serde_json` — JSONL serialization for frames and events
- `crossbeam-channel` — bounded MPSC channel for hook→queue pipeline

**Storage**: Local filesystem — `data/sessions/{session_id}/` (NTFS, two-phase commit)
**Testing**: `cargo test`, `cargo bench` (criterion), integration + perf test suites
**Target Platform**: Windows 10/11 (Desktop Duplication API requires WDDM driver)
**Project Type**: CLI service (long-running background process)
**Performance Goals**:
- Capture latency P95 ≤ 5 ms (DXGI acquire → memory arrival)
- Frame interval P95 ≤ 40 ms (30 ± 2 fps)
- Input hook-to-queue P99 ≤ 10 ms
- Real-time ring-buffer E2E P95 ≤ 8 ms

**Constraints**:
- Zero WRITE/DEBUG process handle access to game at any time (non-intrusion)
- All tunables in YAML profile files — no source hardcoding
- WH_KEYBOARD_LL / WH_MOUSE_LL hooks only (LL variant = no DLL injection into other processes)
- Video encoding on a dedicated thread to avoid blocking capture pipeline

**Scale/Scope**: 600+ unattended sessions (~5 min each), Chrome Dino, Windows only

---

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- [x] **I. Non-Intrusion**: Design uses no game-process intrusion technique.
  - DXGI Desktop Duplication reads the compositor output — zero game-process handle
  - WH_KEYBOARD_LL / WH_MOUSE_LL (LL-variant) run in the hook-installing process; they do NOT
    inject DLLs into other processes (unlike regular WH_KEYBOARD / WH_MOUSE hooks)
  - OpenCV template matching operates on already-captured frame data — no game access
  - Audit test (FR-044) inspects live process handles during session to confirm zero WRITE/DEBUG

- [x] **II. Latency Priority**: Latency assessed for every pipeline stage.
  - DXGI AcquireNextFrame: ~2–3 ms P95 (GPU-based, hardware path)
  - ROI detection (matchTemplate on captured frame): ~0.5–1 ms per frame
  - Ring buffer push (thingbuf lock-free): ~0.05 ms
  - Hook callback → channel push: < 0.1 ms; worker thread drains within P99 ≤ 10 ms budget
  - Video encoding is off-thread via `ffmpeg-next`; encoder latency does NOT block capture path
  - Explicit trade-off: encoding moved off capture thread (throughput favored over encoding
    latency); this is justified because encoded video is an archive artifact, not a real-time
    signal — capture latency budget is fully preserved

- [x] **III. Interface-First**: All replaceable components behind traits.
  - `CaptureBackend` trait — DxgiBackend is one impl; BitBlt or WGC can replace without
    touching any caller
  - `FrameConsumer` trait — DiskRecorderConsumer and RingBufferConsumer are both impls
  - `TerminationTrigger` trait — KeyPressTrigger, TimeoutTrigger, TemplateMatchTrigger,
    WindowLostTrigger are impls; future InferenceModelTrigger drops in with zero structural change
  - `InputHookBackend` trait — WillhookBackend is one impl; mock impl used in unit tests

- [x] **IV. Safe Code**: All three sub-principles satisfied.
  - No `unwrap()`/`expect()` in production paths; all error-propagating functions return
    `Result<_, E>` and bubble errors to the session boundary for a single log entry
  - Typed clock distinction: `MonotonicNs` (u64, relative to session start, from
    `std::time::Instant`) vs. `WallNs` (u64, UNIX epoch, from `std::time::SystemTime`);
    mixing them in the same field is a compile error
  - All tunables in YAML profiles (fps, buffer sizes, thresholds, grace periods)
  - No global mutable state; all shared state via `Arc<>` + `Mutex<>` or channels; hook
    callback state passed via `crossbeam_channel` sender captured in closure

- [x] **V. Test Gates**: Performance regression + session integrity tests planned for all SCs.
  - `tests/perf/capture_latency.rs` — SC-001: P95 capture latency
  - `tests/perf/frame_consistency.rs` — SC-002: FPS, frame interval P95, gap counts
  - `tests/perf/scheduling_drift.rs` — SC-003: schedule error median + P95 over 1 hour
  - `tests/perf/realtime_e2e.rs` — SC-004: capture-to-ring-buffer P95
  - `tests/perf/hook_latency.rs` — SC-005: hook-to-queue P99
  - `tests/integration/session_integrity.rs` — abnormal shutdown, disk-full, thread panic;
    verifies no corrupt data in `data/sessions/` and correct `data/incomplete/` isolation
  - `tests/integration/non_intrusion.rs` — SC-010: process handle audit during live session

- [x] **VI. Observability**: Runtime metrics designed in from the start.
  - `PipelineMetrics` struct (atomics) exposes: `capture_latency_p95_ns`, `queue_depth`,
    `orphan_event_count`, `session_completion_count`, `drop_count` (ring buffer drops)
  - Metrics readable without stopping the pipeline (read-only atomic access)
  - Debug preview window (`src/debug/mod.rs`) spawns independent thread; capture thread never
    calls any debug rendering code; debug feature flag = profile field, not compile feature
  - Debug mode ON/OFF verified by perf tests to have ≡ metric profile

- [x] **VII. Minimum Privilege**: OS permissions minimized and documented.
  - Required: Desktop Duplication access (IDXGIOutputDuplication — read-only compositor access)
  - Required: Global hook registration (SetWindowsHookEx — no elevated privileges needed for
    LL hooks on the same user session)
  - Required: File I/O on `data/` directory
  - NOT required: WRITE, DEBUG, or memory-map access to any game process handle
  - Audit test (SC-010) runs `GetProcessAccessFlags` on the game process handle during session

**Post-Phase 1 re-check**: All gates confirmed after data model and contracts are defined (see
Constitution Check section at end of Phase 1 design).

---

## Project Structure

### Documentation (this feature)

```text
specs/001-capture-pipeline/
├── plan.md           # This file
├── research.md       # Phase 0 — technology decisions
├── data-model.md     # Phase 1 — entity schemas
├── quickstart.md     # Phase 1 — developer setup and run guide
├── contracts/
│   ├── capture-backend.md    # CaptureBackend trait contract
│   ├── frame-consumer.md     # FrameConsumer trait contract
│   ├── trigger.md            # TerminationTrigger trait contract
│   └── session-schema.md     # JSONL + session.json schemas
└── tasks.md          # Phase 2 — generated by /speckit.tasks
```

### Source Code (repository root)

```text
src/
├── main.rs                         # CLI: arg parsing, profile load, pipeline bootstrap
├── config/
│   ├── mod.rs                      # CaptureConfig, ProfileConfig, load_merged_profile()
│   └── merge.rs                    # YAML recursive merge (dict merge, list replace, null remove)
├── capture/
│   ├── mod.rs                      # CaptureBackend trait, CapturedFrame, CaptureError
│   └── dxgi.rs                     # DxgiBackend: IDXGIOutputDuplication impl
├── roi/
│   ├── mod.rs                      # RoiManager, RoiState (Searching/Locked/Lost)
│   └── template.rs                 # TemplateMatchDetector (opencv matchTemplate)
├── hooks/
│   ├── mod.rs                      # InputHookBackend trait, InputEvent, HookError
│   └── win32.rs                    # WillhookBackend (WH_KEYBOARD_LL + WH_MOUSE_LL)
├── session/
│   ├── mod.rs                      # SessionManager, SessionState, SessionId
│   ├── lifecycle.rs                # Preroll, ROI stabilization, start/stop logic
│   ├── triggers.rs                 # TerminationTrigger trait + KeyPress/Timeout/Template/Window
│   └── storage.rs                  # SessionWriter: two-phase commit to data/sessions/
├── pipeline/
│   ├── mod.rs                      # Pipeline: orchestrates capture → ROI → consumer fan-out
│   ├── consumer.rs                 # FrameConsumer trait
│   ├── ring_buffer.rs              # RingBufferConsumer (thingbuf MPSC, drop-oldest)
│   ├── recorder.rs                 # DiskRecorderConsumer (video + JSONL)
│   └── metrics.rs                  # PipelineMetrics (atomics, observability)
├── automation/
│   ├── mod.rs                      # SessionScheduler: batch loop, restart logic
│   └── quarantine.rs               # QuarantineManager: isolate failing sessions
├── debug/
│   └── mod.rs                      # DebugPreview: independent thread, preview window
└── audit/
    └── mod.rs                      # ProcessHandleAudit: SC-010 compliance check

config/
├── base.yaml                       # All default values
├── games/
│   └── chrome_dino.yaml            # Game-specific overrides (ROI refs, triggers)
└── profiles/
    ├── test.override.yaml
    ├── ops.override.yaml
    └── bot.override.yaml

tests/
├── perf/
│   ├── capture_latency.rs          # SC-001: capture latency P95 regression
│   ├── frame_consistency.rs        # SC-002: FPS, interval P95, gap counts
│   ├── scheduling_drift.rs         # SC-003: schedule error P50 and P95
│   ├── realtime_e2e.rs             # SC-004: ring buffer E2E P95
│   └── hook_latency.rs             # SC-005: hook-to-queue P99
├── integration/
│   ├── session_complete.rs         # US1: full 5-min session, all artifact checks
│   ├── session_integrity.rs        # Abnormal shutdown / disk-full / thread-panic isolation
│   ├── batch_automation.rs         # US2: session restart, quarantine, disk protection
│   └── non_intrusion.rs            # SC-010: process handle audit
└── unit/
    ├── capture_backend_mock.rs     # CaptureBackend mock + trait contract tests
    ├── frame_consumer_mock.rs      # FrameConsumer mock + trait contract tests
    ├── trigger_mock.rs             # TerminationTrigger mock + tests
    ├── config_merge.rs             # YAML merge semantics (dict, list, null)
    └── roi_state_machine.rs        # RoiManager state transitions
```

**Structure Decision**: Single Rust binary (CLI service). All subsystems are internal crates
within `src/`; no separate workspace members needed for this scope. The `capture/mod.rs`,
`pipeline/consumer.rs`, `session/triggers.rs`, and `hooks/mod.rs` files define the primary
trait surfaces that Principle III requires. Test files mirror the trait names to enforce
interface-level testing (Principle V).

---

## Complexity Tracking

> No violations — all Constitution Check gates pass without exception.
