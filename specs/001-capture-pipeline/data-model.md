# Data Model: 비침투적 게임 데이터 수집 파이프라인

**Phase**: 1 — Design
**Date**: 2026-02-26
**Branch**: `001-capture-pipeline`

---

## 1. Type Aliases (Clock Source Distinction)

Per Constitution Principle IV, monotonic and wall-clock timestamps MUST be distinct types.

```rust
/// Nanoseconds elapsed since session start (std::time::Instant, monotonic).
/// Used for all frame and event timestamps.
pub type MonotonicNs = u64;

/// Nanoseconds since UNIX epoch (std::time::SystemTime, wall-clock).
/// Used ONLY in session.json for external log joining.
pub type WallNs = u64;
```

These are **compile-enforced** distinctions: a function accepting `MonotonicNs` cannot
accidentally receive a `WallNs` value.

---

## 2. Session

**Rust struct: `SessionRecord`** — serialized to `session.json`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `session_id` | String | Yes | Format: `{YYYYMMDDTHHMMSSZ}-{game}-{profile}` |
| `ts_wall_start_ns` | WallNs | Yes | Wall-clock at session start (UNIX epoch, ns) |
| `ts_mono_start_ns` | MonotonicNs | Yes | Always zero (session timestamp origin) |
| `ts_wall_end_ns` | Option\<WallNs\> | Profile | ops/bot: required. test: null |
| `ts_mono_end_ns` | Option\<MonotonicNs\> | Profile | ops/bot: required. test: null |
| `game_profile` | String | Yes | e.g. `"chrome_dino"` |
| `active_profile` | String | Yes | `"test"` \| `"ops"` \| `"bot"` |
| `mode` | SessionMode | Yes | `"recording"` \| `"realtime"` \| `"both"` |
| `gap_flag` | bool | Yes | True if any Δt > 66 ms occurred |
| `status` | SessionStatus | Yes | See below |

**SessionStatus enum**: `complete` | `incomplete` | `low_quality`

**SessionMode enum**: `recording` | `realtime` | `both`

**Validation rules**:
- `session_id` MUST be unique across `data/sessions/`
- `ts_mono_start_ns` is always 0 (all frame/event timestamps are relative to session start)
- `ts_wall_end_ns` and `ts_mono_end_ns` are null iff `active_profile == "test"`
- A `status: complete` session MUST have all four artifacts present (or three if
  `mode == realtime` with recording disabled)

**State transitions**:
```
[not started]
    │
    ▼ session initialized (preroll begins)
[preroll]
    │
    ▼ ROI stable for roi_stable_seconds
[capturing]
    │   ┌── ROI lost + grace exceeded → mark_low_quality (ops) or stop_bot (bot)
    │   └── ROI lost + reacquired within grace → continue
    │
    ▼ termination trigger fires + commit succeeds
[complete]

[capturing] ── abnormal stop ──► [incomplete] → moved to data/incomplete/
```

---

## 3. Frame

**Rust struct: `FrameRecord`** — one JSON line per frame in `frames.jsonl`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `frame_id` | u64 | Yes | Monotonically increasing, 0-based within session |
| `capture_ts_ns` | MonotonicNs | Yes | Monotonic offset from session start |
| `dxgi_acquire_ts_ns` | Option\<MonotonicNs\> | Diagnostic | DXGI frame acquisition time. null if diagnostics disabled |
| `buffer_ready_ts_ns` | Option\<MonotonicNs\> | Diagnostic | Memory arrival time. null if diagnostics disabled |

**Validation rules**:
- `frame_id` MUST be strictly increasing (no gaps, no duplicates within a session)
- `capture_ts_ns` MUST be strictly increasing
- If `dxgi_acquire_ts_ns` is non-null, `buffer_ready_ts_ns` MUST also be non-null
- `dxgi_acquire_ts_ns` ≤ `buffer_ready_ts_ns` ≤ `capture_ts_ns` (acquisition happens before
  memory arrival, which happens before the pipeline receives the frame)

**Schema compatibility**: When diagnostics are disabled, both diagnostic fields are `null`.
Downstream consumers MUST handle both null and non-null values for backward compatibility.

---

## 4. Event

**Rust struct: `EventRecord`** — one JSON line per event in `events.jsonl`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `event_id` | u64 | Yes | Monotonically increasing, 0-based within session |
| `event_ts_ns` | MonotonicNs | Yes | Monotonic hook-receipt timestamp (= hook_recv_ts) |
| `queue_push_done_ts_ns` | MonotonicNs | Yes | Timestamp after channel push completes |
| `type` | EventType | Yes | See enum below |
| `payload` | EventPayload | Yes | Type-specific data |
| `prev_frame_id` | Option\<u64\> | Yes | ID of the most recent frame before this event. null = orphan |
| `prev_frame_ts_ns` | Option\<MonotonicNs\> | Yes | Timestamp of prev_frame. null = orphan |
| `dt_prev_ns` | Option\<i64\> | Yes | event_ts_ns − prev_frame_ts_ns. null = orphan. Negative values indicate clock anomaly |

**EventType enum**:
- `keyboard_press` — key down
- `keyboard_release` — key up
- `mouse_move` — cursor moved
- `mouse_click` — button press/release
- `mouse_scroll` — wheel scroll

**EventPayload** (type-specific):
```json
// keyboard_press / keyboard_release
{ "vk_code": 32, "scan_code": 57, "flags": 0 }

// mouse_move
{ "x": 640, "y": 480, "dx": 5, "dy": -3 }

// mouse_click
{ "x": 640, "y": 480, "button": "left", "action": "press" }

// mouse_scroll
{ "delta": 120, "direction": "vertical" }
```

**Validation rules**:
- `event_id` MUST be strictly increasing within a session
- `event_ts_ns` ≤ `queue_push_done_ts_ns` always
- `queue_push_done_ts_ns - event_ts_ns` is the hook latency (P99 ≤ 10 ms per SC-005)
- `prev_frame_id` is null only for orphan events (rate ≤ 0.1% per SC-007)
- `dt_prev_ns` negative values indicate that the event was timestamped before its linked frame
  (possible under high load); downstream consumers MUST tolerate negative dt

---

## 5. ROI (Region of Interest)

**Runtime-only struct** — not serialized. Held in memory by `RoiManager`.

| Field | Type | Description |
|-------|------|-------------|
| `x` | u32 | Left edge (monitor-relative pixels) |
| `y` | u32 | Top edge (monitor-relative pixels) |
| `width` | u32 | ROI width (≥ 400 px per FR-007) |
| `height` | u32 | ROI height (≥ 150 px per FR-007) |
| `match_score` | f32 | Last template match score [0.0, 1.0] |
| `state` | RoiState | `Searching` \| `Locked` \| `Lost` |
| `locked_at_frame` | u64 | frame_id when lock was first established |

**State machine**:
```
Searching ──match_score ≥ threshold, confirmed──► Locked
Locked ──shift > max_roi_jump_px──► Lost (reacquisition begins)
Lost ──reacquired within grace──► Locked
Lost ──grace exceeded──► on_exceeded action (mark_low_quality or stop_bot)
```

---

## 6. Profile / Configuration

**Runtime-only struct** — deserialized from merged YAML. Not persisted.

Key config groups and their types:

```
CaptureConfig
├── target_fps: u32                      # default: 30
├── crop_to_window: bool                 # default: true
├── crop_mode: CropMode                  # ClientArea | FullWindow
├── diagnostics_enabled: bool            # default: false in test, true in ops
└── backend: CaptureBackendType          # Dxgi (only supported backend currently)

RoiConfig
├── reference_images: Vec<PathBuf>
├── match_threshold: f32                 # default: 0.75
├── min_width_px: u32                    # default: 400
├── min_height_px: u32                   # default: 150
├── padding_px: u32                      # default: 20
├── clamp_to_monitor: bool               # default: true
├── lock_after_first_success: bool       # default: true
├── max_roi_jump_px: u32                 # default: 30
├── reacquire_confirm_frames: u32        # default: 2
└── reacquire_grace_seconds: f64         # per profile

SessionConfig
├── require_roi_detected: bool
├── roi_stable_seconds: f64              # default: 1.0
├── detection_timeout_seconds: f64       # default: 30.0
├── preroll_seconds: f64                 # default: 2.0
├── require_window_foreground: bool
├── pause_when_background: bool
├── background_grace_seconds: f64
└── record_epoch_perf_pair_on_end: bool  # false in test, true in ops/bot

TriggerConfig
├── stop_key: VirtualKey                 # default: F9
├── timeout_seconds: f64                 # default: 300.0 in ops, 600.0 in bot
├── game_over_template: Option<TemplateTriggerConfig>
└── window_lost_grace_seconds: f64

EventCaptureConfig
├── enabled_event_types: Vec<EventType>
└── mouse_move: Option<MouseMoveFilterConfig>
    ├── max_hz: u32                      # default: 20
    └── min_delta_px: u32               # default: 5

ConsumerConfig
├── recording_enabled: bool
├── realtime_enabled: bool
└── ring_buffer_capacity: usize          # default: 3 (ops), 2 (bot)

AutomationConfig
├── enabled: bool
├── mode: AutomationMode                 # BatchRecording | RealtimeBot
├── restart_delay_seconds: f64           # default: 3.0
├── max_consecutive_failures: u32        # default: 3
├── quarantine_on_failure: bool
└── target_session_count: u32

DiskProtectionConfig
├── enabled: bool
├── check_interval_seconds: u64          # default: 60
├── warning_threshold_gb: u64            # default: 20
└── halt_threshold_gb: u64              # default: 10

DebugConfig
└── enabled: bool                        # runs preview on independent thread if true
```

---

## 7. Pipeline Metrics

**Runtime-only struct** — all fields are atomics for lock-free reads.

| Field | Rust Type | Tracks |
|-------|-----------|--------|
| `capture_latency_p95_ns` | AtomicU64 | Rolling P95 of dxgi_acquire → buffer_ready |
| `frame_interval_p95_ns` | AtomicU64 | Rolling P95 of capture_ts[i+1] - capture_ts[i] |
| `schedule_error_p95_ns` | AtomicI64 | Rolling P95 of schedule offset |
| `hook_latency_p99_ns` | AtomicU64 | Rolling P99 of hook_recv → queue_push_done |
| `orphan_event_count` | AtomicU64 | Total events with null prev_frame_id this session |
| `ring_buffer_drop_count` | AtomicU64 | Total ring buffer overwrites this session |
| `session_completion_count` | AtomicU64 | Total sessions completed in this run |
| `queue_depth` | AtomicU64 | Current encoder queue depth |
| `gap_count` | AtomicU64 | Total Δt > 66 ms events this session |

All fields readable from any thread via `load(Ordering::Relaxed)` without locking.
