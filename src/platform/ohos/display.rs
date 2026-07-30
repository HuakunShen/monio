//! HarmonyOS display and system-property queries.

use crate::display::{DisplayInfo, SystemSettings};
use crate::error::{Error, Result};

fn unsupported(query: &str) -> Error {
    Error::NotSupported(format!("HarmonyOS {query} is not implemented"))
}

pub fn displays() -> Result<Vec<DisplayInfo>> {
    Err(unsupported("display enumeration"))
}

pub fn primary_display() -> Result<DisplayInfo> {
    Err(unsupported("primary display query"))
}

pub fn display_at_point(_x: f64, _y: f64) -> Result<Option<DisplayInfo>> {
    Err(unsupported("display-at-point query"))
}

pub fn system_settings() -> Result<SystemSettings> {
    Err(unsupported("system settings query"))
}
