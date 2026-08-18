//! macOS platform implementation using CGEventTap.

pub mod accessibility;
mod display;
mod gesture;
mod keycodes;
mod listen;
mod media;
mod pointer_capture;
mod provenance;
mod scroll;
mod simulate;
mod text;

pub use display::{display_at_point, displays, primary_display, system_settings};
pub use gesture::{magnify, rotate, smart_magnify};
pub use listen::{run_grab_hook, run_hook, stop_hook};
pub use media::media_key;
pub(crate) use pointer_capture::{begin_relative_pointer_capture, end_relative_pointer_capture};
pub use scroll::scroll;
pub use simulate::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_move_relative, mouse_position,
    mouse_press, mouse_release, simulate,
};
pub use text::type_text;
