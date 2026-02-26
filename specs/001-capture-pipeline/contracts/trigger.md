# Contract: TerminationTrigger

**Interface**: `src/session/triggers.rs` — `TerminationTrigger` trait
**Implementors**: `KeyPressTrigger`, `TimeoutTrigger`, `TemplateMatchTrigger`,
  `WindowLostTrigger`; future: `InferenceModelTrigger`
**Consumer**: `SessionManager` (polls all active triggers each frame)

---

## Trait Definition

```rust
use crate::capture::CapturedFrame;
use crate::hooks::InputEvent;

pub trait TerminationTrigger: Send {
    /// Evaluate whether the session should end.
    ///
    /// Called once per captured frame, in order of trigger priority.
    /// MUST return quickly (< 0.5 ms). Heavy computation (template matching)
    /// MUST be amortized or cached within the trigger implementation.
    ///
    /// `events` contains all input events received since the previous call.
    fn check(
        &mut self,
        frame: &CapturedFrame,
        frame_id: u64,
        events: &[InputEvent],
    ) -> TriggerState;
}

pub enum TriggerState {
    /// Session continues.
    Continue,
    /// Session should end with the given reason.
    /// Caller performs confirmation if `requires_confirmation` is true.
    Terminate { reason: String, requires_confirmation: bool },
}
```

---

## Implementations

### KeyPressTrigger
- Checks `events` for the configured key (default: VK_F9).
- Returns `Terminate` immediately on first matching event.
- `requires_confirmation: false` (single event is sufficient).

### TimeoutTrigger
- Compares `frame.capture_ts_ns` against session start + timeout.
- Returns `Terminate` when elapsed time exceeds configured duration.
- `requires_confirmation: false`.

### TemplateMatchTrigger
- Applies `opencv::imgproc::match_template` against the configured game-over reference
  image on the current frame.
- Maintains an internal consecutive-match counter.
- Returns `Terminate` only after `confirm_frames` (default: 3) consecutive frames above
  threshold (default: 0.80).
- `requires_confirmation: false` (confirmation is internal to this trigger).

### WindowLostTrigger
- Monitors the game window handle (checked each frame via Win32 `IsWindow` API).
- Returns `Terminate` after the window has been absent for `window_lost_grace_seconds`.
- `requires_confirmation: false`.

---

## Future Trigger: InferenceModelTrigger (Deferred)

When an ONNX-based inference model is available, a new trigger is added:

```rust
pub struct InferenceModelTrigger {
    model: OnnxSession,        // loaded from profile path
    threshold: f32,
    confirm_frames: u32,
    consecutive_hits: u32,
}

impl TerminationTrigger for InferenceModelTrigger { ... }
```

**Swap contract**: Replace `TemplateMatchTrigger` with `InferenceModelTrigger` by changing the
profile YAML (`trigger.game_over_detector: "inference_model"` instead of `"template"`). Zero
code changes outside `main.rs` trigger factory.

---

## Invariants

1. A trigger MUST NOT modify any session state. It is a pure observer/detector.
2. The `SessionManager` evaluates triggers in declaration order and stops at the first
   `Terminate` result.
3. If no trigger fires, the session continues normally.
4. Trigger construction (loading templates, model weights) happens once at session start;
   `check` is called per-frame without re-initialization.
