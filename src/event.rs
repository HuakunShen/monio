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
        });
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
        });
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
}
