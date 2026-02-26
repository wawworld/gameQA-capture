# Contract: Session Data Schemas

**Output artifacts**: `session.json`, `frames.jsonl`, `events.jsonl`, `video.mp4`
**Written by**: `SessionWriter` in `src/session/storage.rs`
**Consumed by**: downstream ML pipeline, game state analyzer, real-time bot

---

## session.json

One file per session. Written atomically at session commit.

```json
{
  "session_id": "20260226T143000Z-chrome_dino-ops",
  "ts_wall_start_ns": 1740578400000000000,
  "ts_mono_start_ns": 0,
  "ts_wall_end_ns": 1740578700000000000,
  "ts_mono_end_ns": 300000000000,
  "game_profile": "chrome_dino",
  "active_profile": "ops",
  "mode": "recording",
  "gap_flag": false,
  "status": "complete"
}
```

**Field rules**:
- `ts_mono_start_ns` is always `0` (all frame/event timestamps are relative offsets).
- `ts_wall_end_ns` and `ts_mono_end_ns` are present only when `record_epoch_perf_pair_on_end`
  is `true` (ops and bot profiles). In test profile these fields are absent (not `null`).
- `status` is set at commit time; `incomplete` sessions are never written to `data/sessions/`.

**Version**: schema version is implicit in field presence. Future schema changes MUST add
optional fields only; required fields MUST NOT be removed.

---

## frames.jsonl

One JSON line per captured frame, written in real-time during the session.
Lines are flushed to disk in batches (configurable, default: every 100 frames or 1 second).

```json
{"frame_id":0,"capture_ts_ns":0,"dxgi_acquire_ts_ns":null,"buffer_ready_ts_ns":null}
{"frame_id":1,"capture_ts_ns":33201847,"dxgi_acquire_ts_ns":null,"buffer_ready_ts_ns":null}
{"frame_id":2,"capture_ts_ns":66724130,"dxgi_acquire_ts_ns":2891040,"buffer_ready_ts_ns":4123560}
```

**Field rules**:
- `frame_id`: zero-based, strictly monotonically increasing. No gaps.
- `capture_ts_ns`: nanoseconds elapsed since session start (= `ts_mono_start_ns = 0`).
- `dxgi_acquire_ts_ns` and `buffer_ready_ts_ns`: both present or both `null`. Never mixed.
  When present: `dxgi_acquire_ts_ns` ≤ `buffer_ready_ts_ns` ≤ `capture_ts_ns`.

**Performance note**: A 5-minute session at 30 fps produces 9,000 lines (~450 KB uncompressed).

---

## events.jsonl

One JSON line per input event, written in real-time.

```json
{"event_id":0,"event_ts_ns":125430210,"queue_push_done_ts_ns":125431000,"type":"keyboard_press","payload":{"vk_code":32,"scan_code":57,"flags":0},"prev_frame_id":3,"prev_frame_ts_ns":100441270,"dt_prev_ns":24988940}
{"event_id":1,"event_ts_ns":300112540,"queue_push_done_ts_ns":300113200,"type":"keyboard_release","payload":{"vk_code":32,"scan_code":57,"flags":0},"prev_frame_id":9,"prev_frame_ts_ns":299781300,"dt_prev_ns":331240}
{"event_id":2,"event_ts_ns":12000000,"queue_push_done_ts_ns":12000500,"type":"keyboard_press","payload":{"vk_code":32,"scan_code":57,"flags":0},"prev_frame_id":null,"prev_frame_ts_ns":null,"dt_prev_ns":null}
```

(event_id 2 above is an orphan: captured before the first frame was delivered)

**Field rules**:
- `event_ts_ns` ≤ `queue_push_done_ts_ns`. Difference = hook latency (SC-005 measurement).
- `prev_frame_id` is `null` iff this event is an orphan (no preceding frame within session).
- `dt_prev_ns` may be negative in edge cases (event timestamped before linked frame due to
  OS scheduling). Consumers MUST handle negative values.
- `type` is one of: `keyboard_press`, `keyboard_release`, `mouse_move`, `mouse_click`,
  `mouse_scroll`.
- Mouse payload fields depend on type (see data-model.md §4).

---

## video.mp4

- Codec: H.264 (libx264 or h264_mf)
- Container: MP4
- Frame rate: as configured (default 30 fps)
- Dimensions: equal to the locked ROI dimensions
- CRF: configurable (default 23)
- Present only when `ConsumerConfig.recording_enabled = true`

**Timestamp note**: The video file's internal PTS (presentation timestamps) correspond to
frame sequence numbers. To correlate video frames with `frames.jsonl`, use `frame_id` as the
PTS index: `pts = frame_id`, timebase = `1 / target_fps`.

---

## Directory Layout

```text
data/
├── sessions/
│   └── {session_id}/
│       ├── session.json    # Written last (commit marker)
│       ├── video.mp4       # Written during session, finalized at commit
│       ├── frames.jsonl    # Written during session
│       └── events.jsonl    # Written during session
├── incomplete/
│   └── {session_id}/       # Interrupted sessions (may be missing artifacts)
└── .wip/
    └── {session_id}/       # In-progress session (not externally visible as complete)
```

**Commit protocol**:
1. All artifacts written to `data/.wip/{session_id}/`
2. `session.json` written last within the wip directory (signals artifact completeness)
3. Directory renamed atomically: `data/.wip/{session_id}/` → `data/sessions/{session_id}/`
4. On any failure before step 3: content moved to `data/incomplete/{session_id}/`

**Recovery at startup**: Scan `data/sessions/` for directories missing any required artifact;
move them to `data/incomplete/`. This handles crash-during-rename.
