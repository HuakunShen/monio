//! Windows platform implementation using SetWindowsHookEx.

mod display;
mod keycodes;
mod listen;
mod provenance;
mod simulate;

pub(crate) fn begin_relative_pointer_capture() -> crate::Result<()> {
    Err(crate::Error::NotSupported(
        "relative pointer capture requires a Windows Raw Input backend".into(),
    ))
}

pub(crate) fn end_relative_pointer_capture() -> crate::Result<()> {
    Ok(())
}

pub use display::{display_at_point, displays, primary_display, system_settings};
pub use listen::{run_grab_hook, run_hook, stop_hook};
pub use simulate::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_move_relative, mouse_position,
    mouse_press, mouse_release, simulate,
};
