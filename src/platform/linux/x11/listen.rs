//! X11 input listening using XRecord.

use crate::error::{Error, Result};
use crate::event::{Button, Event, InputOrigin, ScrollDirection};
use crate::hook::{EventHandler, GrabHandler};
use crate::platform::linux::x11::provenance::RequestCorrelation;
use crate::platform::linux::x11::simulate;
use crate::state::{
    self, MASK_ALT, MASK_BUTTON1, MASK_BUTTON2, MASK_BUTTON3, MASK_CTRL, MASK_META, MASK_SHIFT,
};
use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ulong};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
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

const POINTER_EVENT_MASK: c_uint =
    (xlib::ButtonPressMask | xlib::ButtonReleaseMask | xlib::PointerMotionMask) as c_uint;
const GRAB_POLL_INTERVAL: Duration = Duration::from_millis(2);

struct ActiveGrabs {
    display: *mut xlib::Display,
    window: xlib::Window,
    keyboard_grabbed: bool,
    pointer_grabbed: bool,
}

impl ActiveGrabs {
    fn acquire() -> Result<Self> {
        unsafe {
            let display = xlib::XOpenDisplay(null());
            if display.is_null() {
                return Err(Error::HookStartFailed("Failed to open X display".into()));
            }

            let screen = xlib::XDefaultScreen(display);
            let root = xlib::XRootWindow(display, screen);
            let mut attributes: xlib::XSetWindowAttributes = std::mem::zeroed();
            attributes.override_redirect = xlib::True;
            attributes.event_mask = (xlib::KeyPressMask
                | xlib::KeyReleaseMask
                | xlib::ButtonPressMask
                | xlib::ButtonReleaseMask
                | xlib::PointerMotionMask) as _;

            let window = xlib::XCreateWindow(
                display,
                root,
                -1,
                -1,
                1,
                1,
                0,
                0,
                xlib::InputOnly as c_uint,
                null_mut(),
                xlib::CWOverrideRedirect | xlib::CWEventMask,
                &mut attributes,
            );
            if window == 0 {
                xlib::XCloseDisplay(display);
                return Err(Error::HookStartFailed(
                    "Failed to create X11 grab window".into(),
                ));
            }

            xlib::XMapWindow(display, window);
            xlib::XSync(display, FALSE);

            let mut grabs = Self {
                display,
                window,
                keyboard_grabbed: false,
                pointer_grabbed: false,
            };
            grabs.grab_keyboard()?;
            grabs.grab_pointer()?;
            Ok(grabs)
        }
    }

    fn grab_keyboard(&mut self) -> Result<()> {
        let status = unsafe {
            xlib::XGrabKeyboard(
                self.display,
                self.window,
                xlib::False,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                xlib::CurrentTime,
            )
        };
        if status != xlib::GrabSuccess {
            return Err(grab_failed("keyboard", status));
        }
        self.keyboard_grabbed = true;
        Ok(())
    }

    fn grab_pointer(&mut self) -> Result<()> {
        let status = unsafe {
            xlib::XGrabPointer(
                self.display,
                self.window,
                xlib::False,
                POINTER_EVENT_MASK,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                0,
                0,
                xlib::CurrentTime,
            )
        };
        if status != xlib::GrabSuccess {
            return Err(grab_failed("pointer", status));
        }
        self.pointer_grabbed = true;
        Ok(())
    }

    fn replay_key(&mut self, keycode: u32, pressed: bool) -> Result<()> {
        unsafe {
            xlib::XUngrabKeyboard(self.display, xlib::CurrentTime);
            xlib::XSync(self.display, FALSE);
        }
        self.keyboard_grabbed = false;

        let replay_result = simulate::replay_keycode(keycode, pressed);
        self.grab_keyboard()?;
        replay_result
    }

    fn replay_button(&mut self, button: u32, pressed: bool) -> Result<()> {
        unsafe {
            xlib::XUngrabPointer(self.display, xlib::CurrentTime);
            xlib::XSync(self.display, FALSE);
        }
        self.pointer_grabbed = false;

        let replay_result = simulate::replay_button(button, pressed);
        self.grab_pointer()?;
        replay_result
    }

    fn begin_pointer_passthrough(&mut self, button: u32) -> Result<()> {
        unsafe {
            xlib::XUngrabPointer(self.display, xlib::CurrentTime);
            xlib::XSync(self.display, FALSE);
        }
        self.pointer_grabbed = false;

        if let Err(error) = simulate::replay_button(button, true) {
            self.grab_pointer()?;
            return Err(error);
        }
        Ok(())
    }

    fn replay_ungrabbed_pointer_event(&self, event: &xlib::XEvent) -> Result<()> {
        let type_ = event.get_type();
        unsafe {
            match type_ {
                xlib::ButtonPress | xlib::ButtonRelease => {
                    let button = event.button;
                    let _ = convert_event(
                        type_,
                        button.button as u8,
                        button.x_root as f64,
                        button.y_root as f64,
                    );
                    simulate::replay_button(button.button, type_ == xlib::ButtonPress)
                }
                xlib::MotionNotify => {
                    let motion = event.motion;
                    simulate::replay_motion(motion.x_root, motion.y_root)
                }
                _ => Ok(()),
            }
        }
    }

    fn try_reacquire_pointer(&mut self) -> Result<bool> {
        let status = unsafe {
            xlib::XGrabPointer(
                self.display,
                self.window,
                xlib::False,
                POINTER_EVENT_MASK,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                0,
                0,
                xlib::CurrentTime,
            )
        };

        match status {
            xlib::GrabSuccess => {
                self.pointer_grabbed = true;
                self.sync_pointer_button_state();
                Ok(true)
            }
            xlib::AlreadyGrabbed | xlib::GrabFrozen => Ok(false),
            _ => Err(grab_failed("pointer", status)),
        }
    }

    fn sync_pointer_button_state(&self) {
        let screen = unsafe { xlib::XDefaultScreen(self.display) };
        let root = unsafe { xlib::XRootWindow(self.display, screen) };
        let mut root_return = 0;
        let mut child_return = 0;
        let mut root_x = 0;
        let mut root_y = 0;
        let mut window_x = 0;
        let mut window_y = 0;
        let mut mask = 0;
        unsafe {
            xlib::XQueryPointer(
                self.display,
                root,
                &mut root_return,
                &mut child_return,
                &mut root_x,
                &mut root_y,
                &mut window_x,
                &mut window_y,
                &mut mask,
            );
        }

        sync_button_mask(mask, xlib::Button1Mask, MASK_BUTTON1);
        sync_button_mask(mask, xlib::Button2Mask, MASK_BUTTON3);
        sync_button_mask(mask, xlib::Button3Mask, MASK_BUTTON2);
    }

    fn replay_motion(&mut self, x: c_int, y: c_int) -> Result<()> {
        unsafe {
            xlib::XUngrabPointer(self.display, xlib::CurrentTime);
            xlib::XSync(self.display, FALSE);
        }
        self.pointer_grabbed = false;

        let replay_result = simulate::replay_motion(x, y);
        self.grab_pointer()?;
        replay_result
    }
}

fn sync_button_mask(x11_mask: c_uint, x11_button: c_uint, monio_button: u32) {
    if x11_mask & x11_button != 0 {
        state::set_mask(monio_button);
    } else {
        state::unset_mask(monio_button);
    }
}

impl Drop for ActiveGrabs {
    fn drop(&mut self) {
        unsafe {
            if self.pointer_grabbed {
                xlib::XUngrabPointer(self.display, xlib::CurrentTime);
            }
            if self.keyboard_grabbed {
                xlib::XUngrabKeyboard(self.display, xlib::CurrentTime);
            }
            if self.window != 0 {
                xlib::XDestroyWindow(self.display, self.window);
            }
            xlib::XSync(self.display, FALSE);
            xlib::XCloseDisplay(self.display);
        }
    }
}

fn grab_failed(device: &str, status: c_int) -> Error {
    let reason = match status {
        xlib::AlreadyGrabbed => "another X11 client already owns the grab",
        xlib::GrabInvalidTime => "the grab timestamp is invalid",
        xlib::GrabNotViewable => "the grab window is not viewable",
        xlib::GrabFrozen => "the device is frozen by another client",
        _ => "unknown X11 grab error",
    };
    Error::HookStartFailed(format!(
        "Failed to grab X11 {device}: {reason} (status {status})"
    ))
}

fn handle_grab_event<H: GrabHandler>(
    grabs: &mut ActiveGrabs,
    handler: &H,
    event: &xlib::XEvent,
    replay_wheel_release: &mut [bool; 4],
) -> Result<()> {
    let type_ = event.get_type();

    unsafe {
        match type_ {
            xlib::KeyPress | xlib::KeyRelease => {
                let key = event.key;
                if let Some(event) = convert_event(
                    type_,
                    key.keycode as u8,
                    key.x_root as f64,
                    key.y_root as f64,
                ) && handler.handle_event(&event).is_some()
                {
                    grabs.replay_key(key.keycode, type_ == xlib::KeyPress)?;
                }
            }
            xlib::ButtonPress | xlib::ButtonRelease => {
                let button = event.button;
                let code = button.button;

                if type_ == xlib::ButtonRelease && (4..=7).contains(&code) {
                    let replay = std::mem::take(&mut replay_wheel_release[(code - 4) as usize]);
                    if replay {
                        grabs.replay_button(code, false)?;
                    }
                    return Ok(());
                }

                if let Some(event) = convert_event(
                    type_,
                    code as u8,
                    button.x_root as f64,
                    button.y_root as f64,
                ) {
                    let replay = handler.handle_event(&event).is_some();
                    if type_ == xlib::ButtonPress && (4..=7).contains(&code) {
                        replay_wheel_release[(code - 4) as usize] = replay;
                    }
                    if replay {
                        if type_ == xlib::ButtonPress {
                            grabs.begin_pointer_passthrough(code)?;
                        } else {
                            grabs.replay_button(code, false)?;
                        }
                    }
                }
            }
            xlib::MotionNotify => {
                let motion = event.motion;
                if let Some(event) =
                    convert_event(type_, 0, motion.x_root as f64, motion.y_root as f64)
                    && handler.handle_event(&event).is_some()
                {
                    grabs.replay_motion(motion.x_root, motion.y_root)?;
                }
            }
            _ => {}
        }
    }

    Ok(())
}

/// Run the event hook with active X11 keyboard and pointer grabs (blocking).
///
/// Returning `None` consumes an event. Returning `Some(event)` replays the raw
/// X11 event with XTest. A passed pointer press yields the complete local
/// pointer gesture because X11 gives the receiving application an implicit
/// pointer grab; Monio reacquires the pointer when that gesture ends.
pub fn run_grab_hook<H: GrabHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()> {
    {
        let mut stop_flag = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *stop_flag = Some(running.clone());
    }

    let result = (|| {
        let mut grabs = ActiveGrabs::acquire()?;
        handler.handle_event(&Event::hook_enabled());
        let mut replay_wheel_release = [false; 4];

        let loop_result = (|| {
            while running.load(Ordering::SeqCst) {
                if unsafe { xlib::XPending(grabs.display) } == 0 {
                    if !grabs.pointer_grabbed && grabs.try_reacquire_pointer()? {
                        continue;
                    }
                    thread::sleep(GRAB_POLL_INTERVAL);
                    continue;
                }

                let mut event: xlib::XEvent = unsafe { std::mem::zeroed() };
                unsafe { xlib::XNextEvent(grabs.display, &mut event) };
                if !grabs.pointer_grabbed
                    && matches!(
                        event.get_type(),
                        xlib::ButtonPress | xlib::ButtonRelease | xlib::MotionNotify
                    )
                {
                    if event.get_type() == xlib::ButtonRelease {
                        let code = unsafe { event.button.button };
                        if (4..=7).contains(&code) {
                            replay_wheel_release[(code - 4) as usize] = false;
                        }
                    }
                    grabs.replay_ungrabbed_pointer_event(&event)?;
                    continue;
                }
                handle_grab_event(&mut grabs, &handler, &event, &mut replay_wheel_release)?;
            }
            Ok(())
        })();

        handler.handle_event(&Event::hook_disabled());
        loop_result
    })();

    let mut stop_flag = STOP_FLAG
        .lock()
        .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
    *stop_flag = None;

    result
}
