# Research: 비침투적 게임 데이터 수집 파이프라인

**Phase**: 0 — Technology Decisions
**Date**: 2026-02-26
**Branch**: `001-capture-pipeline`

All NEEDS CLARIFICATION items from Technical Context are resolved below.

---

## Decision 1: Implementation Language

**Decision**: Rust (stable, 2024 edition, ≥ 1.82)

**Rationale**: The project constitution explicitly uses Rust idioms (`unwrap()`, `expect()`)
in Principle IV. Rust's ownership model structurally prohibits global mutable state (Principle
IV) and zero-cost abstractions enable interface-driven design (Principle III) without runtime
overhead. The 5 ms capture latency target requires a systems language. Rust's safety guarantees
significantly reduce the risk of data corruption in a long-running unattended process (US2).

**Alternatives considered**:
- C++: Viable for performance, but lacks compile-time ownership guarantees; global mutable state
  is easy to introduce accidentally; no native Result<> propagation.
- Python: Insufficient for P95 ≤ 5 ms capture latency; GIL limits threading model.

**Key risks**: None — Rust is the intended language per the constitution.

---

## Decision 2: Screen Capture Backend — DXGI Desktop Duplication

**Decision**: `windows-rs` (`windows` crate, actively maintained by Microsoft, 2025–26
releases) combined with `win_desktop_duplication` crate as a high-level wrapper.

**Rationale**:
- DXGI Desktop Duplication API captures the compositor's output at the GPU level; it requires
  zero game-process handles and satisfies Principle I completely.
- `win_desktop_duplication` provides low-latency access to raw GPU textures with documented
  sub-10 ms latency; zero-buffering design achieves the P95 ≤ 5 ms target.
- `windows-rs` is first-party Microsoft Rust bindings, actively maintained with 2025–26 releases.

**Alternatives considered**:
- GDI/BitBlt: CPU-side pixel copy, ~20–30 ms latency — violates Principle II.
- Windows Graphics Capture API (WGC): Newer, better for UWP apps, fewer low-level controls;
  considered as a future fallback backend via the `CaptureBackend` abstraction.
- Raw DXGI without wrapper: Possible but significantly more boilerplate for no gain.

**Key risks**:
- DXGI only delivers a new frame when screen content changes; static game screens may not
  produce frames at 30 fps. Mitigation: detect frame-delivery pauses and synthesize timing
  markers in `frames.jsonl` rather than blocking the session.
- Requires WDDM (Windows Display Driver Model) — present on all Windows 8+ systems with a GPU.

---

## Decision 3: Input Event Capture — Low-Level Windows Hooks

**Decision**: `willhook` crate (v0.5.0, feature-complete) for WH_KEYBOARD_LL and WH_MOUSE_LL
hooks with dedicated message-loop threads per hook type.

**Rationale**:
- WH_KEYBOARD_LL and WH_MOUSE_LL (**low-level** variants) run their callback in the
  hook-installing process — they do NOT inject DLLs into any other process. This is a critical
  distinction from regular WH_KEYBOARD/WH_MOUSE hooks, which do inject. LL hooks are
  fully non-intrusive (Principle I).
- `willhook` implements the correct thread model: dedicated background thread per hook type
  with an internal Windows message pump, delivering events via channels. The hook callback
  never blocks; events are immediately pushed to a crossbeam channel.
- P99 ≤ 10 ms hook-to-queue target is achievable: hook callback fires on interrupt, channel
  push is O(1) lock-free; on modern CPUs the round-trip is < 0.5 ms in P99.

**Alternatives considered**:
- Raw `SetWindowsHookEx` + `windows-rs`: Requires manually implementing the message loop and
  thread synchronization — error-prone; `willhook` encapsulates this correctly.
- `win-hotkeys`: Only for hotkey registration, not full event capture.
- DLL injection hooks: Intrusive; prohibited by Principle I.

**Key risks**:
- Hook callback MUST NOT panic. Any panic in an LL hook callback crashes the OS input system.
  All code in the callback path MUST use `?` propagation and never `unwrap()`/`expect()`.
- Windows may enforce a timeout on LL hook callbacks (~300 ms on Windows 10). The `willhook`
  design satisfies this by completing the callback before the timeout.

---

## Decision 4: Video Encoding — H.264/MP4

**Decision**: `ffmpeg-next` crate on a dedicated encoder thread. The capture thread pushes raw
BGRA frames to a bounded queue; the encoder thread pulls and produces H.264/MP4 output.

**Rationale**:
- `ffmpeg-next` is the most mature FFmpeg Rust binding; widely used for multimedia tasks.
- Off-thread encoding means zero impact on capture latency (Principle II explicit trade-off:
  encoding latency is sacrificed for capture latency — justified because the MP4 archive is
  not a real-time signal).
- H.264 @ CRF 23 at 30 fps for a ~400×150 ROI produces ~1–3 ms encoding time per frame and
  ~2–5 MB/s disk throughput — well within disk I/O limits.

**Alternatives considered**:
- Windows Media Foundation (WMF) direct API: < 1 ms better than ffmpeg per frame; not worth
  the complexity increase for a < 5 ms total budget with ample headroom.
- GStreamer Rust bindings: Cross-platform overkill for Windows-only requirement.
- Raw frame storage without encoding: ~50× higher disk I/O; infeasible for 600+ sessions.

**Key risks**:
- If the encoder thread falls behind, the frame queue fills. Mitigation: monitor queue depth
  via `PipelineMetrics`; drop encoder frames (distinct from capture pipeline drops) when the
  queue is full, logging a warning — video quality degrades before capture latency degrades.
- `h264_mf` (hardware encoder) may be absent on some Windows editions. Fallback: `libx264`
  software encoder (slightly higher CPU, ~30–50 ms encoding, still off-thread).

---

## Decision 5: ROI Detection — Template Matching

**Decision**: `opencv` Rust bindings (`opencv` crate) using `match_template` with
`TM_CCOEFF_NORMED` normalization.

**Rationale**:
- OpenCV's `matchTemplate` is highly optimized (SIMD kernels) and achieves ~0.5–1 ms per
  frame for a 400×150 ROI template on a modern CPU — within the 5 ms capture budget.
- `TM_CCOEFF_NORMED` is robust to uniform brightness changes and produces a normalized [0,1]
  score directly comparable to the 0.75 threshold (FR-007).
- No maintained pure-Rust alternative exists at equivalent performance.

**Alternatives considered**:
- Pure-Rust cross-correlation: Achievable but ~5–10 ms per frame — too slow at 30 fps.
- SSIM-based matching: More robust to partial occlusion but higher compute cost.
- Neural detection (ONNX): Overkill for fixed-ROI template detection; reserved for the
  `TerminationTrigger` InferenceModelTrigger (future, via FR-018 extensibility point).

**Key risks**:
- False positives at threshold 0.75 for visually similar regions. Mitigation: 2-frame
  confirmation (FR-010 `reacquire_confirm_frames`) filters single-frame spurious matches.
- OpenCV is a large dependency (~50 MB binary). It does not inject into any process; non-
  intrusion audit should whitelist it explicitly.

---

## Decision 6: Ring Buffer — Drop-Oldest Semantics

**Decision**: `thingbuf` crate — lock-free MPSC bounded ring buffer; when full, the oldest
entry is overwritten, so the consumer always reads the most recent frame.

**Rationale**:
- `thingbuf` provides zero-allocation-per-push lock-free semantics with fixed array backing.
  At 2–3 frame capacity (~360 KB for 400×150 BGRA), pre-allocation is deterministic.
- Built-in overwrite-oldest behavior matches the spec requirement (FR-029) exactly.
- Hook-to-queue latency for the ring buffer push is ~0.05 ms — negligible in the 8 ms E2E
  budget.

**Alternatives considered**:
- `crossbeam-channel` bounded: Blocks producer when full — does NOT drop oldest; requires a
  custom wrapper that reads-and-discards, adding complexity and a potential deadlock.
- `ringbuf` crate: SPSC only (single-producer, single-consumer); unsuitable here.
- `rtrb` real-time ring buffer: Also SPSC only.

**Key risks**:
- No backpressure signal to the producer when frames are dropped. Mitigation: `PipelineMetrics`
  exposes `drop_count` (atomic u64) — observable without code changes.
- Consumer crash leaves producer pushing to a dead ring buffer. Mitigation: consumer health
  check watchdog (periodic ping every 100 ms); session terminates on timeout.

---

## Decision 7: Session Storage Integrity — Atomic Two-Phase Commit

**Decision**: Write all session artifacts to `data/.wip/{session_id}/`; on successful session
completion, rename the directory to `data/sessions/{session_id}/` using `std::fs::rename()`.
On any failure, move to `data/incomplete/`.

**Rationale**:
- On NTFS (same volume), directory rename is effectively atomic: no external observer can see
  a partial state between `data/.wip/` and `data/sessions/`.
- For extra safety on critical writes, use `windows::Win32::Storage::FileSystem::MoveFileExW`
  with `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` when renaming individual files
  within the session directory before the final directory rename.
- Recovery scan at pipeline startup: any directory in `data/sessions/` missing any of the four
  required artifacts is moved to `data/incomplete/` (catches crash-during-rename edge cases).

**Alternatives considered**:
- Copy + delete: Non-atomic; external readers can see incomplete state.
- `MoveFileTransacted`: Deprecated by Microsoft; risky.
- SQLite WAL: Overkill for a simple file-based session store.

**Key risks**:
- Cross-volume rename is NOT atomic. Mitigation: validate at config load time that
  `data/.wip/` and `data/sessions/` reside on the same volume; ERROR if not.
- Power loss during rename: Recovery scan at startup handles this correctly.

---

## Decision 8: Monotonic Clock

**Decision**: `std::time::Instant` for all in-session monotonic timestamps.
`std::time::SystemTime` for the wall-clock anchor in `session.json` only.

**Rationale**:
- `std::time::Instant` on Windows uses `QueryPerformanceCounter` internally — monotonic,
  < 1 µs resolution, zero-overhead in release builds (inline).
- The 5 ms latency budget means clock-call overhead (< 0.2 µs) is < 0.004% — negligible.
- Typed distinction (`MonotonicNs` vs. `WallNs` type aliases) enforces FR-023/FR-024 at
  compile time; mixing them is a type error.

**Alternatives considered**:
- Direct `QueryPerformanceCounter` via `windows-rs`: < 0.1 µs improvement — not worth
  the added unsafe code.
- `GetTickCount64`: 15.625 ms resolution — insufficient for P95 ≤ 5 ms measurement.
- `RDTSC`: CPU-dependent, no calibration built in; Instant already uses invariant TSC.

**Key risks**:
- On virtualised hosts (Hyper-V, VMware), QPC can have large jumps. Target hardware is a
  physical Windows gaming PC — risk is low. Detect VM at startup and warn if applicable.
