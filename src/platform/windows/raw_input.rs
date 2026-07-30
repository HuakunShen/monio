use crate::{Error, Event, Result};
use std::ffi::c_void;
use std::mem::{MaybeUninit, size_of};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Input::{
    GetCurrentInputMessageSource, GetRawInputData, GetRegisteredRawInputDevices, HRAWINPUT,
    IMO_INJECTED, INPUT_MESSAGE_SOURCE, MOUSE_MOVE_ABSOLUTE, MOUSE_VIRTUAL_DESKTOP, RAWINPUT,
    RAWINPUTDEVICE, RAWINPUTHEADER, RAWMOUSE, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE,
    RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, HWND_MESSAGE, MSG, PM_REMOVE, PeekMessageW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT,
};
use windows::core::w;

const GENERIC_DESKTOP_PAGE: u16 = 0x01;
const GENERIC_DESKTOP_MOUSE: u16 = 0x02;
const RAW_INPUT_ERROR: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawMouseMotion {
    Relative {
        delta_x: i32,
        delta_y: i32,
    },
    Absolute {
        normalized_x: i32,
        normalized_y: i32,
        virtual_desktop: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DesktopBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

fn existing_mouse_registration(devices: &[RAWINPUTDEVICE]) -> Option<RAWINPUTDEVICE> {
    devices.iter().copied().find(|device| {
        device.usUsagePage == GENERIC_DESKTOP_PAGE && device.usUsage == GENERIC_DESKTOP_MOUSE
    })
}

fn registration_is_owned_by(registration: Option<RAWINPUTDEVICE>, window: HWND) -> bool {
    registration.is_some_and(|device| {
        device.usUsagePage == GENERIC_DESKTOP_PAGE
            && device.usUsage == GENERIC_DESKTOP_MOUSE
            && device.hwndTarget == window
    })
}

fn registered_devices() -> Result<Vec<RAWINPUTDEVICE>> {
    loop {
        let mut required = 0u32;
        let query = unsafe {
            GetRegisteredRawInputDevices(None, &mut required, size_of::<RAWINPUTDEVICE>() as u32)
        };
        if query == RAW_INPUT_ERROR {
            return Err(Error::HookStartFailed(format!(
                "Failed to query Windows Raw Input registrations: {}",
                windows::core::Error::from_win32()
            )));
        }
        if required == 0 {
            return Ok(Vec::new());
        }

        let mut devices = vec![RAWINPUTDEVICE::default(); required as usize];
        let mut capacity = required;
        let copied = unsafe {
            GetRegisteredRawInputDevices(
                Some(devices.as_mut_ptr()),
                &mut capacity,
                size_of::<RAWINPUTDEVICE>() as u32,
            )
        };
        if copied == RAW_INPUT_ERROR {
            if capacity > required {
                continue;
            }
            return Err(Error::HookStartFailed(format!(
                "Failed to read Windows Raw Input registrations: {}",
                windows::core::Error::from_win32()
            )));
        }

        devices.truncate(copied as usize);
        return Ok(devices);
    }
}

pub(super) struct RawMouseInput {
    window: HWND,
    previous_registration: Option<RAWINPUTDEVICE>,
    registered: bool,
}

impl RawMouseInput {
    pub(super) fn acquire() -> Result<Self> {
        let previous_registration = existing_mouse_registration(&registered_devices()?);
        let window = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("monio raw mouse input"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .map_err(|error| {
            Error::HookStartFailed(format!(
                "Failed to create Windows Raw Input receiver: {error}"
            ))
        })?;

        let registration = RAWINPUTDEVICE {
            usUsagePage: GENERIC_DESKTOP_PAGE,
            usUsage: GENERIC_DESKTOP_MOUSE,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: window,
        };
        if let Err(error) =
            unsafe { RegisterRawInputDevices(&[registration], size_of::<RAWINPUTDEVICE>() as u32) }
        {
            let _ = unsafe { DestroyWindow(window) };
            return Err(Error::HookStartFailed(format!(
                "Failed to register Windows Raw Input mouse: {error}"
            )));
        }

        Ok(Self {
            window,
            previous_registration,
            registered: true,
        })
    }

    pub(super) fn window(&self) -> HWND {
        self.window
    }

    pub(super) fn read(&self, lparam: LPARAM) -> Result<Option<RawMouseMotion>> {
        let mut source = INPUT_MESSAGE_SOURCE::default();
        if unsafe { GetCurrentInputMessageSource(&mut source) }.is_ok()
            && source.originId == IMO_INJECTED
        {
            return Ok(None);
        }

        let mut raw = MaybeUninit::<RAWINPUT>::zeroed();
        let mut size = size_of::<RAWINPUT>() as u32;
        let copied = unsafe {
            GetRawInputData(
                HRAWINPUT(lparam.0 as *mut c_void),
                RID_INPUT,
                Some(raw.as_mut_ptr().cast()),
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            )
        };
        if copied == RAW_INPUT_ERROR {
            return Err(Error::Platform(format!(
                "GetRawInputData failed for Windows mouse input: {}",
                windows::core::Error::from_win32()
            )));
        }
        if copied < size_of::<RAWINPUTHEADER>() as u32 {
            return Err(Error::Platform(format!(
                "GetRawInputData returned a truncated header ({copied} bytes)"
            )));
        }

        let raw = unsafe { raw.assume_init() };
        if raw.header.dwType != RIM_TYPEMOUSE.0 {
            return Ok(None);
        }
        Ok(decode_raw_mouse(unsafe { &raw.data.mouse }))
    }

    pub(super) fn drain_pending(&self) -> Result<()> {
        let mut message = MSG::default();
        while unsafe {
            PeekMessageW(
                &mut message,
                Some(self.window),
                WM_INPUT,
                WM_INPUT,
                PM_REMOVE,
            )
        }
        .as_bool()
        {
            let read_result = self.read(message.lParam);
            unsafe {
                DispatchMessageW(&message);
            }
            read_result?;
        }
        Ok(())
    }

    pub(super) fn restore(&mut self) -> Result<()> {
        self.cleanup()
    }

    fn cleanup(&mut self) -> Result<()> {
        if !self.registered {
            return Ok(());
        }
        self.registered = false;

        let mut first_error = None;
        match registered_devices() {
            Ok(devices)
                if registration_is_owned_by(existing_mouse_registration(&devices), self.window) =>
            {
                let removal = RAWINPUTDEVICE {
                    usUsagePage: GENERIC_DESKTOP_PAGE,
                    usUsage: GENERIC_DESKTOP_MOUSE,
                    dwFlags: RIDEV_REMOVE,
                    hwndTarget: HWND::default(),
                };
                if let Err(error) = unsafe {
                    RegisterRawInputDevices(&[removal], size_of::<RAWINPUTDEVICE>() as u32)
                } {
                    first_error = Some(Error::HookStopFailed(format!(
                        "Failed to remove Windows Raw Input mouse registration: {error}"
                    )));
                } else if let Some(previous) = self.previous_registration
                    && let Err(error) = unsafe {
                        RegisterRawInputDevices(&[previous], size_of::<RAWINPUTDEVICE>() as u32)
                    }
                {
                    first_error = Some(Error::HookStopFailed(format!(
                        "Failed to restore Windows Raw Input mouse registration: {error}"
                    )));
                }
            }
            Ok(_) => {}
            Err(error) => first_error = Some(error),
        }

        if let Err(error) = unsafe { DestroyWindow(self.window) }
            && first_error.is_none()
        {
            first_error = Some(Error::HookStopFailed(format!(
                "Failed to destroy Windows Raw Input receiver: {error}"
            )));
        }
        self.window = HWND::default();

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for RawMouseInput {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn decode_raw_mouse(raw: &RAWMOUSE) -> Option<RawMouseMotion> {
    let absolute = raw.usFlags.0 & MOUSE_MOVE_ABSOLUTE.0 != 0;
    if raw.lLastX == 0 && raw.lLastY == 0 && !absolute {
        return None;
    }
    if absolute {
        Some(RawMouseMotion::Absolute {
            normalized_x: raw.lLastX.clamp(0, 65_535),
            normalized_y: raw.lLastY.clamp(0, 65_535),
            virtual_desktop: raw.usFlags.0 & MOUSE_VIRTUAL_DESKTOP.0 != 0,
        })
    } else {
        Some(RawMouseMotion::Relative {
            delta_x: raw.lLastX,
            delta_y: raw.lLastY,
        })
    }
}

fn absolute_axis(value: i32, origin: i32, extent: i32) -> f64 {
    if extent <= 1 {
        return origin as f64;
    }
    origin as f64 + value as f64 * (extent - 1) as f64 / 65_535.0
}

fn absolute_point(x: i32, y: i32, bounds: DesktopBounds) -> (f64, f64) {
    (
        absolute_axis(x, bounds.x, bounds.width),
        absolute_axis(y, bounds.y, bounds.height),
    )
}

pub(super) fn event_from_motion(
    motion: RawMouseMotion,
    absolute_position: (f64, f64),
    bounds: DesktopBounds,
    dragging: bool,
) -> Option<Event> {
    match motion {
        RawMouseMotion::Relative { delta_x, delta_y } => {
            let (x, y) = absolute_position;
            if dragging {
                Some(Event::mouse_dragged_relative(
                    x,
                    y,
                    delta_x as f64,
                    delta_y as f64,
                ))
            } else {
                Some(Event::mouse_moved_relative(
                    x,
                    y,
                    delta_x as f64,
                    delta_y as f64,
                ))
            }
        }
        RawMouseMotion::Absolute {
            normalized_x,
            normalized_y,
            ..
        } => {
            let (x, y) = absolute_point(normalized_x, normalized_y, bounds);
            if dragging {
                Some(Event::mouse_dragged(x, y))
            } else {
                Some(Event::mouse_moved(x, y))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventType, RelativeMotion};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::{
        MOUSE_MOVE_ABSOLUTE, MOUSE_MOVE_RELATIVE, MOUSE_STATE, MOUSE_VIRTUAL_DESKTOP,
        RAWINPUTDEVICE, RAWMOUSE,
    };

    fn registration(usage_page: u16, usage: u16, window: HWND) -> RAWINPUTDEVICE {
        RAWINPUTDEVICE {
            usUsagePage: usage_page,
            usUsage: usage,
            hwndTarget: window,
            ..Default::default()
        }
    }

    fn raw_mouse(flags: MOUSE_STATE, x: i32, y: i32) -> RAWMOUSE {
        RAWMOUSE {
            usFlags: flags,
            lLastX: x,
            lLastY: y,
            ..Default::default()
        }
    }

    #[test]
    fn decodes_relative_raw_mouse_motion() {
        let raw = raw_mouse(MOUSE_MOVE_RELATIVE, 14, -9);

        assert_eq!(
            decode_raw_mouse(&raw),
            Some(RawMouseMotion::Relative {
                delta_x: 14,
                delta_y: -9,
            })
        );
    }

    #[test]
    fn ignores_zero_relative_motion() {
        let raw = raw_mouse(MOUSE_MOVE_RELATIVE, 0, 0);

        assert_eq!(decode_raw_mouse(&raw), None);
    }

    #[test]
    fn decodes_absolute_virtual_desktop_motion() {
        let raw = raw_mouse(
            MOUSE_STATE(MOUSE_MOVE_ABSOLUTE.0 | MOUSE_VIRTUAL_DESKTOP.0),
            32_768,
            65_535,
        );

        assert_eq!(
            decode_raw_mouse(&raw),
            Some(RawMouseMotion::Absolute {
                normalized_x: 32_768,
                normalized_y: 65_535,
                virtual_desktop: true,
            })
        );
    }

    #[test]
    fn normalizes_absolute_raw_coordinates_to_desktop_pixels() {
        let bounds = DesktopBounds {
            x: -1920,
            y: 0,
            width: 3840,
            height: 1080,
        };

        assert_eq!(absolute_point(0, 0, bounds), (-1920.0, 0.0));
        assert_eq!(absolute_point(65_535, 65_535, bounds), (1919.0, 1079.0));
    }

    #[test]
    fn relative_event_retains_absolute_point_and_drag_state() {
        let moved = event_from_motion(
            RawMouseMotion::Relative {
                delta_x: 3,
                delta_y: -2,
            },
            (100.0, 200.0),
            DesktopBounds::default(),
            false,
        )
        .unwrap();
        let dragged = event_from_motion(
            RawMouseMotion::Relative {
                delta_x: 3,
                delta_y: -2,
            },
            (100.0, 200.0),
            DesktopBounds::default(),
            true,
        )
        .unwrap();

        assert_eq!(moved.event_type, EventType::MouseMoved);
        assert_eq!(dragged.event_type, EventType::MouseDragged);
        assert_eq!(
            moved.mouse.unwrap().relative,
            Some(RelativeMotion {
                delta_x: 3.0,
                delta_y: -2.0,
            })
        );
    }

    #[test]
    fn finds_only_generic_desktop_mouse_registration() {
        let registrations = [
            registration(0x01, 0x06, HWND(10 as _)),
            registration(0x01, 0x02, HWND(20 as _)),
        ];

        assert_eq!(
            existing_mouse_registration(&registrations)
                .unwrap()
                .hwndTarget,
            HWND(20 as _)
        );
    }

    #[test]
    fn restore_is_allowed_only_while_monio_still_owns_mouse_registration() {
        let monio_window = HWND(30 as _);

        assert!(registration_is_owned_by(
            Some(registration(0x01, 0x02, monio_window)),
            monio_window
        ));
        assert!(!registration_is_owned_by(
            Some(registration(0x01, 0x02, HWND(31 as _))),
            monio_window
        ));
    }
}
