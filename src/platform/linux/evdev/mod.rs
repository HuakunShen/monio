//! Linux evdev implementation.
//!
//! This implementation uses evdev to read input events directly from
//! `/dev/input/event*` devices. This works on both X11 and Wayland.
//!
//! ## Permissions
//!
//! To access input devices, the process must either:
//! - Run as root (not recommended)
//! - Run as a user in the `input` group (recommended)
//!
//! To add yourself to the input group:
//! ```bash
//! sudo usermod -aG input $USER
//! # Then log out and back in
//! ```
//!
//! ## Wayland Grab Limitation
//!
//! On **Wayland**, `run_grab_hook` (grab mode) has a known limitation:
//!
//! - ✅ Events you **consume** (return `None` from handler) are properly blocked
//! - ❌ Events you **pass through** (return `Some(event)`) may not reach applications
//!
//! This happens because Wayland compositors use libinput which ignores events
//! from virtual devices (uinput) for security. When we grab the physical devices
//! via evdev, libinput loses access to them, and our re-injected events are
//! not recognized by the compositor.
//!
//! For selective event filtering on Wayland, consider using your compositor's
//! native hotkey/configuration system instead of this library.

#![allow(unused_imports)]

mod display;
mod listen;
mod simulate;

pub use display::{display_at_point, displays, primary_display, system_settings};
pub use listen::{run_grab_hook, run_hook, stop_hook};
pub use simulate::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_press, mouse_release, simulate,
};
