//! Committing text, which is not the same thing as pressing keys.
//!
//! The rest of this crate's simulation surface injects key *positions*:
//! `key_press`, `key_release`, `key_tap`, resolved through a keycode table and
//! then interpreted by whatever layout the user has active. That is the right
//! model for Return, for ⌘C and for a game's WASD. It cannot express what a
//! soft keyboard produces.
//!
//! A phone keyboard has already run layout, an input method, prediction and
//! autocorrect before the application sees anything. Android hands over a
//! committed string through `InputConnection.commitText`, iOS through
//! `UIKeyInput.insertText`, a browser through an `input` event. There is no key
//! position for `好`: somebody typed `nihao`, tapped a candidate, and got two
//! characters that were never on any key. Swipe typing, emoji and paste have
//! the same shape.
//!
//! So text is a separate entry point, and stays one. The two authorize
//! differently — a caller allowed to press Return is not automatically one that
//! should be allowed to commit an arbitrary string — and they fail differently:
//! a wrong keycode types a wrong letter, a wrong commit pastes a wrong
//! sentence.
//!
//! # Platform support
//!
//! | Platform | State |
//! |---|---|
//! | macOS | implemented, `CGEventKeyboardSetUnicodeString` at the session tap |
//! | Windows | not implemented — needs `SendInput` with `KEYEVENTF_UNICODE`. Not `VkKeyScanExW`, which can only produce what the active layout can produce, so a US layout cannot type `é` let alone `好` |
//! | X11 | not implemented — needs libfakekey's remap-a-spare-keycode trick, or a reimplementation of it |
//! | Wayland | **unsolved.** libei has no text protocol. KDE Connect maps through `xkb_utf32_to_keysym` and gives up when there is none; GSConnect shells out to `wtype`/`ydotool`. After twelve years neither project can type Chinese into a Wayland desktop |
//! | HarmonyOS | not surveyed |
//!
//! Every unimplemented platform returns [`Error::NotSupported`] naming what it
//! would need. That is the point of this module existing on those platforms at
//! all: the previous behaviour was a `_ => {}` arm in each `simulate`, which
//! dropped the text and reported success.

use crate::error::Result;

/// Type a string into whatever currently has keyboard focus.
///
/// An empty string is a no-op and not an error, because callers forwarding
/// whatever a keyboard handed them will pass one.
///
/// Nothing acknowledges delivery. On macOS this posts CGEvents, which have no
/// return channel, so a caller that needs to know the text arrived has to
/// observe the target rather than this result. A missing Accessibility grant is
/// silent for the same reason.
///
/// ```no_run
/// # fn main() -> monio::Result<()> {
/// monio::type_text("你好 🌍")?;
/// # Ok(())
/// # }
/// ```
pub fn type_text(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        crate::platform::type_text(text)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(crate::error::Error::NotSupported(format!(
            "typing text is implemented for macOS only; {} needs {}",
            std::env::consts::OS,
            NEEDED
        )))
    }
}

#[cfg(not(target_os = "macos"))]
const NEEDED: &str = "SendInput with KEYEVENTF_UNICODE on Windows, libfakekey on X11, \
                      and has no known answer on Wayland — see this module's documentation";

#[cfg(test)]
mod tests {
    #[test]
    fn empty_is_a_no_op_on_every_platform() {
        // Including the ones where the real call is unimplemented: a caller
        // that forwards an empty commit should not see a platform error for it.
        assert!(super::type_text("").is_ok());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn an_unsupported_platform_says_so_instead_of_dropping_the_text() {
        let error = super::type_text("你好").expect_err("must not silently succeed");
        assert!(matches!(error, crate::error::Error::NotSupported(_)));
    }
}
