//! Linux keycode to Key mappings.
//!
//! Supports both X11 keycodes and evdev keycodes.
//! - X11 keycodes: Used by XRecord (X11 keycodes = evdev + 8)
//! - evdev keycodes: Used by evdev backend (raw Linux input event codes)

#![allow(dead_code)]

use crate::keycode::Key;

// Conversion constant: X11 keycode = evdev keycode + 8
const X11_EVDEV_OFFSET: u32 = 8;

/// Convert an X11 keycode to our Key enum.
pub fn keycode_to_key(code: u32) -> Key {
    match code {
        // Letters (QWERTY layout)
        38 => Key::KeyA,
        56 => Key::KeyB,
        54 => Key::KeyC,
        40 => Key::KeyD,
        26 => Key::KeyE,
        41 => Key::KeyF,
        42 => Key::KeyG,
        43 => Key::KeyH,
        31 => Key::KeyI,
        44 => Key::KeyJ,
        45 => Key::KeyK,
        46 => Key::KeyL,
        58 => Key::KeyM,
        57 => Key::KeyN,
        32 => Key::KeyO,
        33 => Key::KeyP,
        24 => Key::KeyQ,
        27 => Key::KeyR,
        39 => Key::KeyS,
        28 => Key::KeyT,
        30 => Key::KeyU,
        55 => Key::KeyV,
        25 => Key::KeyW,
        53 => Key::KeyX,
        29 => Key::KeyY,
        52 => Key::KeyZ,

        // Numbers
        19 => Key::Num0,
        10 => Key::Num1,
        11 => Key::Num2,
        12 => Key::Num3,
        13 => Key::Num4,
        14 => Key::Num5,
        15 => Key::Num6,
        16 => Key::Num7,
        17 => Key::Num8,
        18 => Key::Num9,

        // Function keys
        67 => Key::F1,
        68 => Key::F2,
        69 => Key::F3,
        70 => Key::F4,
        71 => Key::F5,
        72 => Key::F6,
        73 => Key::F7,
        74 => Key::F8,
        75 => Key::F9,
        76 => Key::F10,
        95 => Key::F11,
        96 => Key::F12,

        // Modifiers
        50 => Key::ShiftLeft,
        62 => Key::ShiftRight,
        37 => Key::ControlLeft,
        105 => Key::ControlRight,
        64 => Key::AltLeft,
        108 => Key::AltRight,
        133 => Key::MetaLeft,
        134 => Key::MetaRight,

        // Navigation and special
        22 => Key::Backspace,
        23 => Key::Tab,
        36 => Key::Enter,
        66 => Key::CapsLock,
        9 => Key::Escape,
        65 => Key::Space,
        112 => Key::PageUp,
        117 => Key::PageDown,
        115 => Key::End,
        110 => Key::Home,
        113 => Key::ArrowLeft,
        111 => Key::ArrowUp,
        114 => Key::ArrowRight,
        116 => Key::ArrowDown,
        118 => Key::Insert,
        119 => Key::Delete,

        // Lock keys
        77 => Key::NumLock,
        78 => Key::ScrollLock,
        107 => Key::PrintScreen,
        127 => Key::Pause,

        // Punctuation
        49 => Key::Grave,
        20 => Key::Minus,
        21 => Key::Equal,
        34 => Key::BracketLeft,
        35 => Key::BracketRight,
        51 => Key::Backslash,
        47 => Key::Semicolon,
        48 => Key::Quote,
        59 => Key::Comma,
        60 => Key::Period,
        61 => Key::Slash,
        94 => Key::IntlBackslash,

        // Numpad
        90 => Key::Numpad0,
        87 => Key::Numpad1,
        88 => Key::Numpad2,
        89 => Key::Numpad3,
        83 => Key::Numpad4,
        84 => Key::Numpad5,
        85 => Key::Numpad6,
        79 => Key::Numpad7,
        80 => Key::Numpad8,
        81 => Key::Numpad9,
        63 => Key::NumpadMultiply,
        86 => Key::NumpadAdd,
        82 => Key::NumpadSubtract,
        91 => Key::NumpadDecimal,
        106 => Key::NumpadDivide,
        104 => Key::NumpadEnter,

        _ => Key::Unknown(code),
    }
}

/// Convert our Key enum to an X11 keycode.
pub fn key_to_keycode(key: Key) -> Option<u32> {
    Some(match key {
        // Letters
        Key::KeyA => 38,
        Key::KeyB => 56,
        Key::KeyC => 54,
        Key::KeyD => 40,
        Key::KeyE => 26,
        Key::KeyF => 41,
        Key::KeyG => 42,
        Key::KeyH => 43,
        Key::KeyI => 31,
        Key::KeyJ => 44,
        Key::KeyK => 45,
        Key::KeyL => 46,
        Key::KeyM => 58,
        Key::KeyN => 57,
        Key::KeyO => 32,
        Key::KeyP => 33,
        Key::KeyQ => 24,
        Key::KeyR => 27,
        Key::KeyS => 39,
        Key::KeyT => 28,
        Key::KeyU => 30,
        Key::KeyV => 55,
        Key::KeyW => 25,
        Key::KeyX => 53,
        Key::KeyY => 29,
        Key::KeyZ => 52,

        // Numbers
        Key::Num0 => 19,
        Key::Num1 => 10,
        Key::Num2 => 11,
        Key::Num3 => 12,
        Key::Num4 => 13,
        Key::Num5 => 14,
        Key::Num6 => 15,
        Key::Num7 => 16,
        Key::Num8 => 17,
        Key::Num9 => 18,

        // Function keys
        Key::F1 => 67,
        Key::F2 => 68,
        Key::F3 => 69,
        Key::F4 => 70,
        Key::F5 => 71,
        Key::F6 => 72,
        Key::F7 => 73,
        Key::F8 => 74,
        Key::F9 => 75,
        Key::F10 => 76,
        Key::F11 => 95,
        Key::F12 => 96,

        // Modifiers
        Key::ShiftLeft => 50,
        Key::ShiftRight => 62,
        Key::ControlLeft => 37,
        Key::ControlRight => 105,
        Key::AltLeft => 64,
        Key::AltRight => 108,
        Key::MetaLeft => 133,
        Key::MetaRight => 134,

        // Navigation and special
        Key::Backspace => 22,
        Key::Tab => 23,
        Key::Enter => 36,
        Key::CapsLock => 66,
        Key::Escape => 9,
        Key::Space => 65,
        Key::PageUp => 112,
        Key::PageDown => 117,
        Key::End => 115,
        Key::Home => 110,
        Key::ArrowLeft => 113,
        Key::ArrowUp => 111,
        Key::ArrowRight => 114,
        Key::ArrowDown => 116,
        Key::Insert => 118,
        Key::Delete => 119,

        // Lock keys
        Key::NumLock => 77,
        Key::ScrollLock => 78,
        Key::PrintScreen => 107,
        Key::Pause => 127,

        // Punctuation
        Key::Grave => 49,
        Key::Minus => 20,
        Key::Equal => 21,
        Key::BracketLeft => 34,
        Key::BracketRight => 35,
        Key::Backslash => 51,
        Key::Semicolon => 47,
        Key::Quote => 48,
        Key::Comma => 59,
        Key::Period => 60,
        Key::Slash => 61,
        Key::IntlBackslash => 94,

        // Numpad
        Key::Numpad0 => 90,
        Key::Numpad1 => 87,
        Key::Numpad2 => 88,
        Key::Numpad3 => 89,
        Key::Numpad4 => 83,
        Key::Numpad5 => 84,
        Key::Numpad6 => 85,
        Key::Numpad7 => 79,
        Key::Numpad8 => 80,
        Key::Numpad9 => 81,
        Key::NumpadMultiply => 63,
        Key::NumpadAdd => 86,
        Key::NumpadSubtract => 82,
        Key::NumpadDecimal => 91,
        Key::NumpadDivide => 106,
        Key::NumpadEnter => 104,

        Key::Unknown(code) => code,
        _ => return None,
    })
}

// ============================================================================
// evdev keycode conversions (for evdev backend)
// evdev keycodes are X11 keycodes - 8
// ============================================================================

/// Convert an evdev keycode to our Key enum.
#[cfg(feature = "evdev")]
pub fn evdev_keycode_to_key(code: u16) -> Key {
    // evdev keycodes are X11 keycodes - 8
    keycode_to_key((code as u32).wrapping_add(X11_EVDEV_OFFSET))
}

/// Convert our Key enum to an evdev keycode.
#[cfg(feature = "evdev")]
pub fn key_to_evdev_keycode(key: Key) -> u16 {
    key_to_keycode(key)
        .map(|x11_code| x11_code.wrapping_sub(X11_EVDEV_OFFSET) as u16)
        .unwrap_or(0)
}
