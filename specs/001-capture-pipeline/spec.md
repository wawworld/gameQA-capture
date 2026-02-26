# Feature Specification: 비침투적 게임 데이터 수집 파이프라인

**Feature Branch**: `001-capture-pipeline`
**Created**: 2026-02-26
**Status**: Draft
**Input**: User description: "비침투적 게임 데이터 수집 파이프라인"

> **Purpose**: Implement a system that collects game data exclusively via OS-level screen capture
> and input logging — without any access to the game process — and systematically demonstrate
> non-intrusive game analysis on Chrome Dino.
>
> **Connected Idea**: IDEA-20260222-비침투적-텔레메트리-게임분석

---

## Implementation Phases

This feature is delivered in three sequential phases, each independently deployable:

| Phase | User Story | Scope | When |
|-------|-----------|-------|------|
| **1 (Now)** | US1 – Pipeline Validation | Full single-session capture pipeline | This iteration |
| **2 (Now)** | US2 – Batch Automation | Unattended 600+ session collection | This iteration |
| **3 (Later)** | US3 – Bot Interface Readiness | Ring buffer **interface and contracts** only; actual bot not integrated | Future iteration |

> **US3 scope note**: The bot integration use case is **deferred**. However, the ring buffer
> interface, abstract consumer contract, and dual-mode switching capability **MUST be built now**
> so that the future bot integration requires zero structural changes. US3 acceptance scenarios
> therefore validate the interface contracts and structural correctness — not a live bot consumer.

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Pipeline Validation Run (Priority: P1)

A QA engineer opens Chrome Dino and starts a single data collection session in **test profile**
to verify the entire pipeline works end-to-end. The engineer interacts with the game normally.
After stopping the session (F9), they inspect the output and confirm that screen frames, input
events, and timestamps were all captured correctly; no access to the game process occurred; and
every performance threshold was met.

**Why this priority**: Without a verified single-session pipeline, automation and bot integration
cannot proceed. This is the MVP that unlocks all other user stories.

**Independent Test**: Run a 5-minute test-profile session on Chrome Dino, stop with F9, and
verify: all four session artifacts exist with correct schemas, every threshold in the performance
acceptance table is satisfied, and the non-intrusion audit reports zero WRITE/DEBUG process handle
accesses.

**Acceptance Scenarios**:

1. **Given** a Chrome Dino window is open and in the foreground, **When** the engineer starts a
   test-profile session, **Then** the system detects the game ROI within 30 seconds, waits 1
   second for ROI stability, silently discards 2 seconds of preroll frames, and enters active
   capture with latency P95 ≤ 5 ms.

2. **Given** the session is actively capturing, **When** the engineer presses a key during
   gameplay, **Then** the keydown and keyup events are each logged with monotonic timestamps; the
   hook-to-queue latency is ≤ 10 ms at P99; and the event record contains the ID and timestamp
   of the immediately preceding captured frame.

3. **Given** a 5-minute session has completed, **When** the engineer reviews the session
   directory, **Then** `session.json`, `video.mp4`, `frames.jsonl`, and `events.jsonl` all exist
   and conform to their schemas; all SC-001 through SC-008 thresholds are satisfied; and the
   non-intrusion audit (SC-010) passes.

4. **Given** the game ROI is lost mid-session, **When** the grace period elapses without
   re-acquisition, **Then** the session continues but is flagged `low_quality` in `session.json`;
   no corrupt data is written to disk.

5. **Given** debug mode is enabled, **When** any performance measurement is taken, **Then** all
   thresholds remain identical to the debug-mode-off baseline (SC-011).

---

### User Story 2 - Automated Batch Collection (Priority: P2)

A QA operator configures **ops profile** and leaves the system running overnight to collect 600+
game sessions without any human intervention. Each session starts automatically, runs until the
game-over screen is detected, and triggers the next session. The operator checks results the next
morning and expects a ≥ 99% completion rate and zero corrupt sessions.

**Why this priority**: Generating training data at scale is the primary project objective. It
depends on a verified single-session pipeline (US1) but can be tested and delivered independently
once that foundation is solid.

**Independent Test**: Configure ops profile with batch automation enabled targeting 600 sessions;
leave running overnight. Verify: ≥ 594 of 600 attempts completed, `data/sessions/` contains no
incomplete entries, `data/incomplete/` captures all interrupted sessions, and disk protection
triggered before free space dropped below 10 GB (if applicable).

**Acceptance Scenarios**:

1. **Given** the ops profile is active and a session is in progress, **When** the game-over
   screen appears, **Then** the pipeline detects it via template matching with ≥ 3 consecutive
   confirming frames above 0.80 threshold, terminates the session cleanly, and starts the next
   session within 3 seconds.

2. **Given** the same session failure occurs 3 consecutive times, **When** the automation system
   processes the failure, **Then** that session is quarantined and the next session begins without
   operator intervention.

3. **Given** disk free space drops below 10 GB (detected at the 60-second check interval),
   **When** the check fires, **Then** data collection halts immediately; no new sessions start;
   the in-progress session is safely closed and moved to `data/incomplete/`.

4. **Given** a session is interrupted by a crash or thread panic, **When** the system recovers,
   **Then** partial session data is atomically moved to `data/incomplete/`; `data/sessions/`
   contains zero corrupt or partial entries.

5. **Given** the game window moves to the background, **When** 2 seconds elapse (ops grace
   period), **Then** capture pauses; when the window returns to the foreground, capture resumes
   and the session continues without restarting.

---

### User Story 3 - Bot Interface Readiness (Priority: P3 / Deferred)

> **Scope**: This user story covers building the **structural interface and contracts** that a
> future bot consumer will use. A live inference bot is **not** integrated in this iteration.
> Acceptance scenarios are validated using a mock/stub consumer.

The pipeline architect verifies that the capture system exposes a well-defined, abstract ring
buffer interface. A future bot integration MUST be achievable by implementing the consumer
interface and switching to bot profile — no changes to the capture pipeline itself.

**Why this priority**: The bot use case drives several architectural decisions (ring buffer,
dual-mode, abstract consumer contract). Building the interface now prevents future structural
refactoring. The full integration is deferred but the design window is now.

**Independent Test**: Activate bot profile with a **mock consumer** (stub that reads from the
ring buffer interface). Verify: end-to-end frame delivery P95 ≤ 8 ms to the stub consumer,
oldest-frame-drop policy fires correctly when consumer lags, and switching between recording-only
/ real-time-only / both modes requires zero code changes (profile change only). No real inference
logic is required.

**Acceptance Scenarios**:

1. **Given** the bot profile is active and a mock consumer is attached to the ring buffer
   interface, **When** a frame is captured, **Then** the frame is available in the 2-frame ring
   buffer within 8 ms at P95; when the buffer is full, the oldest frame is dropped so the newest
   frame is always available to any consumer.

2. **Given** both recording mode and real-time mode are simultaneously active, **When** a frame
   is captured, **Then** it is delivered to both the disk writer and the ring buffer within their
   respective latency bounds; neither channel blocks the other.

3. **Given** the ops profile (recording-only), the test profile (recording-only), and the bot
   profile (real-time, recording optional) are each activated in turn, **When** the pipeline
   starts, **Then** mode switching requires only a profile change — no code modification — and
   all previously passing acceptance criteria for US1 still hold.

4. **Given** the bot session ends (F9 or timeout), **When** the session artifact set is evaluated,
   **Then** `session.json` includes `ts_perf_end_ns`; when recording is disabled, the absence of
   `video.mp4` and `frames.jsonl` is not treated as an incomplete session.

---

### Edge Cases

- What happens when the ROI template does not match within the 30-second detection timeout?
  → Session fails with status `fail_session`; no partial data is written to `data/sessions/`.
- How does the system handle a disk-full condition mid-session?
  → The current session is moved to `data/incomplete/`; collection halts until the operator
  resolves the disk condition.
- What if a keydown event has no matching keyup (e.g., process killed mid-press)?
  → The event is still recorded; session integrity validation flags the pairing error in
  post-processing but does not corrupt the session.
- What happens when the ROI shifts more than 30 px in a single frame?
  → Re-acquisition is triggered; capture pauses during re-acquisition; re-acquisition lock
  requires 2 consecutive confirming frames.
- How does the system behave when debug mode is enabled during a performance-critical session?
  → Debug rendering runs on an independent thread; all performance thresholds apply equally with
  debug mode on or off.

---

## Requirements *(mandatory)*

### Functional Requirements

**Capture Engine**

- **FR-001**: The system MUST capture the game display using OS-level screen capture APIs that
  require no access to the game process's memory, code, or address space.
- **FR-002**: The system MUST crop captured frames to the game window's client area.
- **FR-003**: The system MUST expose capture functionality through an abstract backend interface;
  replacing the capture backend MUST require zero changes to any upstream component (recording,
  detection, bot consumer).
- **FR-004**: The target capture rate MUST be configurable per profile (default: 30 fps).
- **FR-005**: Each captured frame MUST carry a monotonic clock timestamp.
- **FR-006**: When capture diagnostics are enabled, each frame record MUST include display-
  acquisition and memory-arrival timestamps; when disabled, these fields MUST be `null` (schema
  remains compatible in both modes).

**ROI Management**

- **FR-007**: Capture MUST NOT begin until the game ROI is detected via reference-image template
  matching (default threshold: 0.75; minimum size: 400 × 150 px).
- **FR-008**: After first successful ROI detection, the ROI MUST be locked; re-acquisition is
  triggered only when the per-frame ROI shift exceeds the configured threshold (default: 30 px).
- **FR-009**: ROI coordinates MUST be clamped to the monitor boundary at all times.
- **FR-010**: Re-acquisition MUST be confirmed by a configurable number of consecutive matching
  frames (default: 2) before the ROI lock is restored.
- **FR-011**: If the ROI cannot be re-acquired within the grace period, the system MUST execute
  the profile-defined `on_exceeded` action (`mark_low_quality` for ops; `stop_bot` for bot).

**Session Lifecycle**

- **FR-012**: Capture MUST NOT start until the ROI has been continuously stable for the configured
  stabilization period (default: 1 second).
- **FR-013**: If ROI detection times out (default: 30 seconds), the session MUST fail with status
  `fail_session`.
- **FR-014**: Frames captured during the preroll period (default: 2 seconds) MUST be discarded;
  the session timestamp origin is set to the first kept frame.
- **FR-015**: When `require_window_foreground` is enabled, capture MUST proceed only while the
  game window is in the foreground and MUST pause when it moves to the background beyond the
  configured grace period.
- **FR-016**: The system MUST support these session termination triggers: manual key press
  (default: F9), elapsed timeout (default: 300 seconds), game-state template match, and game
  window closure.
- **FR-017**: Template-match termination triggers MUST require a configurable number of
  consecutive confirming frames (default: 3) before the session terminates.
- **FR-018**: The termination trigger mechanism MUST be designed so that an inference-model-based
  trigger can replace the template-match trigger via profile configuration only — no code change.

**Input Event Logging**

- **FR-019**: The system MUST intercept keyboard and mouse events using OS-level global hooks that
  require no access to the game process.
- **FR-020**: Logged event types MUST be configurable per profile (default: keyboard press and
  release only; ops/bot profiles may additionally enable mouse events).
- **FR-021**: Each event record MUST include: event ID, monotonic hook-receipt timestamp, queue-
  insertion completion timestamp, event type, payload, preceding frame ID, preceding frame
  timestamp, and elapsed time since the preceding frame.
- **FR-022**: Mouse move events, when enabled, MUST be rate-limited and filtered by minimum
  movement delta (default: max 20 Hz, minimum delta 5 px).

**Timestamp Synchronization**

- **FR-023**: All frame and event timestamps MUST originate from the same monotonic clock source.
- **FR-024**: Wall-clock timestamps MUST be used only for session metadata and external log
  joining; they MUST NOT be mixed with monotonic timestamps in frame or event records.
- **FR-025**: `session.json` MUST record both a wall-clock epoch value and a monotonic value at
  session start; recording the same pair at session end is configurable per profile (ops and bot
  profiles: required; test profile: not required).
- **FR-026**: Source timestamps MUST be preserved without runtime remapping so that post-session
  analysis can re-map frame-event associations at any time offset.

**Dual Consumption Modes**

- **FR-027**: The system MUST support recording mode (disk serialization), real-time mode (ring
  buffer supply to an inference consumer), and both modes simultaneously.
- **FR-028**: Ring buffer capacity MUST be configurable per profile (ops default: 3 frames; bot
  default: 2 frames).
- **FR-029**: When the ring buffer is full, the oldest frame MUST be dropped to ensure the
  consumer always receives the most recent frame.
- **FR-030**: Switching among recording-only, real-time-only, and both modes MUST require only a
  profile configuration change — no code modification.

**Session Storage Integrity**

- **FR-031**: A session is "complete" only when all four artifacts are present: `session.json`,
  `video.mp4`, `frames.jsonl`, `events.jsonl`. (Exception: `video.mp4` and `frames.jsonl` may
  be absent when recording mode is disabled in bot profile.)
- **FR-032**: If a session is interrupted before completion, its partial data MUST be atomically
  moved to `data/incomplete/` and MUST NOT appear in `data/sessions/`.
- **FR-033**: The system MUST check available disk space every 60 seconds; when free space falls
  below 10 GB, data collection MUST halt immediately.

**Session Automation**

- **FR-034**: The system MUST support unattended batch operation, automatically restarting
  sessions after each completion to accumulate 600+ sessions.
- **FR-035**: Restart after a normal session end MUST complete within 3 seconds (ops profile).
- **FR-036**: If the same session failure occurs 3 consecutive times, that session MUST be
  quarantined and the next session MUST start automatically.

**Configuration Hierarchy**

- **FR-037**: Configuration MUST follow a three-tier hierarchy:
  `base.yaml` → `games/{game}.yaml` → `profiles/{profile}.override.yaml`.
- **FR-038**: Profile merging rules: dictionaries merge key-by-key recursively; lists are fully
  replaced (not appended); a `null` value in an override removes the corresponding base key.
- **FR-039**: All tunable values MUST be declared in profile files; no default values may be
  hardcoded in source code.

**Debug Mode**

- **FR-040**: When debug mode is enabled, the system MUST display the current capture region in a
  real-time preview window.
- **FR-041**: Debug preview rendering MUST run on an independent thread, sharing no execution
  time with the capture pipeline thread.
- **FR-042**: Debug mode MUST NOT cause any performance metric to fall below the thresholds
  defined in the performance acceptance table (SC-001 through SC-008).

**Non-Intrusion Compliance**

- **FR-043**: The system MUST NOT hold WRITE or DEBUG access rights on the game process handle at
  any point during execution.
- **FR-044**: Non-intrusion compliance MUST be verifiable by an automated audit test that inspects
  live process handle permissions during a running session.

### Key Entities

- **Session**: A bounded data collection interval identified by `session_id`; characterized by
  start/end timestamps, active profile name, consumption mode, gap flag, and completion status
  (`complete` | `incomplete` | `low_quality`).
- **Frame**: A single captured display snapshot identified by `frame_id`; carries a monotonic
  capture timestamp and optional diagnostic timestamps (null when diagnostics are disabled).
- **Event**: A single input occurrence (keyboard or mouse) identified by `event_id`; carries a
  monotonic hook-receipt timestamp and is linked to its immediately preceding frame.
- **ROI (Region of Interest)**: The bounding rectangle of game content within the display;
  detected by reference-image template matching and locked after first successful detection.
- **Profile**: A merged configuration layer (`base.yaml` → game override → mode override) that
  governs capture rate, ROI policy, trigger rules, consumption mode, and disk protection.
- **Session Artifact Set**: The complete set of files (`session.json`, `video.mp4`,
  `frames.jsonl`, `events.jsonl`) that together constitute a valid, complete session.

---

## Success Criteria *(mandatory)*

### Measurable Outcomes

<!--
  gameQA Constitution (Principle V): Every numeric success criterion listed here MUST be covered
  by an automated performance regression test. CI must fail if a regression occurs.
-->

- **SC-001**: Capture latency (display-acquisition to memory-arrival) MUST be ≤ 5 ms at P95 over
  any continuous 5-minute measurement window.
- **SC-002**: Frame rate consistency: average 30 ± 2 fps; frame interval P95 ≤ 40 ms; at most
  1 gap > 66 ms per minute; single maximum gap ≤ 200 ms.
- **SC-003**: Scheduling drift: median schedule error ≤ 5 ms and P95 ≤ 20 ms over any 1-hour
  measurement window.
- **SC-004**: Real-time mode end-to-end delivery: capture-to-ring-buffer-arrival MUST be ≤ 8 ms
  at P95.
- **SC-005**: Input hook-to-queue latency MUST be ≤ 10 ms at P99.
- **SC-006**: Input data integrity: zero event losses per session; zero keydown/keyup pairing
  errors per session.
- **SC-007**: Orphan event rate: events without a linkable preceding frame ≤ 0.1% of total events
  per session; no continuous orphan window exceeding 1 second.
- **SC-008**: Game performance impact: capture-on vs. capture-off comparison shows ≤ 3% reduction
  in average game FPS and ≤ 5% increase in game P99 frametime.
- **SC-009**: Batch automation: ≥ 99% session completion rate over a 600+ session run; zero
  incomplete or corrupt sessions in `data/sessions/`.
- **SC-010**: Non-intrusion compliance: automated audit confirms zero instances of WRITE or DEBUG
  process handle access throughout any session.
- **SC-011**: All SC-001 through SC-008 thresholds apply equally with debug mode enabled and
  disabled.

---

## Assumptions

- Target game for this phase: Chrome Dino only. Other games are explicitly out of scope.
- Target OS: Windows. Linux support is a future consideration, not a current requirement.
- **US3 is deferred**: The live inference bot is not built in this iteration. The ring buffer
  interface and contracts are built now (US3 scope); the real consumer implementation is a
  separate future feature.
- The ring buffer consumer protocol (shared memory, callback, IPC, etc.) is a design decision
  for the planning phase; this spec requires only that a defined abstract interface exists.
- Video output format (codec, container, quality settings) is profile-configurable; no defaults
  are hardcoded in source.
- For bot profile with recording disabled, `session.json` alone constitutes a minimal complete
  record; absence of `video.mp4` and `frames.jsonl` is expected and not flagged as incomplete.
