//! Linux platform implementation.
//!
//! Supports two backends:
//! - **X11**: Uses XRecord for listening (default)
//! - **evdev**: Reads directly from /dev/input (works on X11 and Wayland)
//!
//! ## Feature Flags
//!
//! - `x11` (default): Use X11/XRecord for input capture
//! - `evdev`: Use evdev for input capture (works on Wayland)
//!
//! ## Permissions for evdev
//!
//! The evdev backend requires access to /dev/input devices:
//! ```bash
//! sudo usermod -aG input $USER
//! # Then log out and back in
//! ```
//!
//! ## Grab Mode on Wayland - IMPORTANT LIMITATION
//!
//! On **Wayland**, grab mode has a fundamental limitation due to how Wayland
//! compositors handle input via **libinput**:
//!
//! - ✅ **Blocking events works**: Events you consume (return `None`) are blocked
//! - ❌ **Pass-through fails**: Events you pass through (return `Some(event)`)
//!   may not reach other applications
//!
//! **Why:** Wayland compositors use libinput which takes exclusive access to
//! physical devices. When we grab via evdev, we intercept events before libinput
//! sees them. Re-injecting via uinput creates events from a virtual device that
//! libinput typically ignores for security.
//!
//! **Requirements for grab mode (even with limitations):**
//! - Membership in the `input` group
//! - Access to `/dev/uinput` (for re-injection attempts)
//!
//! ```bash
//! sudo usermod -aG input $USER
//! echo 'KERNEL=="uinput", GROUP="input", MODE="0660"' | sudo tee /etc/udev/rules.d/99-uinput.rules
//! sudo udevadm control --reload-rules
//! ```
//!
//! **Recommendation:** Use X11 instead of Wayland for full grab support, or use
//! grab only for consuming/blocking events rather than selective pass-through.

mod keycodes;

#[cfg(feature = "x11")]
mod x11;

#[cfg(feature = "evdev")]
mod evdev;

// Default to X11 if available
#[cfg(feature = "x11")]
pub use x11::*;

// Use evdev if X11 is not enabled but evdev is
#[cfg(all(feature = "evdev", not(feature = "x11")))]
pub use evdev::*;

// If neither X11 nor evdev features are enabled, provide stub implementations
#[cfg(not(any(feature = "x11", feature = "evdev")))]
mod stub {
    use crate::display::{DisplayInfo, SystemSettings};
    use crate::error::{Error, Result};
    use crate::event::{Button, Event};
    use crate::hook::{EventHandler, GrabHandler};
    use crate::keycode::Key;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    pub fn run_hook<H: EventHandler + 'static>(
        _running: &Arc<AtomicBool>,
        _handler: H,
    ) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn run_grab_hook<H: GrabHandler + 'static>(
        _running: &Arc<AtomicBool>,
        _handler: H,
    ) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn stop_hook() -> Result<()> {
        Ok(())
    }

    pub fn simulate(_event: &Event) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn key_press(_key: Key) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn key_release(_key: Key) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn key_tap(_key: Key) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn mouse_press(_button: Button) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn mouse_release(_button: Button) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn mouse_click(_button: Button) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn mouse_position() -> Result<(f64, f64)> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn mouse_move(_x: f64, _y: f64) -> Result<()> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn displays() -> Result<Vec<DisplayInfo>> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn primary_display() -> Result<DisplayInfo> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn display_at_point(_x: f64, _y: f64) -> Result<Option<DisplayInfo>> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }

    pub fn system_settings() -> Result<SystemSettings> {
        Err(Error::NotSupported(
            "No Linux backend enabled. Enable 'x11' or 'evdev' feature.".into(),
        ))
    }
}

#[cfg(not(any(feature = "x11", feature = "evdev")))]
pub use stub::*;
