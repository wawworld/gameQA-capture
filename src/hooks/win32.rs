//! `WillhookBackend` — WH_KEYBOARD_LL + WH_MOUSE_LL via the `willhook` crate.
//!
//! Installs low-level hooks on a dedicated message-loop thread.
//! Events are polled via `Hook::try_recv()` (global channel singleton).
//! `MouseMoveFilter` is applied to mouse-move events (FR-022).
#![deny(clippy::unwrap_used, clippy::expect_used)]

use crate::config::{EventCaptureConfig, EventType};
use crate::hooks::{ClickAction, HookError, InputEvent, InputHookBackend, MouseButton, ScrollDirection};
use crate::MonotonicNs;
use std::sync::Mutex;
use std::time::Instant;
use willhook::event::{
    InputEvent as WHInputEvent, KeyboardKey, KeyPress, MouseButtonPress, MouseEventType,
    MouseWheel, MouseWheelDirection,
};
use willhook::hook::{Hook, HookBuilder};

/// Mouse-move rate-limit and delta filter (FR-022).
///
/// Applied inside the hook backend before events reach the pipeline.
pub struct MouseMoveFilter {
    /// Maximum event rate (events/sec). Default: 20 Hz.
    max_hz: u32,
    /// Minimum Euclidean pixel distance to forward. Default: 5 px.
    min_delta_px: u32,
    /// Timestamp of the last forwarded mouse-move event.
    last_forward_ns: Option<u64>,
    /// Position at the last forwarded mouse-move event.
    last_x: i32,
    last_y: i32,
}

impl MouseMoveFilter {
    pub fn new(max_hz: u32, min_delta_px: u32) -> Self {
        Self {
            max_hz,
            min_delta_px,
            last_forward_ns: None,
            last_x: 0,
            last_y: 0,
        }
    }

    /// Returns `true` if the event should be forwarded.
    pub fn should_forward(&mut self, x: i32, y: i32, now_ns: u64) -> bool {
        let min_interval_ns = if self.max_hz > 0 {
            1_000_000_000u64 / self.max_hz as u64
        } else {
            0
        };

        if let Some(last_ns) = self.last_forward_ns {
            if now_ns.saturating_sub(last_ns) < min_interval_ns {
                return false;
            }
        }

        let dx = (x - self.last_x).unsigned_abs();
        let dy = (y - self.last_y).unsigned_abs();
        let distance = (dx * dx + dy * dy) as f64;
        let min_dist = (self.min_delta_px * self.min_delta_px) as f64;

        if distance < min_dist {
            return false;
        }

        self.last_forward_ns = Some(now_ns);
        self.last_x = x;
        self.last_y = y;
        true
    }
}

/// WH_KEYBOARD_LL + WH_MOUSE_LL hook backend via `willhook`.
///
/// `willhook` uses a process-global hook channel. `start()` installs the hooks;
/// `stop()` drops the handle, which unregisters both hooks and drains the channel.
/// Mutable state shared with `try_recv(&self)` is protected by `Mutex`.
pub struct WillhookBackend {
    config: EventCaptureConfig,
    session_start: Instant,
    /// Live hook handle. `None` before `start()` or after `stop()`.
    hook: Option<Hook>,
    /// Rate-limit / delta filter for mouse-move events.
    mouse_filter: Mutex<MouseMoveFilter>,
    /// Last known absolute cursor position — used to compute dx/dy and to fill
    /// click-event coordinates (willhook `MousePressEvent` carries no position).
    last_mouse_pos: Mutex<(i32, i32)>,
}

impl WillhookBackend {
    pub fn new(config: EventCaptureConfig, session_start: Instant) -> Self {
        let (max_hz, min_delta_px) = config
            .mouse_move
            .as_ref()
            .map(|m| (m.max_hz, m.min_delta_px))
            .unwrap_or((20, 5));
        Self {
            config,
            session_start,
            hook: None,
            mouse_filter: Mutex::new(MouseMoveFilter::new(max_hz, min_delta_px)),
            last_mouse_pos: Mutex::new((0, 0)),
        }
    }

    fn elapsed_ns(&self) -> MonotonicNs {
        MonotonicNs(self.session_start.elapsed().as_nanos() as u64)
    }
}

impl InputHookBackend for WillhookBackend {
    fn start(&mut self) -> Result<(), HookError> {
        if self.hook.is_some() {
            return Ok(());
        }
        let hook = HookBuilder::new()
            .with_keyboard()
            .with_mouse()
            .build()
            .ok_or_else(|| {
                HookError::InstallFailed(
                    "willhook: hook installation failed \
                     (another hook is already active or OS refused the request)"
                        .to_string(),
                )
            })?;
        self.hook = Some(hook);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), HookError> {
        // Dropping the Hook handle calls Hook::drop(), which unregisters
        // WH_KEYBOARD_LL + WH_MOUSE_LL and drains the global channel.
        self.hook = None;
        Ok(())
    }

    fn try_recv(&self) -> Option<InputEvent> {
        let hook = self.hook.as_ref()?;
        // Drain events until one passes all filters, or the channel is empty.
        loop {
            let wh_event = hook.try_recv().ok()?;
            let now_ns = self.elapsed_ns();
            if let Some(event) = convert_event(
                wh_event,
                now_ns,
                &self.config,
                &self.mouse_filter,
                &self.last_mouse_pos,
            ) {
                return Some(event);
            }
        }
    }
}

// ─── Event conversion ─────────────────────────────────────────────────────────

/// Convert a `willhook` event to our `InputEvent`, applying config filters.
///
/// Returns `None` if the event type is disabled in config, filtered by
/// `MouseMoveFilter`, or the event cannot be meaningfully represented.
fn convert_event(
    wh: WHInputEvent,
    now_ns: MonotonicNs,
    config: &EventCaptureConfig,
    mouse_filter: &Mutex<MouseMoveFilter>,
    last_pos: &Mutex<(i32, i32)>,
) -> Option<InputEvent> {
    match wh {
        WHInputEvent::Keyboard(kb) => {
            let vk_code = kb.key.as_ref().map(keyboard_key_to_vk).unwrap_or(0);
            match kb.pressed {
                KeyPress::Down(_) => {
                    if !config.enabled_event_types.contains(&EventType::KeyboardPress) {
                        return None;
                    }
                    Some(InputEvent::KeyPress {
                        vk_code,
                        scan_code: 0,
                        flags: 0,
                        recv_ts_ns: now_ns,
                    })
                }
                KeyPress::Up(_) => {
                    if !config.enabled_event_types.contains(&EventType::KeyboardRelease) {
                        return None;
                    }
                    Some(InputEvent::KeyRelease {
                        vk_code,
                        scan_code: 0,
                        flags: 0,
                        recv_ts_ns: now_ns,
                    })
                }
                KeyPress::Other(_) => None,
            }
        }

        WHInputEvent::Mouse(me) => match me.event {
            MouseEventType::Move(move_evt) => {
                if !config.enabled_event_types.contains(&EventType::MouseMove) {
                    return None;
                }
                // Compute absolute position and delta, updating last_mouse_pos.
                let (x, y, dx, dy) = {
                    let mut guard = last_pos.lock().ok()?;
                    let (px, py) = *guard;
                    let (x, y) = move_evt.point.map(|p| (p.x, p.y)).unwrap_or((px, py));
                    *guard = (x, y);
                    (x, y, x - px, y - py)
                };
                if !mouse_filter.lock().ok()?.should_forward(x, y, now_ns.0) {
                    return None;
                }
                Some(InputEvent::MouseMove { x, y, dx, dy, recv_ts_ns: now_ns })
            }

            MouseEventType::Press(press_evt) => {
                if !config.enabled_event_types.contains(&EventType::MouseClick) {
                    return None;
                }
                let (x, y) = *last_pos.lock().ok()?;
                let button = wh_button_to_ours(&press_evt.button)?;
                let action = match press_evt.pressed {
                    MouseButtonPress::Down => ClickAction::Press,
                    MouseButtonPress::Up => ClickAction::Release,
                    MouseButtonPress::Other(_) => return None,
                };
                Some(InputEvent::MouseClick { x, y, button, action, recv_ts_ns: now_ns })
            }

            MouseEventType::Wheel(wheel_evt) => {
                if !config.enabled_event_types.contains(&EventType::MouseScroll) {
                    return None;
                }
                let direction = match wheel_evt.wheel {
                    MouseWheel::Horizontal => ScrollDirection::Horizontal,
                    MouseWheel::Vertical | MouseWheel::Unknown(_) => ScrollDirection::Vertical,
                };
                let delta = match wheel_evt.direction {
                    Some(MouseWheelDirection::Forward) => 120,
                    Some(MouseWheelDirection::Backward) => -120,
                    Some(MouseWheelDirection::Unknown(_)) | None => 0,
                };
                Some(InputEvent::MouseScroll { delta, direction, recv_ts_ns: now_ns })
            }

            MouseEventType::Other(_) => None,
        },

        WHInputEvent::Other(_) => None,
    }
}

/// Convert a `willhook` `MouseButton` to our `MouseButton`.
///
/// Returns `None` for unknown or unexpected button codes.
fn wh_button_to_ours(btn: &willhook::event::MouseButton) -> Option<MouseButton> {
    use willhook::event::MouseButton as WB;
    match btn {
        WB::Left(_) => Some(MouseButton::Left),
        WB::Right(_) => Some(MouseButton::Right),
        WB::Middle(_) => Some(MouseButton::Middle),
        WB::X1(_) => Some(MouseButton::X1),
        WB::X2(_) => Some(MouseButton::X2),
        WB::UnkownX(_) | WB::Other(_) => None,
    }
}

/// Map a `willhook` `KeyboardKey` to a Windows Virtual Key code (`u16`).
///
/// `willhook` provides a named-key enum; we reverse-map to VK constants so
/// that the rest of the pipeline can use raw VK codes for trigger matching.
/// Keys outside the known set return `0`.
fn keyboard_key_to_vk(key: &KeyboardKey) -> u16 {
    match key {
        KeyboardKey::A => 0x41,
        KeyboardKey::B => 0x42,
        KeyboardKey::C => 0x43,
        KeyboardKey::D => 0x44,
        KeyboardKey::E => 0x45,
        KeyboardKey::F => 0x46,
        KeyboardKey::G => 0x47,
        KeyboardKey::H => 0x48,
        KeyboardKey::I => 0x49,
        KeyboardKey::J => 0x4A,
        KeyboardKey::K => 0x4B,
        KeyboardKey::L => 0x4C,
        KeyboardKey::M => 0x4D,
        KeyboardKey::N => 0x4E,
        KeyboardKey::O => 0x4F,
        KeyboardKey::P => 0x50,
        KeyboardKey::Q => 0x51,
        KeyboardKey::R => 0x52,
        KeyboardKey::S => 0x53,
        KeyboardKey::T => 0x54,
        KeyboardKey::U => 0x55,
        KeyboardKey::V => 0x56,
        KeyboardKey::W => 0x57,
        KeyboardKey::X => 0x58,
        KeyboardKey::Y => 0x59,
        KeyboardKey::Z => 0x5A,
        KeyboardKey::Number0 => 0x30,
        KeyboardKey::Number1 => 0x31,
        KeyboardKey::Number2 => 0x32,
        KeyboardKey::Number3 => 0x33,
        KeyboardKey::Number4 => 0x34,
        KeyboardKey::Number5 => 0x35,
        KeyboardKey::Number6 => 0x36,
        KeyboardKey::Number7 => 0x37,
        KeyboardKey::Number8 => 0x38,
        KeyboardKey::Number9 => 0x39,
        KeyboardKey::LeftAlt => 0xA4,
        KeyboardKey::RightAlt => 0xA5,
        KeyboardKey::LeftShift => 0xA0,
        KeyboardKey::RightShift => 0xA1,
        KeyboardKey::LeftControl => 0xA2,
        KeyboardKey::RightControl => 0xA3,
        KeyboardKey::BackSpace => 0x08,
        KeyboardKey::Tab => 0x09,
        KeyboardKey::Enter => 0x0D,
        KeyboardKey::Escape => 0x1B,
        KeyboardKey::Space => 0x20,
        KeyboardKey::PageUp => 0x21,
        KeyboardKey::PageDown => 0x22,
        KeyboardKey::Home => 0x24,
        KeyboardKey::ArrowLeft => 0x25,
        KeyboardKey::ArrowUp => 0x26,
        KeyboardKey::ArrowRight => 0x27,
        KeyboardKey::ArrowDown => 0x28,
        KeyboardKey::Print => 0x2A,
        KeyboardKey::PrintScreen => 0x2C,
        KeyboardKey::Insert => 0x2D,
        KeyboardKey::Delete => 0x2E,
        KeyboardKey::LeftWindows => 0x5B,
        KeyboardKey::RightWindows => 0x5C,
        KeyboardKey::Comma => 0xBC,
        KeyboardKey::Period => 0xBE,
        KeyboardKey::Slash => 0xBF,
        KeyboardKey::SemiColon => 0xBA,
        KeyboardKey::Apostrophe => 0xDE,
        KeyboardKey::LeftBrace => 0xDB,
        KeyboardKey::BackwardSlash => 0xDC,
        KeyboardKey::RightBrace => 0xDD,
        KeyboardKey::Grave => 0xC0,
        KeyboardKey::F1 => 0x70,
        KeyboardKey::F2 => 0x71,
        KeyboardKey::F3 => 0x72,
        KeyboardKey::F4 => 0x73,
        KeyboardKey::F5 => 0x74,
        KeyboardKey::F6 => 0x75,
        KeyboardKey::F7 => 0x76,
        KeyboardKey::F8 => 0x77,
        KeyboardKey::F9 => 0x78,
        KeyboardKey::F10 => 0x79,
        KeyboardKey::F11 => 0x7A,
        KeyboardKey::F12 => 0x7B,
        KeyboardKey::F13 => 0x7C,
        KeyboardKey::F14 => 0x7D,
        KeyboardKey::F15 => 0x7E,
        KeyboardKey::F16 => 0x7F,
        KeyboardKey::F17 => 0x80,
        KeyboardKey::F18 => 0x81,
        KeyboardKey::F19 => 0x82,
        KeyboardKey::F20 => 0x83,
        KeyboardKey::F21 => 0x84,
        KeyboardKey::F22 => 0x85,
        KeyboardKey::F23 => 0x86,
        KeyboardKey::F24 => 0x87,
        KeyboardKey::NumLock => 0x90,
        KeyboardKey::ScrollLock => 0x91,
        KeyboardKey::CapsLock => 0x14,
        KeyboardKey::Numpad0 => 0x60,
        KeyboardKey::Numpad1 => 0x61,
        KeyboardKey::Numpad2 => 0x62,
        KeyboardKey::Numpad3 => 0x63,
        KeyboardKey::Numpad4 => 0x64,
        KeyboardKey::Numpad5 => 0x65,
        KeyboardKey::Numpad6 => 0x66,
        KeyboardKey::Numpad7 => 0x67,
        KeyboardKey::Numpad8 => 0x68,
        KeyboardKey::Numpad9 => 0x69,
        KeyboardKey::Multiply => 0x6A,
        KeyboardKey::Add => 0x6B,
        KeyboardKey::Separator => 0x6C,
        KeyboardKey::Subtract => 0x6D,
        KeyboardKey::Decimal => 0x6E,
        KeyboardKey::Divide => 0x6F,
        KeyboardKey::Other(vk) => (*vk).min(u16::MAX as u32) as u16,
        KeyboardKey::InvalidKeyCodeReceived => 0,
    }
}
