//! evdev backend has no display/system settings access.

use crate::display::{DisplayInfo, SystemSettings};
use crate::error::{Error, Result};

pub fn displays() -> Result<Vec<DisplayInfo>> {
    Err(Error::NotSupported(
        "Display information not available for evdev backend".into(),
    ))
}

pub fn primary_display() -> Result<DisplayInfo> {
    Err(Error::NotSupported(
        "Display information not available for evdev backend".into(),
    ))
}

pub fn display_at_point(_x: f64, _y: f64) -> Result<Option<DisplayInfo>> {
    Err(Error::NotSupported(
        "Display information not available for evdev backend".into(),
    ))
}

pub fn system_settings() -> Result<SystemSettings> {
    Err(Error::NotSupported(
        "System settings not available for evdev backend".into(),
    ))
}
