//! macOS platform implementation using CGEventTap.

pub mod accessibility;
mod display;
mod keycodes;
mod listen;
mod pointer_capture;
mod provenance;
mod simulate;

pub use display::{display_at_point, displays, primary_display, system_settings};
pub use listen::{run_grab_hook, run_hook, stop_hook};
pub(crate) use pointer_capture::{begin_relative_pointer_capture, end_relative_pointer_capture};
pub use simulate::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_move_relative, mouse_position,
    mouse_press, mouse_release, simulate,
};
