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
//! ## Wayland Grab Compatibility
//!
//! On **Wayland**, `run_grab_hook` pass-through behavior depends on the
//! compositor and libinput environment:
//!
//! - Events you **consume** (return `None` from the handler) are blocked through
//!   the kernel's exclusive evdev grab.
//! - Events you **pass through** (return `Some(event)`) are re-injected through
//!   Monio's uinput virtual device.
//!
//! Grabbing intercepts events before the compositor sees them. Pass-through
//! requires re-injection through a uinput virtual device, and compositor policy
//! for those events varies. GNOME 46 with libinput 1.25 has been natively
//! verified for selective keyboard blocking and keyboard, click, motion, and
//! drag pass-through; validate other compositors separately.
//!
//! For an unprivileged desktop product, prefer compositor-mediated portal/libei
//! APIs when available.

#![allow(unused_imports)]

mod display;
mod listen;
mod provenance;
mod simulate;

pub use display::{display_at_point, displays, primary_display, system_settings};
pub use listen::{run_grab_hook, run_hook, stop_hook};
pub use simulate::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_move_relative, mouse_position,
    mouse_press, mouse_release, simulate,
};
