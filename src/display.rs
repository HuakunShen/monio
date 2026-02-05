//! Display and system property queries.

use crate::error::Result;

/// A rectangle in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left coordinate.
    pub x: f64,
    /// Top coordinate.
    pub y: f64,
    /// Width in screen points.
    pub width: f64,
    /// Height in screen points.
    pub height: f64,
}

impl Rect {
    /// Check whether a point is inside this rectangle.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

/// Information about a display/monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInfo {
    /// Platform-specific identifier (best-effort).
    pub id: u32,
    /// Display bounds in screen coordinates.
    pub bounds: Rect,
    /// Scale factor relative to 1.0 (96 DPI on Windows, 1x on macOS).
    pub scale_factor: f64,
    /// Refresh rate in Hz, if available.
    pub refresh_rate: Option<u32>,
    /// Whether this is the primary display.
    pub is_primary: bool,
}

/// System input settings (platform-specific units where noted).
#[derive(Debug, Clone, PartialEq)]
pub struct SystemSettings {
    /// Keyboard repeat rate (platform-specific units).
    pub keyboard_repeat_rate: Option<u32>,
    /// Keyboard repeat delay (milliseconds when available).
    pub keyboard_repeat_delay: Option<u32>,
    /// Mouse sensitivity/speed (platform-specific units).
    pub mouse_sensitivity: Option<f64>,
    /// Mouse acceleration (platform-specific units).
    pub mouse_acceleration: Option<f64>,
    /// Mouse acceleration threshold (platform-specific units).
    pub mouse_acceleration_threshold: Option<f64>,
    /// Double-click time in milliseconds.
    pub double_click_time: Option<u32>,
    /// Current keyboard layout identifier (best-effort).
    pub keyboard_layout: Option<String>,
}

/// List all available displays.
pub fn displays() -> Result<Vec<DisplayInfo>> {
    crate::platform::displays()
}

/// Get the primary display.
pub fn primary_display() -> Result<DisplayInfo> {
    crate::platform::primary_display()
}

/// Find the display containing a point (screen coordinates).
pub fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>> {
    crate::platform::display_at_point(x, y)
}

/// Query system input settings (best-effort, per platform).
pub fn system_settings() -> Result<SystemSettings> {
    crate::platform::system_settings()
}
