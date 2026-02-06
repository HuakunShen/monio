//! X11 implementation using XRecord.

mod display;
mod listen;
mod simulate;

pub use display::{display_at_point, displays, primary_display, system_settings};
pub use listen::{run_grab_hook, run_hook, stop_hook};
pub use simulate::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_position, mouse_press,
    mouse_release, simulate,
};
