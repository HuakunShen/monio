//! HarmonyOS pointer, display, and system-property queries.

use super::result::platform_error;
use crate::display::{DisplayInfo, SystemSettings};
use crate::error::{Error, Result};
use ohos_input_sys::input_manager::OH_Input_GetPointerLocation;

fn unsupported(query: &str) -> Error {
    Error::NotSupported(format!(
        "HarmonyOS Input Kit does not expose this query: {query}"
    ))
}

pub fn mouse_position() -> Result<(f64, f64)> {
    let mut display_id = 0;
    let mut x = 0.0;
    let mut y = 0.0;

    // SAFETY: all output pointers refer to initialized stack values and remain
    // valid for this synchronous call.
    match unsafe { OH_Input_GetPointerLocation(&mut display_id, &mut x, &mut y) } {
        Ok(()) => Ok((x, y)),
        Err(code) => Err(platform_error("OH_Input_GetPointerLocation", code.0.get())),
    }
}

pub fn displays() -> Result<Vec<DisplayInfo>> {
    Err(unsupported("display enumeration"))
}

pub fn primary_display() -> Result<DisplayInfo> {
    Err(unsupported("primary display"))
}

pub fn display_at_point(_x: f64, _y: f64) -> Result<Option<DisplayInfo>> {
    Err(unsupported("display at point"))
}

pub fn system_settings() -> Result<SystemSettings> {
    Err(unsupported("system input settings"))
}
