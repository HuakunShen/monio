use crate::event::Button;
use crate::keycode::Key;

const LETTERS: [Key; 26] = [
    Key::KeyA,
    Key::KeyB,
    Key::KeyC,
    Key::KeyD,
    Key::KeyE,
    Key::KeyF,
    Key::KeyG,
    Key::KeyH,
    Key::KeyI,
    Key::KeyJ,
    Key::KeyK,
    Key::KeyL,
    Key::KeyM,
    Key::KeyN,
    Key::KeyO,
    Key::KeyP,
    Key::KeyQ,
    Key::KeyR,
    Key::KeyS,
    Key::KeyT,
    Key::KeyU,
    Key::KeyV,
    Key::KeyW,
    Key::KeyX,
    Key::KeyY,
    Key::KeyZ,
];

const NUMBERS: [Key; 10] = [
    Key::Num0,
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

const FUNCTION_KEYS: [Key; 12] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
    Key::F11,
    Key::F12,
];

const NUMPAD_KEYS: [Key; 10] = [
    Key::Numpad0,
    Key::Numpad1,
    Key::Numpad2,
    Key::Numpad3,
    Key::Numpad4,
    Key::Numpad5,
    Key::Numpad6,
    Key::Numpad7,
    Key::Numpad8,
    Key::Numpad9,
];

pub(crate) fn keycode_to_key(code: i32) -> Key {
    match code {
        2000..=2009 => NUMBERS[(code - 2000) as usize],
        2012 => Key::ArrowUp,
        2013 => Key::ArrowDown,
        2014 => Key::ArrowLeft,
        2015 => Key::ArrowRight,
        2017..=2042 => LETTERS[(code - 2017) as usize],
        2043 => Key::Comma,
        2044 => Key::Period,
        2045 => Key::AltLeft,
        2046 => Key::AltRight,
        2047 => Key::ShiftLeft,
        2048 => Key::ShiftRight,
        2049 => Key::Tab,
        2050 => Key::Space,
        2052 => Key::BrowserHome,
        2053 => Key::LaunchMail,
        2054 => Key::Enter,
        2055 => Key::Backspace,
        2056 => Key::Grave,
        2057 => Key::Minus,
        2058 => Key::Equal,
        2059 => Key::BracketLeft,
        2060 => Key::BracketRight,
        2061 => Key::Backslash,
        2062 => Key::Semicolon,
        2063 => Key::Quote,
        2064 => Key::Slash,
        2067 => Key::ContextMenu,
        2068 => Key::PageUp,
        2069 => Key::PageDown,
        2070 => Key::Escape,
        2071 => Key::Delete,
        2072 => Key::ControlLeft,
        2073 => Key::ControlRight,
        2074 => Key::CapsLock,
        2075 => Key::ScrollLock,
        2076 => Key::MetaLeft,
        2077 => Key::MetaRight,
        2079 => Key::PrintScreen,
        2080 => Key::Pause,
        2081 => Key::Home,
        2082 => Key::End,
        2083 => Key::Insert,
        2084 => Key::BrowserForward,
        2085 | 2086 => Key::MediaPlayPause,
        2090..=2101 => FUNCTION_KEYS[(code - 2090) as usize],
        2102 => Key::NumLock,
        2103..=2112 => NUMPAD_KEYS[(code - 2103) as usize],
        2113 => Key::NumpadDivide,
        2114 => Key::NumpadMultiply,
        2115 => Key::NumpadSubtract,
        2116 => Key::NumpadAdd,
        2117 => Key::NumpadDecimal,
        2119 => Key::NumpadEnter,
        2120 => Key::NumpadEqual,
        1 => Key::BrowserHome,
        2 => Key::BrowserBack,
        9 => Key::BrowserSearch,
        10 => Key::MediaPlayPause,
        11 => Key::MediaStop,
        12 => Key::MediaNext,
        13 => Key::MediaPrevious,
        16 => Key::VolumeUp,
        17 => Key::VolumeDown,
        22 => Key::VolumeMute,
        _ => Key::Unknown(code as u32),
    }
}

pub(crate) fn key_to_keycode(key: Key) -> Option<i32> {
    if let Some(index) = LETTERS.iter().position(|candidate| *candidate == key) {
        return Some(2017 + index as i32);
    }
    if let Some(index) = NUMBERS.iter().position(|candidate| *candidate == key) {
        return Some(2000 + index as i32);
    }
    if let Some(index) = FUNCTION_KEYS.iter().position(|candidate| *candidate == key) {
        return Some(2090 + index as i32);
    }
    if let Some(index) = NUMPAD_KEYS.iter().position(|candidate| *candidate == key) {
        return Some(2103 + index as i32);
    }

    match key {
        Key::ArrowUp => Some(2012),
        Key::ArrowDown => Some(2013),
        Key::ArrowLeft => Some(2014),
        Key::ArrowRight => Some(2015),
        Key::Comma => Some(2043),
        Key::Period => Some(2044),
        Key::AltLeft => Some(2045),
        Key::AltRight => Some(2046),
        Key::ShiftLeft => Some(2047),
        Key::ShiftRight => Some(2048),
        Key::Tab => Some(2049),
        Key::Space => Some(2050),
        Key::BrowserHome => Some(2052),
        Key::LaunchMail => Some(2053),
        Key::Enter => Some(2054),
        Key::Backspace => Some(2055),
        Key::Grave => Some(2056),
        Key::Minus => Some(2057),
        Key::Equal => Some(2058),
        Key::BracketLeft => Some(2059),
        Key::BracketRight => Some(2060),
        Key::Backslash => Some(2061),
        Key::Semicolon => Some(2062),
        Key::Quote => Some(2063),
        Key::Slash => Some(2064),
        Key::ContextMenu => Some(2067),
        Key::PageUp => Some(2068),
        Key::PageDown => Some(2069),
        Key::Escape => Some(2070),
        Key::Delete => Some(2071),
        Key::ControlLeft => Some(2072),
        Key::ControlRight => Some(2073),
        Key::CapsLock => Some(2074),
        Key::ScrollLock => Some(2075),
        Key::MetaLeft => Some(2076),
        Key::MetaRight => Some(2077),
        Key::PrintScreen => Some(2079),
        Key::Pause => Some(2080),
        Key::Home => Some(2081),
        Key::End => Some(2082),
        Key::Insert => Some(2083),
        Key::BrowserForward => Some(2084),
        Key::NumLock => Some(2102),
        Key::NumpadDivide => Some(2113),
        Key::NumpadMultiply => Some(2114),
        Key::NumpadSubtract => Some(2115),
        Key::NumpadAdd => Some(2116),
        Key::NumpadDecimal => Some(2117),
        Key::NumpadEnter => Some(2119),
        Key::NumpadEqual => Some(2120),
        Key::BrowserBack => Some(2),
        Key::BrowserSearch => Some(9),
        Key::MediaPlayPause => Some(10),
        Key::MediaStop => Some(11),
        Key::MediaNext => Some(12),
        Key::MediaPrevious => Some(13),
        Key::VolumeUp => Some(16),
        Key::VolumeDown => Some(17),
        Key::VolumeMute => Some(22),
        Key::Unknown(code) => i32::try_from(code).ok(),
        _ => None,
    }
}

pub(crate) fn button_from_native(button: i32) -> Button {
    use super::constants::{
        MOUSE_BUTTON_BACK, MOUSE_BUTTON_FORWARD, MOUSE_BUTTON_LEFT, MOUSE_BUTTON_MIDDLE,
        MOUSE_BUTTON_RIGHT,
    };

    match button {
        MOUSE_BUTTON_LEFT => Button::Left,
        MOUSE_BUTTON_MIDDLE => Button::Middle,
        MOUSE_BUTTON_RIGHT => Button::Right,
        MOUSE_BUTTON_FORWARD => Button::Button5,
        MOUSE_BUTTON_BACK => Button::Button4,
        value => Button::Unknown(u8::try_from(value).unwrap_or(u8::MAX)),
    }
}

pub(crate) fn button_to_native(button: Button) -> Option<i32> {
    use super::constants::{
        MOUSE_BUTTON_BACK, MOUSE_BUTTON_FORWARD, MOUSE_BUTTON_LEFT, MOUSE_BUTTON_MIDDLE,
        MOUSE_BUTTON_RIGHT,
    };

    match button {
        Button::Left => Some(MOUSE_BUTTON_LEFT),
        Button::Middle => Some(MOUSE_BUTTON_MIDDLE),
        Button::Right => Some(MOUSE_BUTTON_RIGHT),
        Button::Button4 => Some(MOUSE_BUTTON_BACK),
        Button::Button5 => Some(MOUSE_BUTTON_FORWARD),
        Button::Unknown(value) => Some(i32::from(value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_keycodes_map_in_both_directions() {
        let pairs = [
            (2017, Key::KeyA),
            (2042, Key::KeyZ),
            (2000, Key::Num0),
            (2009, Key::Num9),
            (2090, Key::F1),
            (2101, Key::F12),
            (2047, Key::ShiftLeft),
            (2073, Key::ControlRight),
            (2070, Key::Escape),
            (2081, Key::Home),
            (2056, Key::Grave),
            (2116, Key::NumpadAdd),
            (16, Key::VolumeUp),
            (10, Key::MediaPlayPause),
            (2084, Key::BrowserForward),
            (2053, Key::LaunchMail),
            (2067, Key::ContextMenu),
        ];

        for (code, key) in pairs {
            assert_eq!(keycode_to_key(code), key);
            assert_eq!(key_to_keycode(key), Some(code));
        }
    }

    #[test]
    fn unknown_and_unsupported_keys_are_explicit() {
        assert_eq!(keycode_to_key(99_999), Key::Unknown(99_999));
        assert_eq!(key_to_keycode(Key::Unknown(99_999)), Some(99_999));
        assert_eq!(key_to_keycode(Key::F13), None);
        assert_eq!(key_to_keycode(Key::IntlYen), None);
        assert_eq!(key_to_keycode(Key::LaunchApp1), None);
    }

    #[test]
    fn native_mouse_buttons_map_in_both_directions() {
        let pairs = [
            (0, crate::event::Button::Left),
            (1, crate::event::Button::Middle),
            (2, crate::event::Button::Right),
            (4, crate::event::Button::Button4),
            (3, crate::event::Button::Button5),
        ];

        for (code, button) in pairs {
            assert_eq!(button_from_native(code), button);
            assert_eq!(button_to_native(button), Some(code));
        }

        assert_eq!(
            button_from_native(42),
            crate::event::Button::Unknown(42)
        );
        assert_eq!(
            button_to_native(crate::event::Button::Unknown(42)),
            Some(42)
        );
    }
}
