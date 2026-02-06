//! macOS display and system property queries.

use crate::display::{DisplayInfo, Rect, SystemSettings};
use crate::error::{Error, Result};
use objc2_core_foundation::{
    CFNumber, CFNumberType, CFPreferencesCopyValue, CFString, kCFPreferencesAnyApplication,
    kCFPreferencesAnyHost, kCFPreferencesCurrentUser,
};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode,
    CGDisplayPixelsWide, CGError, CGGetActiveDisplayList, CGMainDisplayID,
};

fn display_info(display_id: CGDirectDisplayID, index: u32) -> DisplayInfo {
    let bounds = CGDisplayBounds(display_id);
    let width_points = bounds.size.width as f64;
    let height_points = bounds.size.height as f64;
    let width_pixels = CGDisplayPixelsWide(display_id) as f64;
    let scale_factor = if width_points > 0.0 {
        width_pixels / width_points
    } else {
        1.0
    };

    let refresh_rate = CGDisplayCopyDisplayMode(display_id)
        .map(|mode| {
            let rate = CGDisplayMode::refresh_rate(Some(&mode));
            if rate > 0.0 {
                Some(rate.round() as u32)
            } else {
                None
            }
        })
        .unwrap_or(None);

    DisplayInfo {
        id: index,
        bounds: Rect {
            x: bounds.origin.x as f64,
            y: bounds.origin.y as f64,
            width: width_points,
            height: height_points,
        },
        scale_factor,
        refresh_rate,
        is_primary: display_id == CGMainDisplayID(),
    }
}

pub fn displays() -> Result<Vec<DisplayInfo>> {
    let mut max_displays = 8usize;
    loop {
        let mut displays = vec![0; max_displays];
        let mut count: u32 = 0;
        let status = unsafe {
            CGGetActiveDisplayList(max_displays as u32, displays.as_mut_ptr(), &mut count)
        };
        if status != CGError::Success {
            return Err(Error::Platform(format!(
                "CGGetActiveDisplayList failed: {:?}",
                status
            )));
        }

        if (count as usize) <= max_displays {
            displays.truncate(count as usize);
            return Ok(displays
                .into_iter()
                .enumerate()
                .map(|(idx, display_id)| display_info(display_id, (idx + 1) as u32))
                .collect());
        }

        max_displays = count as usize;
    }
}

pub fn primary_display() -> Result<DisplayInfo> {
    Ok(display_info(CGMainDisplayID(), 1))
}

pub fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>> {
    let all = displays()?;
    Ok(all
        .into_iter()
        .find(|display| display.bounds.contains(x, y)))
}

pub fn system_settings() -> Result<SystemSettings> {
    let keyboard_repeat_rate = pref_number_i64("KeyRepeat").map(|value| (value * 15) as u32);
    let keyboard_repeat_delay =
        pref_number_i64("InitialKeyRepeat").map(|value| (value * 15) as u32);

    let mouse_sensitivity = pref_number_f64("com.apple.mouse.scaling");
    let double_click_time = pref_number_f64("com.apple.mouse.doubleClickThreshold")
        .map(|seconds| (seconds * 1000.0) as u32);

    Ok(SystemSettings {
        keyboard_repeat_rate,
        keyboard_repeat_delay,
        mouse_sensitivity,
        mouse_acceleration: None,
        mouse_acceleration_threshold: None,
        double_click_time,
        keyboard_layout: None,
    })
}

fn pref_number_i64(key: &str) -> Option<i64> {
    let key = CFString::from_str(key);
    let value = unsafe {
        CFPreferencesCopyValue(
            &key,
            kCFPreferencesAnyApplication,
            kCFPreferencesCurrentUser,
            kCFPreferencesAnyHost,
        )
    }?;
    let number = value.downcast::<CFNumber>().ok()?;
    let mut out: i64 = 0;
    // SAFETY: `out` is a valid i64 and the pointer is properly aligned
    let ok = unsafe { number.value(CFNumberType::SInt64Type, &mut out as *mut _ as *mut _) };
    if ok { Some(out) } else { None }
}

fn pref_number_f64(key: &str) -> Option<f64> {
    let key = CFString::from_str(key);
    let value = unsafe {
        CFPreferencesCopyValue(
            &key,
            kCFPreferencesAnyApplication,
            kCFPreferencesCurrentUser,
            kCFPreferencesAnyHost,
        )
    }?;
    let number = value.downcast::<CFNumber>().ok()?;
    let mut out: f64 = 0.0;
    // SAFETY: `out` is a valid f64 and the pointer is properly aligned
    let ok = unsafe { number.value(CFNumberType::Float64Type, &mut out as *mut _ as *mut _) };
    if ok { Some(out) } else { None }
}
