//! Windows display and system property queries.

use crate::display::{DisplayInfo, Rect, SystemSettings};
use crate::error::{Error, Result};
use std::mem::{MaybeUninit, size_of};
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    ENUM_CURRENT_SETTINGS, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, HDC,
    HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayoutNameW;
use windows::Win32::UI::WindowsAndMessaging::{
    MONITORINFOF_PRIMARY, SPI_GETKEYBOARDDELAY, SPI_GETKEYBOARDSPEED, SPI_GETMOUSE,
    SPI_GETMOUSESPEED, SYSTEM_PARAMETERS_INFO_ACTION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::PCWSTR;

pub fn displays() -> Result<Vec<DisplayInfo>> {
    let mut context = MonitorContext {
        displays: Vec::new(),
        next_id: 1,
    };

    let ok = unsafe {
        EnumDisplayMonitors(
            Some(HDC(std::ptr::null_mut())),
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut context as *mut _ as isize),
        )
    };

    if ok.as_bool() && !context.displays.is_empty() {
        Ok(context.displays)
    } else {
        Err(Error::Platform("EnumDisplayMonitors failed".into()))
    }
}

pub fn primary_display() -> Result<DisplayInfo> {
    let displays = displays()?;
    displays
        .into_iter()
        .find(|display| display.is_primary)
        .ok_or_else(|| Error::Platform("primary display not found".into()))
}

pub fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>> {
    let displays = displays()?;
    Ok(displays
        .into_iter()
        .find(|display| display.bounds.contains(x, y)))
}

pub fn system_settings() -> Result<SystemSettings> {
    let keyboard_repeat_rate = system_param_u32(SPI_GETKEYBOARDSPEED);
    let keyboard_repeat_delay = system_param_u32(SPI_GETKEYBOARDDELAY);
    let mouse_sensitivity = system_param_u32(SPI_GETMOUSESPEED).map(|v| v as f64);
    let (mouse_acceleration_threshold, mouse_acceleration) = get_mouse_accel();
    let double_click_time = None; // GetDoubleClickTime not available in windows 0.59
    let keyboard_layout = get_keyboard_layout_name();

    Ok(SystemSettings {
        keyboard_repeat_rate,
        keyboard_repeat_delay,
        mouse_sensitivity,
        mouse_acceleration,
        mouse_acceleration_threshold,
        double_click_time,
        keyboard_layout,
    })
}

struct MonitorContext {
    displays: Vec<DisplayInfo>,
    next_id: u32,
}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprc: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let context = unsafe { &mut *(lparam.0 as *mut MonitorContext) };
    let info = monitor_info(hmonitor);
    let display = display_from_info(hmonitor, &info, context.next_id);
    context.next_id += 1;
    context.displays.push(display);
    BOOL(1)
}

fn monitor_info(hmonitor: HMONITOR) -> MONITORINFOEXW {
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let _ = GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut MONITORINFO);
    }
    info
}

fn display_from_info(hmonitor: HMONITOR, info: &MONITORINFOEXW, id: u32) -> DisplayInfo {
    let rect = info.monitorInfo.rcMonitor;
    let width = (rect.right - rect.left) as f64;
    let height = (rect.bottom - rect.top) as f64;
    let is_primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;

    let scale_factor = match monitor_dpi_scale(hmonitor) {
        Some(scale) => scale,
        None => 1.0,
    };

    let refresh_rate = monitor_refresh_rate(info);

    DisplayInfo {
        id,
        bounds: Rect {
            x: rect.left as f64,
            y: rect.top as f64,
            width,
            height,
        },
        scale_factor,
        refresh_rate,
        is_primary,
    }
}

fn monitor_dpi_scale(hmonitor: HMONITOR) -> Option<f64> {
    let mut dpi_x: u32 = 0;
    let mut dpi_y: u32 = 0;
    let result = unsafe { GetDpiForMonitor(hmonitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    if result.is_ok() && dpi_x > 0 {
        Some(dpi_x as f64 / 96.0)
    } else {
        let dpi = unsafe { GetDpiForSystem() };
        if dpi > 0 {
            Some(dpi as f64 / 96.0)
        } else {
            None
        }
    }
}

fn monitor_refresh_rate(info: &MONITORINFOEXW) -> Option<u32> {
    let mut devmode =
        unsafe { MaybeUninit::<windows::Win32::Graphics::Gdi::DEVMODEW>::zeroed().assume_init() };
    devmode.dmSize = size_of::<windows::Win32::Graphics::Gdi::DEVMODEW>() as u16;

    let device = PCWSTR(info.szDevice.as_ptr());
    let ok = unsafe { EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut devmode) };
    if ok.as_bool() && devmode.dmDisplayFrequency > 1 {
        Some(devmode.dmDisplayFrequency)
    } else {
        None
    }
}

fn system_param_u32(action: SYSTEM_PARAMETERS_INFO_ACTION) -> Option<u32> {
    let mut value: u32 = 0;
    let ok = unsafe { SystemParametersInfoW(action, 0, Some((&mut value as *mut u32).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    if ok.is_ok() { Some(value) } else { None }
}


fn get_mouse_accel() -> (Option<f64>, Option<f64>) {
    let mut mouse = [0i32; 3];
    let result = unsafe { SystemParametersInfoW(SPI_GETMOUSE, 0, Some(mouse.as_mut_ptr().cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    if result.is_ok() {
        let threshold = ((mouse[0] + mouse[1]) as f64) / 2.0;
        let speed = mouse[2] as f64;
        (Some(threshold), Some(speed))
    } else {
        (None, None)
    }
}

fn get_keyboard_layout_name() -> Option<String> {
    let mut buffer = [0u16; 9];
    let result = unsafe { GetKeyboardLayoutNameW(&mut buffer) };
    if result.is_ok() {
        let name = String::from_utf16_lossy(&buffer);
        Some(name.trim_end_matches('\0').to_string())
    } else {
        None
    }
}
