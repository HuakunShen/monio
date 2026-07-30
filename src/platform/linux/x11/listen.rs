//! X11 input listening using XRecord.

use crate::error::{Error, Result};
use crate::event::{Button, Event, InputOrigin, ScrollDirection};
use crate::hook::{EventHandler, GrabHandler};
use crate::platform::linux::x11::provenance::RequestCorrelation;
use crate::platform::linux::x11::simulate;
use crate::state::{
    self, MASK_ALT, MASK_BUTTON1, MASK_BUTTON2, MASK_BUTTON3, MASK_CTRL, MASK_META, MASK_SHIFT,
};
use std::os::raw::{c_char, c_int, c_uchar, c_ulong};
use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use x11::xlib;
use x11::xrecord;

use crate::platform::linux::keycodes::keycode_to_key;

/// Stored handler for the callback
static HANDLER: Mutex<Option<Box<dyn EventHandler>>> = Mutex::new(None);

/// Flag to signal stopping
static STOP_FLAG: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// XRecord context for stopping the hook
static CONTEXT: Mutex<Option<xrecord::XRecordContext>> = Mutex::new(None);

const FALSE: c_int = 0;

/// XRecord data structure for events
#[repr(C)]
struct XRecordDatum {
    type_: u8,
    code: u8,
    _rest: u64,
    _1: bool,
    _2: bool,
    _3: bool,
    root_x: i16,
    root_y: i16,
    _event_x: i16,
    _event_y: i16,
    _state: u16,
}

/// Update modifier mask from keyboard event
fn update_key_modifier(code: u32, pressed: bool) {
    let mask = match code {
        50 | 62 => MASK_SHIFT,  // Shift L/R
        37 | 105 => MASK_CTRL,  // Control L/R
        64 | 108 => MASK_ALT,   // Alt L/R
        133 | 134 => MASK_META, // Super L/R
        _ => return,
    };

    if pressed {
        state::set_mask(mask);
    } else {
        state::unset_mask(mask);
    }
}

/// Convert X11 event to our Event type
fn convert_event(type_: c_int, code: u8, x: f64, y: f64) -> Option<Event> {
    match type_ {
        t if t == xlib::KeyPress => {
            let code32 = code as u32;
            update_key_modifier(code32, true);
            let key = keycode_to_key(code32);
            Some(Event::key_pressed(key, code32))
        }

        t if t == xlib::KeyRelease => {
            let code32 = code as u32;
            update_key_modifier(code32, false);
            let key = keycode_to_key(code32);
            Some(Event::key_released(key, code32))
        }

        t if t == xlib::ButtonPress => {
            match code {
                1 => {
                    state::set_mask(MASK_BUTTON1);
                    Some(Event::mouse_pressed(Button::Left, x, y))
                }
                2 => {
                    state::set_mask(MASK_BUTTON3);
                    Some(Event::mouse_pressed(Button::Middle, x, y))
                }
                3 => {
                    state::set_mask(MASK_BUTTON2);
                    Some(Event::mouse_pressed(Button::Right, x, y))
                }
                // Scroll wheel events in X11
                4 => Some(Event::mouse_wheel(x, y, ScrollDirection::Up, 1.0)),
                5 => Some(Event::mouse_wheel(x, y, ScrollDirection::Down, 1.0)),
                6 => Some(Event::mouse_wheel(x, y, ScrollDirection::Left, 1.0)),
                7 => Some(Event::mouse_wheel(x, y, ScrollDirection::Right, 1.0)),
                c => Some(Event::mouse_pressed(Button::Unknown(c), x, y)),
            }
        }

        t if t == xlib::ButtonRelease => {
            match code {
                1 => {
                    state::unset_mask(MASK_BUTTON1);
                    Some(Event::mouse_released(Button::Left, x, y))
                }
                2 => {
                    state::unset_mask(MASK_BUTTON3);
                    Some(Event::mouse_released(Button::Middle, x, y))
                }
                3 => {
                    state::unset_mask(MASK_BUTTON2);
                    Some(Event::mouse_released(Button::Right, x, y))
                }
                4..=7 => None, // Wheel "release" - ignored
                c => Some(Event::mouse_released(Button::Unknown(c), x, y)),
            }
        }

        t if t == xlib::MotionNotify => {
            // THE KEY FIX: Check button state for drag detection
            if state::is_button_held() {
                Some(Event::mouse_dragged(x, y))
            } else {
                Some(Event::mouse_moved(x, y))
            }
        }

        _ => None,
    }
}

/// XRecord callback
unsafe extern "C" fn record_callback(
    closure: *mut c_char,
    raw_data: *mut xrecord::XRecordInterceptData,
) {
    unsafe {
        let data = match raw_data.as_ref() {
            Some(d) => d,
            None => return,
        };

        let correlation = (closure as *mut RequestCorrelation).as_mut();
        if data.category == xrecord::XRecordFromClient {
            if let Some(correlation) = correlation {
                correlation.observe_request(data);
            }
            xrecord::XRecordFreeData(raw_data);
            return;
        }
        if data.category != xrecord::XRecordFromServer {
            xrecord::XRecordFreeData(raw_data);
            return;
        }

        // Check stop flag
        if let Ok(guard) = STOP_FLAG.lock()
            && let Some(ref flag) = *guard
            && !flag.load(Ordering::SeqCst)
        {
            xrecord::XRecordFreeData(raw_data);
            return;
        }

        // Parse the event data
        #[allow(clippy::cast_ptr_alignment)]
        let xdatum = match (data.data as *const XRecordDatum).as_ref() {
            Some(d) => d,
            None => {
                xrecord::XRecordFreeData(raw_data);
                return;
            }
        };

        let type_ = xdatum.type_ as c_int;
        let code = xdatum.code;
        let x = xdatum.root_x as f64;
        let y = xdatum.root_y as f64;
        let origin = correlation
            .map(|correlation| correlation.classify_device_event(xdatum.type_, code))
            .unwrap_or(InputOrigin::Unknown);

        if let Some(mut event) = convert_event(type_, code, x, y)
            && let Ok(guard) = HANDLER.lock()
            && let Some(ref handler) = *guard
        {
            event.origin = origin;
            handler.handle_event(&event);
        }

        xrecord::XRecordFreeData(raw_data);
    }
}

/// Run the event hook (blocking).
pub fn run_hook<H: EventHandler + 'static>(running: &Arc<AtomicBool>, handler: H) -> Result<()> {
    // Store handler and stop flag
    {
        let mut h = HANDLER
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *h = Some(Box::new(handler));
    }
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = Some(running.clone());
    }

    unsafe {
        // Open display
        let dpy_control = xlib::XOpenDisplay(null());
        if dpy_control.is_null() {
            return Err(Error::HookStartFailed("Failed to open X display".into()));
        }

        // Check for RECORD extension
        let extension_name = c"RECORD";
        let extension = xlib::XInitExtension(dpy_control, extension_name.as_ptr());
        if extension.is_null() {
            xlib::XCloseDisplay(dpy_control);
            return Err(Error::HookStartFailed(
                "XRecord extension not available".into(),
            ));
        }

        let correlation_config = query_correlation_config(dpy_control);

        // Prepare record range
        let mut record_range: xrecord::XRecordRange = *xrecord::XRecordAllocRange();
        record_range.device_events.first = xlib::KeyPress as c_uchar;
        record_range.device_events.last = xlib::MotionNotify as c_uchar;
        if let Some((_, xtest_major_opcode)) = correlation_config {
            record_range.ext_requests.ext_major.first = xtest_major_opcode;
            record_range.ext_requests.ext_major.last = xtest_major_opcode;
            record_range.ext_requests.ext_minor.first = 2;
            record_range.ext_requests.ext_minor.last = 2;
        }

        // Create context
        let mut record_all_clients: c_ulong = xrecord::XRecordAllClients;
        let context = xrecord::XRecordCreateContext(
            dpy_control,
            0,
            &mut record_all_clients,
            1,
            &mut &mut record_range as *mut &mut xrecord::XRecordRange
                as *mut *mut xrecord::XRecordRange,
            1,
        );

        if context == 0 {
            xlib::XCloseDisplay(dpy_control);
            return Err(Error::HookStartFailed(
                "Failed to create XRecord context".into(),
            ));
        }

        xlib::XSync(dpy_control, FALSE);

        // Store context for stop_hook to use
        {
            let mut c = CONTEXT
                .lock()
                .map_err(|_| Error::ThreadError("context mutex poisoned".into()))?;
            *c = Some(context);
        }

        // Send hook enabled event
        if let Ok(guard) = HANDLER.lock()
            && let Some(ref handler) = *guard
        {
            handler.handle_event(&Event::hook_enabled());
        }

        let mut correlation = correlation_config.map(|(client_id_base, xtest_major_opcode)| {
            RequestCorrelation::new(client_id_base, xtest_major_opcode)
        });
        let closure = correlation
            .as_mut()
            .map(|correlation| correlation as *mut RequestCorrelation as *mut c_char)
            .unwrap_or(null::<c_char>() as *mut c_char);

        // Run the record loop
        let result =
            xrecord::XRecordEnableContext(dpy_control, context, Some(record_callback), closure);

        // Send hook disabled event
        if let Ok(guard) = HANDLER.lock()
            && let Some(ref handler) = *guard
        {
            handler.handle_event(&Event::hook_disabled());
        }

        // Clean up
        xrecord::XRecordDisableContext(dpy_control, context);
        xrecord::XRecordFreeContext(dpy_control, context);
        xlib::XCloseDisplay(dpy_control);

        if result == 0 {
            return Err(Error::HookStartFailed(
                "Failed to enable XRecord context".into(),
            ));
        }
    }

    // Clean up handler and statics
    {
        let mut h = HANDLER
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *h = None;
    }
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = None;
    }
    {
        let mut c = CONTEXT
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *c = None;
    }

    Ok(())
}

unsafe fn query_correlation_config(display: *mut xlib::Display) -> Option<(c_ulong, c_uchar)> {
    let client_id_base = match simulate::initialize() {
        Ok(client_id_base) => client_id_base,
        Err(error) => {
            log::warn!("X11 self-injection detection unavailable: {error}");
            return None;
        }
    };

    let mut major_opcode = 0;
    let mut first_event = 0;
    let mut first_error = 0;
    if unsafe {
        xlib::XQueryExtension(
            display,
            c"XTEST".as_ptr(),
            &mut major_opcode,
            &mut first_event,
            &mut first_error,
        )
    } == 0
        || !(0..=u8::MAX as c_int).contains(&major_opcode)
    {
        log::warn!("X11 self-injection detection unavailable: XTEST extension not available");
        return None;
    }

    Some((client_id_base, major_opcode as c_uchar))
}

/// Stop the event hook.
pub fn stop_hook() -> Result<()> {
    // Signal the stop flag to tell the XRecord loop to exit
    if let Ok(guard) = STOP_FLAG.lock()
        && let Some(ref flag) = *guard
    {
        flag.store(false, Ordering::SeqCst);
    }

    // XRecordDisableContext needs to be called from a separate control display
    // connection to unblock XRecordEnableContext on the data connection
    unsafe {
        if let Ok(ctx_guard) = CONTEXT.lock()
            && let Some(ctx) = *ctx_guard
        {
            // Open a new display connection for the control channel
            let dpy_control = xlib::XOpenDisplay(null());
            if !dpy_control.is_null() {
                xrecord::XRecordDisableContext(dpy_control, ctx);
                xlib::XCloseDisplay(dpy_control);
            }
        }
    }

    Ok(())
}

/// Wrapper to adapt a GrabHandler to an EventHandler.
/// Used when grab is not supported and we fall back to listen mode.
struct GrabToListenAdapter<H: GrabHandler>(H);

impl<H: GrabHandler> EventHandler for GrabToListenAdapter<H> {
    fn handle_event(&self, event: &Event) {
        // Call the grab handler but ignore the return value
        // since we can't actually consume events in listen mode
        let _ = self.0.handle_event(event);
    }
}

/// Run the event hook with grab capability (blocking).
///
/// **Note**: X11 XRecord does not support event grabbing. This function
/// falls back to listen mode. The handler will still be called, but
/// returning `None` will not actually consume events.
///
/// For true event grabbing on Linux, consider using evdev or XI2.
pub fn run_grab_hook<H: GrabHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()> {
    log::warn!(
        "X11 XRecord does not support event grabbing. \
        Falling back to listen mode. Events cannot be consumed."
    );

    // Wrap the grab handler as an event handler
    let adapter = GrabToListenAdapter(handler);
    run_hook(running, adapter)
}
