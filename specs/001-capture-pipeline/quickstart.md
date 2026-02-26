# Quickstart: 비침투적 게임 데이터 수집 파이프라인

**Branch**: `001-capture-pipeline`
**Target OS**: Windows 10/11 (WDDM GPU required)

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust toolchain | stable ≥ 1.82 | `rustup install stable` |
| OpenCV | 4.8+ | See §1 below |
| FFmpeg dev libraries | 6.x+ | See §2 below |
| MSVC build tools | VS 2022 | `winget install Microsoft.VisualStudio.2022.BuildTools` |

---

## 1. OpenCV Setup (Windows)

```powershell
# Option A: vcpkg (recommended)
vcpkg install opencv4[contrib]:x64-windows
$env:OPENCV_DIR = "C:\vcpkg\packages\opencv4_x64-windows"

# Option B: pre-built binary
# Download from https://opencv.org/releases/ and set:
$env:OPENCV_DIR = "C:\opencv\build"
$env:Path += ";$env:OPENCV_DIR\x64\vc17\bin"
```

Add to `Cargo.toml` (handled by the project's build.rs):
```toml
[dependencies]
opencv = { version = "0.91", features = ["clang-runtime"] }
```

---

## 2. FFmpeg Setup (Windows)

```powershell
# Install ffmpeg dev libraries via vcpkg
vcpkg install ffmpeg:x64-windows

# OR download pre-built shared libs (gyan.dev builds):
# Extract to C:\ffmpeg and set:
$env:FFMPEG_DIR = "C:\ffmpeg"
```

---

## 3. Build

```powershell
# Debug build (not for performance measurement)
cargo build

# Release build (required for all performance testing)
cargo build --release
```

---

## 4. Configuration

All configuration is profile-based. Start from the three-tier hierarchy:

```
config/base.yaml          ← all defaults
config/games/chrome_dino.yaml  ← game-specific overrides
config/profiles/test.override.yaml   ← test profile
config/profiles/ops.override.yaml    ← ops profile
config/profiles/bot.override.yaml    ← bot profile
```

The merged config for a run is: `base.yaml` ← `chrome_dino.yaml` ← `{profile}.override.yaml`.

### Verify profile merge

```powershell
cargo run --release -- config dump --game chrome_dino --profile test
```

Prints the fully-merged config. Use this to confirm that no value is sourced from hardcoded
defaults in source code (Principle IV).

---

## 5. Run: Test Profile (US1 — Single Session Validation)

```powershell
# 1. Open Chrome Dino game in browser
# 2. Ensure the game window is in the foreground
# 3. Start a test-profile session:
cargo run --release -- capture --game chrome_dino --profile test

# Output:
# [INFO] Loading profile: chrome_dino + test
# [INFO] Searching for ROI... (timeout: 30s)
# [INFO] ROI locked at (x=120, y=45, w=640, h=180), score=0.92
# [INFO] ROI stable for 1.0s, discarding preroll (2.0s)...
# [INFO] Capture started. Session: 20260226T143000Z-chrome_dino-test
# [INFO] Press F9 to stop.
# ...
# [INFO] Session committed: data/sessions/20260226T143000Z-chrome_dino-test/
# [INFO] Artifacts: session.json, video.mp4, frames.jsonl, events.jsonl
```

### Validate session artifacts

```powershell
# Confirm all four files exist
ls data/sessions/20260226T143000Z-chrome_dino-test/

# Check performance metrics from session.json
cargo run --release -- validate --session data/sessions/20260226T143000Z-chrome_dino-test/
# Prints: capture latency P95, frame interval P95, hook latency P99, orphan rate, gap count
```

---

## 6. Run: Ops Profile (US2 — Batch Automation)

```powershell
cargo run --release -- capture --game chrome_dino --profile ops --auto

# Output:
# [INFO] Automation mode: batch_recording, target: 600 sessions
# [INFO] Session 1/600 starting...
# [INFO] ROI locked, capturing...
# [INFO] Termination trigger: game_over template matched (3 frames confirmed)
# [INFO] Session 1 committed (5m 12s). Restarting in 3s...
# [INFO] Session 2/600 starting...
# ...
# [WARN] Disk free space below 20GB warning threshold
# [INFO] Continuing (above halt threshold 10GB)...
```

### Monitor batch progress

```powershell
# Live session count
ls data/sessions/ | Measure-Object | Select-Object Count

# Check for failures
ls data/incomplete/ | Measure-Object | Select-Object Count
```

---

## 7. Run: Bot Profile (US3 — Interface Readiness)

```powershell
cargo run --release -- capture --game chrome_dino --profile bot --mock-consumer

# --mock-consumer attaches a stub consumer to the ring buffer interface
# and prints frame delivery latency statistics

# Output:
# [INFO] Bot profile: real-time mode, ring buffer capacity: 2 frames
# [INFO] Mock consumer attached to FrameChannel
# [INFO] Capturing... Press F9 to stop.
# ...
# [INFO] Mock consumer stats:
#   Frames received: 9187
#   E2E latency P95: 6.2ms  ✓ (target: ≤8ms)
#   Ring buffer drops: 0
```

---

## 8. Run Performance Tests

```powershell
# All performance regression tests (requires ~6 minutes for scheduling drift)
cargo test --release -- --test-threads=1 perf

# Individual tests
cargo test --release -- perf::capture_latency   # SC-001
cargo test --release -- perf::frame_consistency # SC-002
cargo test --release -- perf::hook_latency      # SC-005
cargo test --release -- perf::realtime_e2e      # SC-004
```

---

## 9. Run Session Integrity Tests

```powershell
# Abnormal shutdown, disk-full, thread-panic scenarios
cargo test --release -- integration::session_integrity

# Non-intrusion audit (SC-010)
cargo test --release -- integration::non_intrusion
# NOTE: Requires running Chrome Dino in the background during this test.
# The test inspects live process handles and verifies zero WRITE/DEBUG access.
```

---

## 10. Debug Mode

```powershell
cargo run --release -- capture --game chrome_dino --profile test --debug

# A separate window opens showing the current capture ROI in real-time.
# Debug rendering runs on an independent thread.
# Run the performance tests simultaneously to verify SC-011 (debug mode parity).
```

---

## 11. Verify Non-Intrusion (Manual)

During any active session, open Process Hacker (or Task Manager) and inspect the game
process's handle list:
- `gameQA.exe` MUST NOT appear in the game process's handle list.
- The game process's security properties MUST show no entries from `gameQA.exe`.
- The automated test `integration::non_intrusion` performs this check programmatically
  using `GetProcessAccessFlags`.

---

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `CaptureError::DeviceLost` at startup | DXGI not available | Ensure WDDM GPU driver is installed; try restarting the display driver |
| ROI detection timeout (30s) | Reference image doesn't match current game skin | Update `config/games/chrome_dino.yaml` reference images |
| `capture_latency P95 > 5ms` in perf test | CPU overloaded or GPU driver issue | Pin capture thread to a dedicated core; close background GPU tasks |
| `queue_push_done - event_ts > 10ms` | Hook thread starved | Raise hook thread priority to `THREAD_PRIORITY_TIME_CRITICAL` (profile setting) |
| Missing `video.mp4` on session complete | ffmpeg library not found | Verify `FFMPEG_DIR` env var and that DLLs are on PATH |
| All sessions go to `data/incomplete/` | Disk on different volume from `.wip/` | Ensure `data/` is on the same NTFS volume; update config if needed |
