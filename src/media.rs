//! The keys a keyboard has above the number row.
//!
//! Separate from [`crate::keycode::Key`] because they are a different kind of
//! thing: a letter key is delivered to whatever has focus and means whatever
//! that application decides, while these are consumed by the system, which acts
//! on them itself and — this is the part that matters to a caller — *shows the
//! user that it did*.
//!
//! Injecting the key rather than setting the thing it controls is what buys the
//! on-screen overlay. On macOS, `osascript`'s `set volume` changes the level
//! silently; the same change made by posting the key draws the same overlay
//! F12 does. For a remote control that is not a nicety: the overlay is the only
//! acknowledgement a person standing away from the machine gets.
//!
//! # Platform support
//!
//! | Platform | State |
//! |---|---|
//! | macOS | implemented — `NSEventTypeSystemDefined` subtype 8 |
//! | Windows | not implemented — `SendInput` with `VK_VOLUME_UP`/`VK_MEDIA_*`, which is straightforward and simply not written yet |
//! | X11 | not implemented — `XF86AudioRaiseVolume` through XTest, subject to the desktop environment actually binding it |
//! | Wayland | not implemented — libei can send the key; whether anything shows an overlay is the compositor's business |
//! | HarmonyOS | not surveyed |

use crate::error::Result;

/// One of the system keys.
///
/// Deliberately not exhaustive of every `NX_KEYTYPE_*`: these are the ones with
/// a use for a remote control and a verified code. Adding another is a line in
/// the platform file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    VolumeUp,
    VolumeDown,
    /// Toggles. There is no separate mute-on and mute-off key, on any keyboard.
    Mute,
    BrightnessUp,
    BrightnessDown,
    PlayPause,
    Next,
    Previous,
}

/// Press and release one system key.
///
/// ```no_run
/// # fn main() -> monio::Result<()> {
/// monio::media_key(monio::MediaKey::VolumeUp)?;
/// # Ok(())
/// # }
/// ```
pub fn media_key(key: MediaKey) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        crate::platform::media_key(key)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = key;
        Err(crate::error::Error::NotSupported(format!(
            "media keys are implemented for macOS only; {} needs SendInput with \
             VK_VOLUME_* or the XF86Audio* keysyms — see this module's documentation",
            std::env::consts::OS,
        )))
    }
}
