//! X11 display and system property queries.

use crate::display::{DisplayInfo, Rect, SystemSettings};
use crate::error::{Error, Result};
use std::ptr::null;
use x11::xlib;

pub fn displays() -> Result<Vec<DisplayInfo>> {
    with_display(|display| unsafe {
        let screen = xlib::XDefaultScreen(display);
        let width = xlib::XDisplayWidth(display, screen) as f64;
        let height = xlib::XDisplayHeight(display, screen) as f64;

        Ok(vec![DisplayInfo {
            id: 1,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            scale_factor: 1.0,
            refresh_rate: None,
            is_primary: true,
        }])
    })
}

pub fn primary_display() -> Result<DisplayInfo> {
    displays()?
        .into_iter()
        .next()
        .ok_or_else(|| Error::Platform("X11 display information unavailable".into()))
}

pub fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>> {
    let displays = displays()?;
    Ok(displays
        .into_iter()
        .find(|display| display.bounds.contains(x, y)))
}

pub fn system_settings() -> Result<SystemSettings> {
    let (mouse_sensitivity, mouse_acceleration, mouse_acceleration_threshold) =
        with_display(|display| unsafe {
            let mut accel_numerator: i32 = 0;
            let mut accel_denominator: i32 = 0;
            let mut threshold: i32 = 0;
            xlib::XGetPointerControl(
                display,
                &mut accel_numerator,
                &mut accel_denominator,
                &mut threshold,
            );

            Ok((
                Some(accel_numerator as f64),
                Some(accel_denominator as f64),
                Some(threshold as f64),
            ))
        })?;

    Ok(SystemSettings {
        keyboard_repeat_rate: None,
        keyboard_repeat_delay: None,
        mouse_sensitivity,
        mouse_acceleration,
        mouse_acceleration_threshold,
        double_click_time: None,
        keyboard_layout: None,
    })
}

fn with_display<T>(f: impl FnOnce(*mut xlib::Display) -> Result<T>) -> Result<T> {
    unsafe {
        let display = xlib::XOpenDisplay(null());
        if display.is_null() {
            return Err(Error::Platform("XOpenDisplay failed".into()));
        }
        let result = f(display);
        xlib::XCloseDisplay(display);
        result
    }
}
