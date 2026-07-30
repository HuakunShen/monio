use crate::error::{Error, Result};
use crate::event::RelativeMotion;
use std::os::raw::c_int;
use std::slice;
use x11::{xinput2, xlib};

const XI2_MAJOR: c_int = 2;
const XI2_MINOR: c_int = 1;
const RAW_MOTION_MASK_LEN: usize = (xinput2::XI_RawMotion as usize / 8) + 1;

fn supports_grab_independent_raw_events(major: c_int, minor: c_int) -> bool {
    major > XI2_MAJOR || (major == XI2_MAJOR && minor >= XI2_MINOR)
}

pub(super) struct RawMotionInput {
    opcode: c_int,
    root: xlib::Window,
    selected: bool,
}

impl RawMotionInput {
    pub(super) fn initialize(display: *mut xlib::Display, root: xlib::Window) -> Result<Self> {
        let mut opcode = 0;
        let mut first_event = 0;
        let mut first_error = 0;
        let extension_available = unsafe {
            xlib::XQueryExtension(
                display,
                c"XInputExtension".as_ptr(),
                &mut opcode,
                &mut first_event,
                &mut first_error,
            )
        };
        if extension_available == 0 {
            return Err(Error::HookStartFailed(
                "XInputExtension is unavailable; X11 grab requires XI2 2.1 or newer".into(),
            ));
        }

        let mut major = XI2_MAJOR;
        let mut minor = XI2_MINOR;
        let version_status = unsafe { xinput2::XIQueryVersion(display, &mut major, &mut minor) };
        if version_status != xlib::Success as c_int
            || !supports_grab_independent_raw_events(major, minor)
        {
            return Err(Error::HookStartFailed(format!(
                "X11 grab requires XI2 2.1 or newer; server reported {major}.{minor}"
            )));
        }

        let mut input = Self {
            opcode,
            root,
            selected: false,
        };
        input.select(display)?;
        Ok(input)
    }

    pub(super) fn select(&mut self, display: *mut xlib::Display) -> Result<()> {
        if self.selected {
            return Ok(());
        }

        let mut mask = [0u8; RAW_MOTION_MASK_LEN];
        xinput2::XISetMask(&mut mask, xinput2::XI_RawMotion);
        self.update_selection(display, &mut mask)?;
        self.selected = true;
        Ok(())
    }

    pub(super) fn deselect(&mut self, display: *mut xlib::Display) -> Result<()> {
        if !self.selected {
            return Ok(());
        }

        let mut mask = [0u8; RAW_MOTION_MASK_LEN];
        self.update_selection(display, &mut mask)?;
        self.selected = false;
        Ok(())
    }

    fn update_selection(&self, display: *mut xlib::Display, mask: &mut [u8]) -> Result<()> {
        let mut event_mask = xinput2::XIEventMask {
            deviceid: xinput2::XIAllMasterDevices,
            mask_len: mask.len() as c_int,
            mask: mask.as_mut_ptr(),
        };
        let status = unsafe { xinput2::XISelectEvents(display, self.root, &mut event_mask, 1) };
        unsafe { xlib::XSync(display, xlib::False) };

        if status == xlib::Success as c_int {
            Ok(())
        } else {
            Err(Error::HookStartFailed(format!(
                "XISelectEvents failed with status {status}"
            )))
        }
    }

    pub(super) fn is_selected(&self) -> bool {
        self.selected
    }

    pub(super) fn decode(
        &self,
        display: *mut xlib::Display,
        event: &mut xlib::XEvent,
    ) -> Result<Option<RelativeMotion>> {
        if event.get_type() != xlib::GenericEvent {
            return Ok(None);
        }

        let cookie = unsafe { &mut event.generic_event_cookie };
        if cookie.extension != self.opcode || cookie.evtype != xinput2::XI_RawMotion {
            return Ok(None);
        }

        if unsafe { xlib::XGetEventData(display, cookie) } == 0 {
            return Err(Error::Platform(
                "XGetEventData failed for XI_RawMotion".into(),
            ));
        }

        let result = unsafe { decode_cookie(cookie) };
        unsafe { xlib::XFreeEventData(display, cookie) };
        result
    }
}

unsafe fn decode_cookie(cookie: &xlib::XGenericEventCookie) -> Result<Option<RelativeMotion>> {
    let raw = unsafe { (cookie.data as *const xinput2::XIRawEvent).as_ref() }
        .ok_or_else(|| Error::Platform("XI_RawMotion cookie contained no data".into()))?;

    let mask_len = usize::try_from(raw.valuators.mask_len)
        .map_err(|_| Error::Platform("XI_RawMotion contained a negative mask length".into()))?;
    if mask_len == 0 {
        return Ok(None);
    }
    if raw.valuators.mask.is_null() {
        return Err(Error::Platform(
            "XI_RawMotion contained a null valuator mask".into(),
        ));
    }

    let mask = unsafe { slice::from_raw_parts(raw.valuators.mask, mask_len) };
    let value_count = mask.iter().map(|byte| byte.count_ones() as usize).sum();
    if value_count == 0 {
        return Ok(None);
    }
    if raw.raw_values.is_null() {
        return Err(Error::Platform(
            "XI_RawMotion contained null raw valuator values".into(),
        ));
    }

    let values = unsafe { slice::from_raw_parts(raw.raw_values, value_count) };
    Ok(decode_axes(mask, values))
}

fn decode_axes(mask: &[u8], values: &[f64]) -> Option<RelativeMotion> {
    let mut value_index = 0;
    let mut delta_x = None;
    let mut delta_y = None;

    for axis in 0..mask.len() * 8 {
        if mask[axis / 8] & (1 << (axis % 8)) == 0 {
            continue;
        }

        let value = *values.get(value_index)?;
        value_index += 1;

        match axis {
            0 => delta_x = Some(value),
            1 => delta_y = Some(value),
            _ => {}
        }
    }

    (delta_x.is_some() || delta_y.is_some()).then(|| RelativeMotion {
        delta_x: delta_x.unwrap_or(0.0),
        delta_y: delta_y.unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xi20_lacks_grab_independent_raw_events() {
        assert!(!supports_grab_independent_raw_events(2, 0));
    }

    #[test]
    fn xi21_has_grab_independent_raw_events() {
        assert!(supports_grab_independent_raw_events(2, 1));
    }

    #[test]
    fn decodes_both_relative_axes() {
        assert_eq!(
            decode_axes(&[0b0000_0011], &[3.5, -4.25]),
            Some(RelativeMotion {
                delta_x: 3.5,
                delta_y: -4.25,
            })
        );
    }

    #[test]
    fn decodes_sparse_y_axis() {
        assert_eq!(
            decode_axes(&[0b0000_0010], &[7.0]),
            Some(RelativeMotion {
                delta_x: 0.0,
                delta_y: 7.0,
            })
        );
    }

    #[test]
    fn ignores_events_without_xy_axes() {
        assert_eq!(decode_axes(&[0b0000_0100], &[9.0]), None);
    }
}
