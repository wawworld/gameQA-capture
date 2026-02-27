# gameQA Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-02-26

## Active Technologies

- Rust 2024 edition (stable, ≥ 1.82) (001-capture-pipeline)

## Project Structure

```text
src/
├── lib.rs              # MonotonicNs / WallNs newtypes; module declarations
├── main.rs             # CLI: capture | config dump | validate
├── capture/            # CaptureBackend trait + DxgiBackend stub
├── config/             # ProfileConfig structs + YAML merge (load_merged_profile)
├── hooks/              # InputHookBackend trait + WillhookBackend stub + MouseMoveFilter
├── pipeline/           # Pipeline orchestrator, FrameConsumer, PipelineMetrics, ring_buffer, recorder
├── roi/                # RoiManager state machine + TemplateMatchDetector stub
├── session/            # SessionRecord, SessionWriter (two-phase commit), lifecycle, triggers
├── automation/         # SessionScheduler + QuarantineManager
├── debug/              # DebugPreview stub
└── audit/              # ProcessHandleAudit stub
config/
├── base.yaml           # All defaults (source of truth — no hardcoded defaults in src/)
├── games/chrome_dino.yaml
└── profiles/{test,ops,bot}.override.yaml
tests/
├── unit/               # Mock backends + contract tests (T013–T017)
├── perf/               # SC-001–SC-008 regression tests (T018–T020, T037, T043, T055)
└── integration/        # Session lifecycle, integrity, batch automation (T021–T023, T038, T044)
benches/                # Criterion benchmarks (one per perf SC)
```

## Commands

```sh
cargo check                                    # compile check (no Rust install needed for file structure)
cargo test                                     # unit + integration tests
cargo test --test session_integrity            # session commit/abort tests (no live game needed)
cargo test --test batch_automation             # automation scheduler tests (no live game needed)
cargo clippy --all-targets -- -D warnings      # lint (requires #![deny(clippy::unwrap_used)])
cargo fmt                                      # format
cargo bench                                    # performance regression suite (requires release build)
cargo run --release -- capture --game chrome_dino --profile test    # single session
cargo run --release -- capture --game chrome_dino --profile ops --auto  # batch automation
cargo run --release -- config dump --game chrome_dino --profile test    # dump merged config
```

## Key Decisions

- **Newtype clocks**: `MonotonicNs(u64)` and `WallNs(u64)` are distinct structs, NOT type aliases — mixing is a compile error.
- **No unwrap/expect in production**: enforced via `#![deny(clippy::unwrap_used, clippy::expect_used)]` in every `src/` module.
- **Three-tier YAML merge**: `base.yaml` ← `games/{game}.yaml` ← `profiles/{profile}.override.yaml` with dict-merge, list-replace, null-remove semantics.
- **Two-phase commit**: session writes go to `.wip/` first; atomic rename to `sessions/` on success; move to `incomplete/` on failure.
- **Non-intrusion**: DXGI Desktop Duplication (read-only compositor) + WH_KEYBOARD_LL / WH_MOUSE_LL (no DLL injection). Zero game-process handle access.
- **Off-thread encoding**: ffmpeg-next H.264 encoding on a dedicated thread; capture thread is never blocked by disk I/O.

## Windows-Only Dependencies

- `windows` (windows-rs) — DXGI, WinAPI
- `win_desktop_duplication` — DXGI capture wrapper (requires WDDM driver)
- `willhook` — WH_KEYBOARD_LL / WH_MOUSE_LL
- `ffmpeg-next` — requires `FFMPEG_DIR` env var set to FFmpeg dev libraries
- `opencv` — requires `OPENCV_DIR` env var; see `specs/001-capture-pipeline/quickstart.md`

## Code Style

Rust 2024 edition (stable, ≥ 1.82): Follow standard conventions

## Recent Changes

- 001-capture-pipeline: Added Rust 2024 edition (stable, ≥ 1.82)
- 001-capture-pipeline: Implemented Phase 1 (scaffold), Phase 2 (foundational types + traits), Phase 3–5 stubs (US1/US2/US3 test + impl skeleton). Platform-specific implementations (DxgiBackend, WillhookBackend, opencv ROI, ffmpeg recorder) are stubbed with TODO markers for hardware-dependent completion.

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
