//! HarmonyOS pointer, display, and system-property queries.

use super::result::platform_error;
use crate::display::{DisplayInfo, Rect, SystemSettings};
use crate::error::{Error, Result};
use ohos_display_sys::{
    NativeDisplayManager_DisplayInfo, NativeDisplayManager_DisplaysInfo,
    NativeDisplayManager_ErrorCode, NativeDisplayManager_ErrorCode_DISPLAY_MANAGER_OK,
    OH_NativeDisplayManager_CreateAllDisplays, OH_NativeDisplayManager_CreatePrimaryDisplay,
    OH_NativeDisplayManager_DestroyAllDisplays, OH_NativeDisplayManager_DestroyDisplay,
    OH_NativeDisplayManager_GetDefaultDisplayId, OH_NativeDisplayManager_GetDisplayPosition,
};
use ohos_input_sys::input_manager::OH_Input_GetPointerLocation;
use std::ptr;
use std::sync::Mutex;

static LAST_POINTER_POS: Mutex<Option<(f64, f64)>> = Mutex::new(None);

pub(super) fn remember_pointer_position(x: f64, y: f64) {
    if x.is_finite()
        && y.is_finite()
        && let Ok(mut guard) = LAST_POINTER_POS.lock()
    {
        *guard = Some((x, y));
    }
}

fn last_pointer_position() -> Option<(f64, f64)> {
    LAST_POINTER_POS.lock().ok().and_then(|guard| *guard)
}

/// Returns the live OS pointer location, falling back to the last observed or
/// injected pointer position when Input Kit reports no physical pointer device.
pub(super) fn effective_pointer_position() -> Result<(f64, f64)> {
    match mouse_position() {
        Ok(pos) => Ok(pos),
        Err(Error::NotSupported(_)) => {
            last_pointer_position().ok_or_else(|| {
                Error::NotSupported(
                    "OH_Input_GetPointerLocation reported no pointer device and no prior pointer position is known".into(),
                )
            })
        }
        Err(err) => Err(err),
    }
}

/// Returns a best-effort global pointer position for axis/scroll injection when
/// no physical pointer device is attached.
pub(super) fn scroll_pointer_position() -> (f64, f64) {
    if let Ok(pos) = effective_pointer_position() {
        return pos;
    }
    if let Ok(primary) = primary_display() {
        return (
            primary.bounds.x + primary.bounds.width / 2.0,
            primary.bounds.y + primary.bounds.height / 2.0,
        );
    }
    (0.0, 0.0)
}

struct AllDisplaysGuard(*mut NativeDisplayManager_DisplaysInfo);

impl Drop for AllDisplaysGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard owns the pointer returned by
            // `OH_NativeDisplayManager_CreateAllDisplays` and destroys it once.
            unsafe { OH_NativeDisplayManager_DestroyAllDisplays(self.0) };
            self.0 = ptr::null_mut();
        }
    }
}

struct PrimaryDisplayGuard(*mut NativeDisplayManager_DisplayInfo);

impl Drop for PrimaryDisplayGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this guard owns the pointer returned by
            // `OH_NativeDisplayManager_CreatePrimaryDisplay` and destroys it once.
            unsafe { OH_NativeDisplayManager_DestroyDisplay(self.0) };
            self.0 = ptr::null_mut();
        }
    }
}

fn display_manager_error(operation: &str, code: NativeDisplayManager_ErrorCode) -> Error {
    Error::Platform(format!(
        "{operation} failed with HarmonyOS Display Manager result {code}"
    ))
}

fn display_position(display_id: u32) -> (f64, f64) {
    let mut x = 0;
    let mut y = 0;
    // SAFETY: `x` and `y` are valid stack output pointers for the synchronous call.
    let rc = unsafe {
        OH_NativeDisplayManager_GetDisplayPosition(u64::from(display_id), &mut x, &mut y)
    };
    if rc == NativeDisplayManager_ErrorCode_DISPLAY_MANAGER_OK {
        (f64::from(x), f64::from(y))
    } else {
        (0.0, 0.0)
    }
}

fn convert_display_info(raw: &NativeDisplayManager_DisplayInfo, is_primary: bool) -> DisplayInfo {
    let (x, y) = display_position(raw.id);
    let scale_factor = if raw.densityPixels > 0.0 && raw.densityPixels.is_finite() {
        f64::from(raw.densityPixels)
    } else {
        1.0
    };
    let refresh_rate = (raw.refreshRate > 0).then_some(raw.refreshRate);

    DisplayInfo {
        id: raw.id,
        bounds: Rect {
            x,
            y,
            width: f64::from(raw.width.max(0)),
            height: f64::from(raw.height.max(0)),
        },
        scale_factor,
        refresh_rate,
        is_primary,
    }
}

pub fn mouse_position() -> Result<(f64, f64)> {
    let mut display_id = 0;
    let mut x = 0.0;
    let mut y = 0.0;

    // SAFETY: all output pointers refer to initialized stack values and remain
    // valid for this synchronous call.
    match unsafe { OH_Input_GetPointerLocation(&mut display_id, &mut x, &mut y) } {
        Ok(()) => {
            let known_displays = displays().unwrap_or_default();
            let (global_x, global_y) = super::translate::resolve_global_pointer_position(
                display_id,
                x,
                y,
                &known_displays,
            );
            remember_pointer_position(global_x, global_y);
            Ok((global_x, global_y))
        }
        Err(code) => Err(platform_error("OH_Input_GetPointerLocation", code.0.get())),
    }
}

pub fn displays() -> Result<Vec<DisplayInfo>> {
    let mut raw_all: *mut NativeDisplayManager_DisplaysInfo = ptr::null_mut();
    // SAFETY: `raw_all` is a valid out-pointer; the guard frees it on all paths.
    let rc = unsafe { OH_NativeDisplayManager_CreateAllDisplays(&mut raw_all) };
    if rc != NativeDisplayManager_ErrorCode_DISPLAY_MANAGER_OK {
        return Err(display_manager_error(
            "OH_NativeDisplayManager_CreateAllDisplays",
            rc,
        ));
    }
    let guard = AllDisplaysGuard(raw_all);
    if guard.0.is_null() {
        return Err(Error::Platform(
            "OH_NativeDisplayManager_CreateAllDisplays returned null".into(),
        ));
    }

    // SAFETY: `guard.0` is non-null and initialized by Display Manager.
    let list = unsafe { &*guard.0 };
    if list.displaysLength == 0 || list.displaysInfo.is_null() {
        return Ok(Vec::new());
    }

    let mut default_id = 0u64;
    // SAFETY: `default_id` is a valid out-pointer; failure just treats none as matched.
    let default_rc = unsafe { OH_NativeDisplayManager_GetDefaultDisplayId(&mut default_id) };
    let has_default = default_rc == NativeDisplayManager_ErrorCode_DISPLAY_MANAGER_OK;

    // SAFETY: `displaysInfo` points to `displaysLength` valid structs owned by `guard`.
    let raw_slice =
        unsafe { std::slice::from_raw_parts(list.displaysInfo, list.displaysLength as usize) };

    let mut result = Vec::with_capacity(raw_slice.len());
    for (idx, item) in raw_slice.iter().enumerate() {
        let is_primary = if has_default {
            u64::from(item.id) == default_id
        } else {
            idx == 0
        };
        result.push(convert_display_info(item, is_primary));
    }
    Ok(result)
}

pub fn primary_display() -> Result<DisplayInfo> {
    let mut raw_primary: *mut NativeDisplayManager_DisplayInfo = ptr::null_mut();
    // SAFETY: `raw_primary` is a valid out-pointer; the guard frees it on all paths.
    let rc = unsafe { OH_NativeDisplayManager_CreatePrimaryDisplay(&mut raw_primary) };
    if rc != NativeDisplayManager_ErrorCode_DISPLAY_MANAGER_OK {
        return Err(display_manager_error(
            "OH_NativeDisplayManager_CreatePrimaryDisplay",
            rc,
        ));
    }
    let guard = PrimaryDisplayGuard(raw_primary);
    if guard.0.is_null() {
        return Err(Error::Platform(
            "OH_NativeDisplayManager_CreatePrimaryDisplay returned null".into(),
        ));
    }

    // SAFETY: `guard.0` is non-null and initialized by Display Manager.
    let info = unsafe { &*guard.0 };
    Ok(convert_display_info(info, true))
}

pub fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>> {
    Ok(displays()?
        .into_iter()
        .find(|display| display.bounds.contains(x, y)))
}

pub fn system_settings() -> Result<SystemSettings> {
    Err(Error::NotSupported(
        "HarmonyOS Display/Input Kit does not expose system input settings".into(),
    ))
}
