//! Virtual key code definitions.

#[cfg(feature = "recorder")]
use serde::{Deserialize, Serialize};

/// Virtual key codes for keyboard keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub enum Key {
    // Letters
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    // Numbers (top row)
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,

    // Modifiers
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    MetaLeft, // Windows/Command/Super
    MetaRight,

    // Navigation
    Escape,
    Tab,
    CapsLock,
    Space,
    Enter,
    Backspace,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Lock keys
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,

    // Punctuation and symbols
    Grave,        // ` ~
    Minus,        // - _
    Equal,        // = +
    BracketLeft,  // [ {
    BracketRight, // ] }
    Backslash,    // \ |
    Semicolon,    // ; :
    Quote,        // ' "
    Comma,        // , <
    Period,       // . >
    Slash,        // / ?

    // Numpad
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,
    NumpadEqual,

    // Media keys
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlayPause,
    MediaStop,
    MediaNext,
    MediaPrevious,

    // Browser keys
    BrowserBack,
    BrowserForward,
    BrowserRefresh,
    BrowserStop,
    BrowserSearch,
    BrowserFavorites,
    BrowserHome,

    // Application keys
    LaunchMail,
    LaunchApp1,
    LaunchApp2,

    // International / special
    IntlBackslash,
    IntlYen,
    IntlRo,

    // Context menu
    ContextMenu,

    // Unknown key with raw code
    Unknown(u32),
}

impl Key {
    /// Check if this is a modifier key.
    pub fn is_modifier(&self) -> bool {
        matches!(
            self,
            Key::ShiftLeft
                | Key::ShiftRight
                | Key::ControlLeft
                | Key::ControlRight
                | Key::AltLeft
                | Key::AltRight
                | Key::MetaLeft
                | Key::MetaRight
        )
    }

    /// Check if this is a letter key.
    pub fn is_letter(&self) -> bool {
        matches!(
            self,
            Key::KeyA
                | Key::KeyB
                | Key::KeyC
                | Key::KeyD
                | Key::KeyE
                | Key::KeyF
                | Key::KeyG
                | Key::KeyH
                | Key::KeyI
                | Key::KeyJ
                | Key::KeyK
                | Key::KeyL
                | Key::KeyM
                | Key::KeyN
                | Key::KeyO
                | Key::KeyP
                | Key::KeyQ
                | Key::KeyR
                | Key::KeyS
                | Key::KeyT
                | Key::KeyU
                | Key::KeyV
                | Key::KeyW
                | Key::KeyX
                | Key::KeyY
                | Key::KeyZ
        )
    }

    /// Check if this is a number key (top row).
    pub fn is_number(&self) -> bool {
        matches!(
            self,
            Key::Num0
                | Key::Num1
                | Key::Num2
                | Key::Num3
                | Key::Num4
                | Key::Num5
                | Key::Num6
                | Key::Num7
                | Key::Num8
                | Key::Num9
        )
    }

    /// Check if this is a function key.
    pub fn is_function_key(&self) -> bool {
        matches!(
            self,
            Key::F1
                | Key::F2
                | Key::F3
                | Key::F4
                | Key::F5
                | Key::F6
                | Key::F7
                | Key::F8
                | Key::F9
                | Key::F10
                | Key::F11
                | Key::F12
                | Key::F13
                | Key::F14
                | Key::F15
                | Key::F16
                | Key::F17
                | Key::F18
                | Key::F19
                | Key::F20
                | Key::F21
                | Key::F22
                | Key::F23
                | Key::F24
        )
    }

    /// Check if this is a numpad key.
    pub fn is_numpad(&self) -> bool {
        matches!(
            self,
            Key::Numpad0
                | Key::Numpad1
                | Key::Numpad2
                | Key::Numpad3
                | Key::Numpad4
                | Key::Numpad5
                | Key::Numpad6
                | Key::Numpad7
                | Key::Numpad8
                | Key::Numpad9
                | Key::NumpadAdd
                | Key::NumpadSubtract
                | Key::NumpadMultiply
                | Key::NumpadDivide
                | Key::NumpadDecimal
                | Key::NumpadEnter
                | Key::NumpadEqual
        )
    }

    /// Check if this is a media key.
    pub fn is_media(&self) -> bool {
        matches!(
            self,
            Key::VolumeUp
                | Key::VolumeDown
                | Key::VolumeMute
                | Key::MediaPlayPause
                | Key::MediaStop
                | Key::MediaNext
                | Key::MediaPrevious
        )
    }

    /// Check if this is a navigation key.
    pub fn is_navigation(&self) -> bool {
        matches!(
            self,
            Key::ArrowUp
                | Key::ArrowDown
                | Key::ArrowLeft
                | Key::ArrowRight
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
        )
    }
}

impl Default for Key {
    fn default() -> Self {
        Key::Unknown(0)
    }
}
