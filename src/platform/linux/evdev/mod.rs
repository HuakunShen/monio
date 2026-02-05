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

mod display;
mod listen;
mod simulate;

pub use display::{display_at_point, displays, primary_display, system_settings};
pub use listen::{run_grab_hook, run_hook, stop_hook};
pub use simulate::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_press, mouse_release, simulate,
};
