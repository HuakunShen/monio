//! Event types and enums for the input hook library.

use crate::keycode::Key;
use std::time::SystemTime;

#[cfg(feature = "recorder")]
use serde::{Deserialize, Serialize};

/// The type of input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub enum EventType {
    /// Hook has been enabled and is now listening.
    HookEnabled,
    /// Hook has been disabled and is no longer listening.
    HookDisabled,

    /// A key was pressed down.
    KeyPressed,
    /// A key was released.
    KeyReleased,
    /// A character was typed (after dead key processing).
    KeyTyped,

    /// A mouse button was pressed.
    MousePressed,
    /// A mouse button was released.
    MouseReleased,
    /// A mouse button was clicked (press + release without movement).
    MouseClicked,
    /// The mouse was moved (no buttons held).
    MouseMoved,
    /// The mouse was moved while a button was held (drag).
    MouseDragged,

    /// The mouse wheel was scrolled.
    MouseWheel,
}

/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub enum Button {
    /// Left mouse button (Button 1).
    Left,
    /// Right mouse button (Button 2).
    Right,
    /// Middle mouse button (Button 3).
    Middle,
    /// Extra button 1 (typically back).
    Button4,
    /// Extra button 2 (typically forward).
    Button5,
    /// Unknown or unsupported button.
    Unknown(u8),
}

impl Button {
    /// Get the button number (1-indexed).
    pub fn number(&self) -> u8 {
        match self {
            Button::Left => 1,
            Button::Right => 2,
            Button::Middle => 3,
            Button::Button4 => 4,
            Button::Button5 => 5,
            Button::Unknown(n) => *n,
        }
    }

    /// Create a Button from a number (1-indexed).
    pub fn from_number(n: u8) -> Self {
        match n {
            1 => Button::Left,
            2 => Button::Right,
            3 => Button::Middle,
            4 => Button::Button4,
            5 => Button::Button5,
            _ => Button::Unknown(n),
        }
    }
}

/// Scroll direction for mouse wheel events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub enum ScrollDirection {
    /// Scrolling up (away from user).
    Up,
    /// Scrolling down (toward user).
    Down,
    /// Scrolling left.
    Left,
    /// Scrolling right.
    Right,
}

/// Keyboard event data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub struct KeyboardData {
    /// The virtual key code.
    pub key: Key,
    /// The raw platform-specific keycode.
    pub raw_code: u32,
    /// The Unicode character, if this is a KeyTyped event.
    pub char: Option<char>,
}

/// Relative pointer motion associated with a mouse movement event.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub struct RelativeMotion {
    /// Horizontal movement delta.
    pub delta_x: f64,
    /// Vertical movement delta.
    pub delta_y: f64,
}

/// Mouse event data.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub struct MouseData {
    /// The mouse button (for press/release/click events).
    pub button: Option<Button>,
    /// X coordinate (screen coordinates).
    pub x: f64,
    /// Y coordinate (screen coordinates).
    pub y: f64,
    /// Click count (for click events).
    pub clicks: u8,
    /// Relative motion supplied by backends that can observe device deltas.
    #[cfg_attr(feature = "recorder", serde(default))]
    pub relative: Option<RelativeMotion>,
}

/// Mouse wheel event data.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub struct WheelData {
    /// X coordinate (screen coordinates).
    pub x: f64,
    /// Y coordinate (screen coordinates).
    pub y: f64,
    /// Scroll direction.
    pub direction: ScrollDirection,
    /// Amount of rotation (in platform-specific units).
    pub delta: f64,
}

/// Provenance information retained by the active platform input backend.
///
/// `Unknown` is deliberately not equivalent to physical input. A backend uses
/// this value whenever it cannot make a stronger, evidence-backed claim.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum InputOrigin {
    /// The backend cannot determine where this event originated.
    #[default]
    Unknown,
    /// The active backend has evidence that this event was synthesized.
    Injected {
        /// Best available identity for the injector.
        injector: InjectorIdentity,
    },
}

/// Identity retained for an injected input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum InjectorIdentity {
    /// The active backend recognizes this process session as the injector.
    ThisMonioSession,
}

/// A complete input event.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub struct Event {
    /// The type of event.
    pub event_type: EventType,
    /// Timestamp when the event occurred.
    pub time: SystemTime,
    /// Current modifier/button mask when event occurred.
    pub mask: u32,
    /// Best available evidence about where this event originated.
    #[cfg_attr(feature = "recorder", serde(default))]
    pub origin: InputOrigin,
    /// Keyboard-specific data.
    pub keyboard: Option<KeyboardData>,
    /// Mouse-specific data.
    pub mouse: Option<MouseData>,
    /// Wheel-specific data.
    pub wheel: Option<WheelData>,
}

impl Event {
    /// Create a new event with the given type and current timestamp.
    pub fn new(event_type: EventType) -> Self {
        Self {
            event_type,
            time: SystemTime::now(),
            mask: crate::state::get_mask(),
            origin: InputOrigin::Unknown,
            keyboard: None,
            mouse: None,
            wheel: None,
        }
    }

    /// Create a hook enabled event.
    pub fn hook_enabled() -> Self {
        Self::new(EventType::HookEnabled)
    }

    /// Create a hook disabled event.
    pub fn hook_disabled() -> Self {
        Self::new(EventType::HookDisabled)
    }

    /// Create a key pressed event.
    pub fn key_pressed(key: Key, raw_code: u32) -> Self {
        let mut event = Self::new(EventType::KeyPressed);
        event.keyboard = Some(KeyboardData {
            key,
            raw_code,
            char: None,
        });
        event
    }

    /// Create a key released event.
    pub fn key_released(key: Key, raw_code: u32) -> Self {
        let mut event = Self::new(EventType::KeyReleased);
        event.keyboard = Some(KeyboardData {
            key,
            raw_code,
            char: None,
        });
        event
    }

    /// Create a key typed event.
    pub fn key_typed(key: Key, raw_code: u32, char: char) -> Self {
        let mut event = Self::new(EventType::KeyTyped);
        event.keyboard = Some(KeyboardData {
            key,
            raw_code,
            char: Some(char),
        });
        event
    }

    /// Create a mouse pressed event.
    pub fn mouse_pressed(button: Button, x: f64, y: f64) -> Self {
        let mut event = Self::new(EventType::MousePressed);
        event.mouse = Some(MouseData {
            button: Some(button),
            x,
            y,
            clicks: 0,
            relative: None,
        });
        event
    }

    /// Create a mouse released event.
    pub fn mouse_released(button: Button, x: f64, y: f64) -> Self {
        let mut event = Self::new(EventType::MouseReleased);
        event.mouse = Some(MouseData {
            button: Some(button),
            x,
            y,
            clicks: 0,
            relative: None,
        });
        event
    }

    /// Create a mouse clicked event.
    pub fn mouse_clicked(button: Button, x: f64, y: f64, clicks: u8) -> Self {
        let mut event = Self::new(EventType::MouseClicked);
        event.mouse = Some(MouseData {
            button: Some(button),
            x,
            y,
            clicks,
            relative: None,
        });
        event
    }

    /// Create a mouse moved event.
    pub fn mouse_moved(x: f64, y: f64) -> Self {
        let mut event = Self::new(EventType::MouseMoved);
        event.mouse = Some(MouseData {
            button: None,
            x,
            y,
            clicks: 0,
            relative: None,
        });
        event
    }

    /// Create a mouse moved event with absolute position and relative motion.
    pub fn mouse_moved_relative(x: f64, y: f64, delta_x: f64, delta_y: f64) -> Self {
        let mut event = Self::mouse_moved(x, y);
        event
            .mouse
            .as_mut()
            .expect("mouse movement event should contain mouse data")
            .relative = Some(RelativeMotion { delta_x, delta_y });
        event
    }

    /// Create a mouse dragged event.
    pub fn mouse_dragged(x: f64, y: f64) -> Self {
        let mut event = Self::new(EventType::MouseDragged);
        event.mouse = Some(MouseData {
            button: None,
            x,
            y,
            clicks: 0,
            relative: None,
        });
        event
    }

    /// Create a mouse dragged event with absolute position and relative motion.
    pub fn mouse_dragged_relative(x: f64, y: f64, delta_x: f64, delta_y: f64) -> Self {
        let mut event = Self::mouse_dragged(x, y);
        event
            .mouse
            .as_mut()
            .expect("mouse drag event should contain mouse data")
            .relative = Some(RelativeMotion { delta_x, delta_y });
        event
    }

    /// Create a mouse wheel event.
    pub fn mouse_wheel(x: f64, y: f64, direction: ScrollDirection, delta: f64) -> Self {
        let mut event = Self::new(EventType::MouseWheel);
        event.wheel = Some(WheelData {
            x,
            y,
            direction,
            delta,
        });
        event
    }

    /// Check if this is a keyboard event.
    pub fn is_keyboard(&self) -> bool {
        matches!(
            self.event_type,
            EventType::KeyPressed | EventType::KeyReleased | EventType::KeyTyped
        )
    }

    /// Check if this is a mouse event.
    pub fn is_mouse(&self) -> bool {
        matches!(
            self.event_type,
            EventType::MousePressed
                | EventType::MouseReleased
                | EventType::MouseClicked
                | EventType::MouseMoved
                | EventType::MouseDragged
                | EventType::MouseWheel
        )
    }

    /// Whether this event was injected by the current Monio process session.
    pub fn is_from_this_monio_session(&self) -> bool {
        matches!(
            self.origin,
            InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_events_have_unknown_input_origin() {
        let event = Event::key_pressed(Key::KeyA, 0);

        assert_eq!(event.origin, InputOrigin::Unknown);
    }

    #[test]
    fn event_reports_injection_from_this_monio_session() {
        let mut event = Event::mouse_moved(100.0, 200.0);
        event.origin = InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        };

        assert!(event.is_from_this_monio_session());
    }

    #[test]
    fn absolute_mouse_motion_has_no_relative_delta() {
        let event = Event::mouse_moved(100.0, 200.0);

        assert_eq!(event.mouse.unwrap().relative, None);
    }

    #[test]
    fn relative_mouse_motion_keeps_absolute_position_and_delta() {
        let event = Event::mouse_moved_relative(100.0, 200.0, -3.5, 4.25);
        let mouse = event.mouse.unwrap();

        assert_eq!((mouse.x, mouse.y), (100.0, 200.0));
        assert_eq!(
            mouse.relative,
            Some(RelativeMotion {
                delta_x: -3.5,
                delta_y: 4.25,
            })
        );
    }

    #[test]
    fn relative_drag_uses_drag_event_type() {
        let event = Event::mouse_dragged_relative(10.0, 20.0, 1.0, -2.0);

        assert_eq!(event.event_type, EventType::MouseDragged);
        assert_eq!(
            event.mouse.unwrap().relative,
            Some(RelativeMotion {
                delta_x: 1.0,
                delta_y: -2.0,
            })
        );
    }

    #[cfg(feature = "recorder")]
    #[test]
    fn legacy_serialized_event_defaults_to_unknown_origin() {
        let event = Event::key_pressed(Key::KeyA, 0);
        let mut value = serde_json::to_value(event).expect("event should serialize");
        value
            .as_object_mut()
            .expect("event should serialize as an object")
            .remove("origin");

        let decoded: Event =
            serde_json::from_value(value).expect("legacy event should deserialize");

        assert_eq!(decoded.origin, InputOrigin::Unknown);
    }

    #[cfg(feature = "recorder")]
    #[test]
    fn legacy_serialized_mouse_event_defaults_to_no_relative_motion() {
        let event = Event::mouse_moved(10.0, 20.0);
        let mut value = serde_json::to_value(event).expect("event should serialize");
        value["mouse"]
            .as_object_mut()
            .expect("mouse should be an object")
            .remove("relative");

        let decoded: Event =
            serde_json::from_value(value).expect("legacy event should deserialize");

        assert_eq!(decoded.mouse.unwrap().relative, None);
    }
}
