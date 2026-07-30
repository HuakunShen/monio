//! Windows input listening using SetWindowsHookEx.

use crate::error::{Error, Result};
use crate::event::{Button, Event, ScrollDirection};
use crate::hook::{EventHandler, GrabHandler};
use crate::platform::motion::{Motion, motion_from_event};
use crate::state::{
    self, MASK_ALT, MASK_BUTTON1, MASK_BUTTON2, MASK_BUTTON3, MASK_BUTTON4, MASK_BUTTON5,
    MASK_CTRL, MASK_META, MASK_SHIFT,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// Wrapper for HHOOK to make it Send + Sync
#[derive(Clone, Copy)]
struct SendableHHOOK(HHOOK);

// SAFETY: HHOOK is just a handle/pointer that the Windows API owns.
// It's safe to send between threads because Windows handles are thread-safe.
unsafe impl Send for SendableHHOOK {}
unsafe impl Sync for SendableHHOOK {}
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, GetSystemMetrics, HC_ACTION, HHOOK,
    KBDLLHOOKSTRUCT, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SM_CXSCREEN,
    SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_INPUT, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::{
    keycodes::keycode_to_key,
    provenance,
    raw_input::{self, DesktopBounds, RawMouseInput, RawMouseMotion},
    simulate,
};

// Constants
const WHEEL_DELTA: i16 = 120;

/// Stored handler for the callback (listen mode)
static HANDLER: Mutex<Option<Box<dyn EventHandler>>> = Mutex::new(None);

/// Stored handler for the callback (grab mode)
static GRAB_HANDLER: Mutex<Option<Box<dyn GrabHandler>>> = Mutex::new(None);

/// Flag to signal stopping
static STOP_FLAG: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Hook handles
static KEYBOARD_HOOK: Mutex<Option<SendableHHOOK>> = Mutex::new(None);
static MOUSE_HOOK: Mutex<Option<SendableHHOOK>> = Mutex::new(None);

/// Thread ID for message posting
static THREAD_ID: Mutex<u32> = Mutex::new(0);

/// Flag indicating whether we're in grab mode
static GRAB_MODE: AtomicBool = AtomicBool::new(false);

/// Process-global Windows backend ownership.
static ACTIVE_SESSION: AtomicBool = AtomicBool::new(false);

/// Raw Input movement is dispatched only after both hooks are installed.
static GRAB_READY: AtomicBool = AtomicBool::new(false);

/// Latest absolute point supplied by the low-level mouse hook.
static LATEST_PHYSICAL_POINT: Mutex<Option<(i32, i32)>> = Mutex::new(None);

struct ActiveSession;

impl ActiveSession {
    fn claim() -> Result<Self> {
        ACTIVE_SESSION
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| Error::AlreadyRunning)?;
        Ok(Self)
    }
}

impl Drop for ActiveSession {
    fn drop(&mut self) {
        ACTIVE_SESSION.store(false, Ordering::SeqCst);
    }
}

struct CallbackState {
    grab: bool,
}

impl CallbackState {
    fn listen<H: EventHandler + 'static>(running: &Arc<AtomicBool>, handler: H) -> Result<Self> {
        let mut handler_slot = HANDLER
            .lock()
            .map_err(|_| Error::ThreadError("Windows handler mutex poisoned".into()))?;
        let mut stop_slot = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("Windows stop flag mutex poisoned".into()))?;
        let mut thread_slot = THREAD_ID
            .lock()
            .map_err(|_| Error::ThreadError("Windows thread ID mutex poisoned".into()))?;

        *handler_slot = Some(Box::new(handler));
        *stop_slot = Some(running.clone());
        *thread_slot = unsafe { GetCurrentThreadId() };
        GRAB_MODE.store(false, Ordering::SeqCst);
        GRAB_READY.store(false, Ordering::SeqCst);
        if let Ok(mut point) = LATEST_PHYSICAL_POINT.lock() {
            *point = None;
        }
        Ok(Self { grab: false })
    }

    fn grab<H: GrabHandler + 'static>(running: &Arc<AtomicBool>, handler: H) -> Result<Self> {
        let mut handler_slot = GRAB_HANDLER
            .lock()
            .map_err(|_| Error::ThreadError("Windows grab handler mutex poisoned".into()))?;
        let mut stop_slot = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("Windows stop flag mutex poisoned".into()))?;
        let mut thread_slot = THREAD_ID
            .lock()
            .map_err(|_| Error::ThreadError("Windows thread ID mutex poisoned".into()))?;

        *handler_slot = Some(Box::new(handler));
        *stop_slot = Some(running.clone());
        *thread_slot = unsafe { GetCurrentThreadId() };
        GRAB_MODE.store(true, Ordering::SeqCst);
        GRAB_READY.store(false, Ordering::SeqCst);
        if let Ok(mut point) = LATEST_PHYSICAL_POINT.lock() {
            *point = None;
        }
        Ok(Self { grab: true })
    }
}

impl Drop for CallbackState {
    fn drop(&mut self) {
        GRAB_READY.store(false, Ordering::SeqCst);
        GRAB_MODE.store(false, Ordering::SeqCst);
        if let Ok(mut point) = LATEST_PHYSICAL_POINT.lock() {
            *point = None;
        }
        if self.grab {
            if let Ok(mut handler) = GRAB_HANDLER.lock() {
                *handler = None;
            }
        } else if let Ok(mut handler) = HANDLER.lock() {
            *handler = None;
        }
        if let Ok(mut stop) = STOP_FLAG.lock() {
            *stop = None;
        }
        if let Ok(mut thread_id) = THREAD_ID.lock() {
            *thread_id = 0;
        }
    }
}

#[derive(Default)]
struct InstalledHooks {
    keyboard: Option<SendableHHOOK>,
    mouse: Option<SendableHHOOK>,
}

impl InstalledHooks {
    fn install() -> Result<Self> {
        let mut hooks = Self::default();

        let keyboard = unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_callback), None, 0).map_err(
                |error| Error::HookStartFailed(format!("Failed to set keyboard hook: {error}")),
            )?
        };
        hooks.keyboard = Some(SendableHHOOK(keyboard));
        {
            let mut published = KEYBOARD_HOOK
                .lock()
                .map_err(|_| Error::ThreadError("Windows keyboard hook mutex poisoned".into()))?;
            *published = hooks.keyboard;
        }

        let mouse = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_callback), None, 0) } {
            Ok(mouse) => mouse,
            Err(error) => {
                let _ = hooks.restore();
                return Err(Error::HookStartFailed(format!(
                    "Failed to set mouse hook: {error}"
                )));
            }
        };
        hooks.mouse = Some(SendableHHOOK(mouse));
        if MOUSE_HOOK
            .lock()
            .map(|mut published| {
                *published = hooks.mouse;
            })
            .is_err()
        {
            let message = "Windows mouse hook mutex poisoned".to_string();
            let _ = hooks.restore();
            return Err(Error::ThreadError(message));
        }

        Ok(hooks)
    }

    fn restore(&mut self) -> Result<()> {
        let mut first_error = None;

        if let Some(hook) = self.mouse.take()
            && let Err(error) = unsafe { UnhookWindowsHookEx(hook.0) }
        {
            first_error = Some(Error::HookStopFailed(format!(
                "Failed to remove Windows mouse hook: {error}"
            )));
        }
        if let Ok(mut published) = MOUSE_HOOK.lock() {
            *published = None;
        }

        if let Some(hook) = self.keyboard.take()
            && let Err(error) = unsafe { UnhookWindowsHookEx(hook.0) }
            && first_error.is_none()
        {
            first_error = Some(Error::HookStopFailed(format!(
                "Failed to remove Windows keyboard hook: {error}"
            )));
        }
        if let Ok(mut published) = KEYBOARD_HOOK.lock() {
            *published = None;
        }

        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for InstalledHooks {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseMoveRoute {
    Legacy,
    RawPhysical,
    Injected,
}

fn mouse_move_route(grab_mode: bool, grab_ready: bool, injected: bool) -> MouseMoveRoute {
    if !grab_mode || !grab_ready {
        MouseMoveRoute::Legacy
    } else if injected {
        MouseMoveRoute::Injected
    } else {
        MouseMoveRoute::RawPhysical
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MotionReplay {
    Relative { delta_x: f64, delta_y: f64 },
    Absolute { x: f64, y: f64 },
}

fn replay_for_accepted_event(event: &Event) -> Option<MotionReplay> {
    match motion_from_event(event) {
        Some(Motion::Relative { delta_x, delta_y }) => {
            Some(MotionReplay::Relative { delta_x, delta_y })
        }
        Some(Motion::Absolute { x, y }) => Some(MotionReplay::Absolute { x, y }),
        None => None,
    }
}

fn accepted_replay_for_raw_motion<H: GrabHandler + ?Sized>(
    handler: &H,
    motion: RawMouseMotion,
    absolute_point: (f64, f64),
    desktop_bounds: DesktopBounds,
    dragging: bool,
) -> Option<MotionReplay> {
    let event = raw_input::event_from_motion(motion, absolute_point, desktop_bounds, dragging)?;
    handler.handle_event(&event)?;
    replay_for_accepted_event(&event)
}

fn handle_raw_motion<H: GrabHandler + ?Sized>(
    handler: &H,
    motion: RawMouseMotion,
    absolute_point: (f64, f64),
    desktop_bounds: DesktopBounds,
) -> Result<()> {
    match accepted_replay_for_raw_motion(
        handler,
        motion,
        absolute_point,
        desktop_bounds,
        state::is_button_held(),
    ) {
        Some(MotionReplay::Relative { delta_x, delta_y }) => {
            simulate::replay_mouse_move_relative(delta_x, delta_y)?;
        }
        Some(MotionReplay::Absolute { x, y }) => {
            simulate::replay_mouse_move_absolute(x, y)?;
        }
        None => {}
    }
    Ok(())
}

fn stop_requested() -> bool {
    STOP_FLAG
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|flag| !flag.load(Ordering::SeqCst)))
        .unwrap_or(false)
}

fn latest_physical_point_or_cursor() -> Result<(f64, f64)> {
    if let Ok(point) = LATEST_PHYSICAL_POINT.lock()
        && let Some((x, y)) = *point
    {
        return Ok((x as f64, y as f64));
    }
    simulate::mouse_position()
}

fn desktop_bounds_for(motion: RawMouseMotion) -> Result<DesktopBounds> {
    let bounds = match motion {
        RawMouseMotion::Relative { .. } => DesktopBounds::default(),
        RawMouseMotion::Absolute {
            virtual_desktop: true,
            ..
        } => DesktopBounds {
            x: unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
            y: unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
            width: unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) },
            height: unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) },
        },
        RawMouseMotion::Absolute {
            virtual_desktop: false,
            ..
        } => DesktopBounds {
            x: 0,
            y: 0,
            width: unsafe { GetSystemMetrics(SM_CXSCREEN) },
            height: unsafe { GetSystemMetrics(SM_CYSCREEN) },
        },
    };

    if matches!(motion, RawMouseMotion::Absolute { .. })
        && (bounds.width <= 0 || bounds.height <= 0)
    {
        Err(Error::Platform(
            "Windows returned invalid desktop bounds for Raw Input".into(),
        ))
    } else {
        Ok(bounds)
    }
}

fn run_listen_message_loop() -> Result<()> {
    let mut message = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        match status {
            -1 => {
                return Err(Error::Platform(format!(
                    "GetMessageW failed for Windows input hook: {}",
                    windows::core::Error::from_win32()
                )));
            }
            0 => return Ok(()),
            _ => {}
        }
        if stop_requested() {
            return Ok(());
        }
        unsafe {
            DispatchMessageW(&message);
        }
    }
}

fn run_grab_message_loop(raw_mouse: &RawMouseInput) -> Result<()> {
    let mut message = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        match status {
            -1 => {
                return Err(Error::Platform(format!(
                    "GetMessageW failed for Windows grab hook: {}",
                    windows::core::Error::from_win32()
                )));
            }
            0 => return Ok(()),
            _ => {}
        }
        if stop_requested() {
            return Ok(());
        }

        if message.message == WM_INPUT && message.hwnd == raw_mouse.window() {
            let motion = raw_mouse.read(message.lParam);
            unsafe {
                DispatchMessageW(&message);
            }
            if let Some(motion) = motion? {
                let point = latest_physical_point_or_cursor()?;
                let bounds = desktop_bounds_for(motion)?;
                let handler = GRAB_HANDLER.lock().map_err(|_| {
                    Error::ThreadError("Windows grab handler mutex poisoned".into())
                })?;
                if let Some(handler) = handler.as_ref() {
                    handle_raw_motion(handler.as_ref(), motion, point, bounds)?;
                }
            }
            continue;
        }

        unsafe {
            DispatchMessageW(&message);
        }
    }
}

/// Update modifier mask from keyboard event
fn update_key_modifier(code: u32, pressed: bool) {
    let mask = match code {
        0xA0 | 0xA1 => MASK_SHIFT, // VK_LSHIFT, VK_RSHIFT
        0xA2 | 0xA3 => MASK_CTRL,  // VK_LCONTROL, VK_RCONTROL
        0xA4 | 0xA5 => MASK_ALT,   // VK_LMENU, VK_RMENU
        0x5B | 0x5C => MASK_META,  // VK_LWIN, VK_RWIN
        _ => return,
    };

    if pressed {
        state::set_mask(mask);
    } else {
        state::unset_mask(mask);
    }
}

/// Get VK code from KBDLLHOOKSTRUCT
unsafe fn get_vk_code(lpdata: LPARAM) -> u32 {
    let kb = unsafe { *(lpdata.0 as *const KBDLLHOOKSTRUCT) };
    kb.vkCode
}

/// Get point from MSLLHOOKSTRUCT
unsafe fn get_mouse_point(lpdata: LPARAM) -> (i32, i32) {
    let mouse = unsafe { *(lpdata.0 as *const MSLLHOOKSTRUCT) };
    (mouse.pt.x, mouse.pt.y)
}

/// Get wheel delta from MSLLHOOKSTRUCT
unsafe fn get_wheel_delta(lpdata: LPARAM) -> i16 {
    let mouse = unsafe { *(lpdata.0 as *const MSLLHOOKSTRUCT) };
    ((mouse.mouseData >> 16) & 0xFFFF) as i16
}

/// Get X button code from MSLLHOOKSTRUCT
unsafe fn get_xbutton_code(lpdata: LPARAM) -> u8 {
    let mouse = unsafe { *(lpdata.0 as *const MSLLHOOKSTRUCT) };
    ((mouse.mouseData >> 16) & 0xFFFF) as u8
}

/// Convert Windows message to our Event type
unsafe fn convert_event(wparam: WPARAM, lparam: LPARAM) -> Option<Event> {
    let msg = wparam.0 as u32;

    let origin = match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
            let keyboard = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            provenance::keyboard_event_origin(keyboard)
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN
        | WM_MBUTTONUP | WM_XBUTTONDOWN | WM_XBUTTONUP | WM_MOUSEMOVE | WM_MOUSEWHEEL
        | WM_MOUSEHWHEEL => {
            let mouse = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            provenance::mouse_event_origin(mouse)
        }
        _ => return None,
    };

    let event = match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let code = unsafe { get_vk_code(lparam) };
            update_key_modifier(code, true);
            let key = keycode_to_key(code as u16);
            Some(Event::key_pressed(key, code))
        }

        WM_KEYUP | WM_SYSKEYUP => {
            let code = unsafe { get_vk_code(lparam) };
            update_key_modifier(code, false);
            let key = keycode_to_key(code as u16);
            Some(Event::key_released(key, code))
        }

        WM_LBUTTONDOWN => {
            state::set_mask(MASK_BUTTON1);
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_pressed(Button::Left, x as f64, y as f64))
        }

        WM_LBUTTONUP => {
            state::unset_mask(MASK_BUTTON1);
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_released(Button::Left, x as f64, y as f64))
        }

        WM_RBUTTONDOWN => {
            state::set_mask(MASK_BUTTON2);
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_pressed(Button::Right, x as f64, y as f64))
        }

        WM_RBUTTONUP => {
            state::unset_mask(MASK_BUTTON2);
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_released(Button::Right, x as f64, y as f64))
        }

        WM_MBUTTONDOWN => {
            state::set_mask(MASK_BUTTON3);
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_pressed(Button::Middle, x as f64, y as f64))
        }

        WM_MBUTTONUP => {
            state::unset_mask(MASK_BUTTON3);
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_released(Button::Middle, x as f64, y as f64))
        }

        WM_XBUTTONDOWN => {
            let xbutton = unsafe { get_xbutton_code(lparam) };
            let (button, mask) = match xbutton {
                1 => (Button::Button4, MASK_BUTTON4),
                2 => (Button::Button5, MASK_BUTTON5),
                _ => (Button::Unknown(xbutton), 0),
            };
            if mask != 0 {
                state::set_mask(mask);
            }
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_pressed(button, x as f64, y as f64))
        }

        WM_XBUTTONUP => {
            let xbutton = unsafe { get_xbutton_code(lparam) };
            let (button, mask) = match xbutton {
                1 => (Button::Button4, MASK_BUTTON4),
                2 => (Button::Button5, MASK_BUTTON5),
                _ => (Button::Unknown(xbutton), 0),
            };
            if mask != 0 {
                state::unset_mask(mask);
            }
            let (x, y) = unsafe { get_mouse_point(lparam) };
            Some(Event::mouse_released(button, x as f64, y as f64))
        }

        WM_MOUSEMOVE => {
            let (x, y) = unsafe { get_mouse_point(lparam) };
            // THE KEY FIX: Check button state for drag detection
            if state::is_button_held() {
                Some(Event::mouse_dragged(x as f64, y as f64))
            } else {
                Some(Event::mouse_moved(x as f64, y as f64))
            }
        }

        WM_MOUSEWHEEL => {
            let (x, y) = unsafe { get_mouse_point(lparam) };
            let delta = unsafe { get_wheel_delta(lparam) };
            let delta_units = delta as f64 / WHEEL_DELTA as f64;
            let (direction, abs_delta) = if delta > 0 {
                (ScrollDirection::Up, delta_units)
            } else {
                (ScrollDirection::Down, -delta_units)
            };
            Some(Event::mouse_wheel(x as f64, y as f64, direction, abs_delta))
        }

        WM_MOUSEHWHEEL => {
            let (x, y) = unsafe { get_mouse_point(lparam) };
            let delta = unsafe { get_wheel_delta(lparam) };
            let delta_units = delta as f64 / WHEEL_DELTA as f64;
            let (direction, abs_delta) = if delta > 0 {
                (ScrollDirection::Right, delta_units)
            } else {
                (ScrollDirection::Left, -delta_units)
            };
            Some(Event::mouse_wheel(x as f64, y as f64, direction, abs_delta))
        }

        _ => None,
    };

    event.map(|mut event| {
        event.origin = origin;
        event
    })
}

/// Keyboard hook callback
unsafe extern "system" fn keyboard_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // Check stop flag
        if let Ok(guard) = STOP_FLAG.lock()
            && let Some(ref flag) = *guard
            && !flag.load(Ordering::SeqCst)
        {
            // Stop requested
            if let Ok(thread_id) = THREAD_ID.lock() {
                let _ = unsafe { PostThreadMessageW(*thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            }
        }

        if let Some(event) = unsafe { convert_event(wparam, lparam) } {
            // Check if we're in grab mode
            if GRAB_MODE.load(Ordering::SeqCst) {
                if let Ok(guard) = GRAB_HANDLER.lock()
                    && let Some(ref handler) = *guard
                    && handler.handle_event(&event).is_none()
                {
                    // Handler returned None - consume the event
                    return LRESULT(1);
                }
            } else {
                // Listen mode: just dispatch
                if let Ok(guard) = HANDLER.lock()
                    && let Some(ref handler) = *guard
                {
                    handler.handle_event(&event);
                }
            }
        }
    }

    let hook = KEYBOARD_HOOK.lock().ok().and_then(|g| g.map(|h| h.0));
    unsafe { CallNextHookEx(hook, code, wparam, lparam) }
}

/// Mouse hook callback
unsafe extern "system" fn mouse_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // Check stop flag (same as keyboard callback)
        if let Ok(guard) = STOP_FLAG.lock()
            && let Some(ref flag) = *guard
            && !flag.load(Ordering::SeqCst)
        {
            // Stop requested
            if let Ok(thread_id) = THREAD_ID.lock() {
                let _ = unsafe { PostThreadMessageW(*thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            }
        }

        if wparam.0 as u32 == WM_MOUSEMOVE {
            let mouse = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            if provenance::is_grab_replay(mouse) {
                return unsafe { call_next_mouse_hook(code, wparam, lparam) };
            }
            let injected = mouse.flags & LLMHF_INJECTED != 0;
            if mouse_move_route(
                GRAB_MODE.load(Ordering::SeqCst),
                GRAB_READY.load(Ordering::SeqCst),
                injected,
            ) == MouseMoveRoute::RawPhysical
            {
                if let Ok(mut point) = LATEST_PHYSICAL_POINT.lock() {
                    *point = Some((mouse.pt.x, mouse.pt.y));
                }
                return LRESULT(1);
            }
        }

        if let Some(event) = unsafe { convert_event(wparam, lparam) } {
            // Check if we're in grab mode
            if GRAB_MODE.load(Ordering::SeqCst) {
                if let Ok(guard) = GRAB_HANDLER.lock()
                    && let Some(ref handler) = *guard
                    && handler.handle_event(&event).is_none()
                {
                    // Handler returned None - consume the event
                    return LRESULT(1);
                }
            } else {
                // Listen mode: just dispatch
                if let Ok(guard) = HANDLER.lock()
                    && let Some(ref handler) = *guard
                {
                    handler.handle_event(&event);
                }
            }
        }
    }

    unsafe { call_next_mouse_hook(code, wparam, lparam) }
}

unsafe fn call_next_mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let hook = MOUSE_HOOK.lock().ok().and_then(|g| g.map(|h| h.0));
    unsafe { CallNextHookEx(hook, code, wparam, lparam) }
}

/// Run the event hook (blocking).
pub fn run_hook<H: EventHandler + 'static>(running: &Arc<AtomicBool>, handler: H) -> Result<()> {
    let _session = ActiveSession::claim()?;
    provenance::initialize()?;
    let _callbacks = CallbackState::listen(running, handler)?;
    let mut hooks = InstalledHooks::install()?;

    // Send hook enabled event
    {
        if let Ok(guard) = HANDLER.lock()
            && let Some(ref handler) = *guard
        {
            handler.handle_event(&Event::hook_enabled());
        }
    }

    let mut result = run_listen_message_loop();
    let hook_cleanup = hooks.restore();
    if result.is_ok() {
        result = hook_cleanup;
    }

    // Send hook disabled event
    {
        if let Ok(guard) = HANDLER.lock()
            && let Some(ref handler) = *guard
        {
            handler.handle_event(&Event::hook_disabled());
        }
    }

    result
}

/// Run the event hook with grab capability (blocking).
///
/// Similar to `run_hook`, but allows the handler to consume events by returning `None`.
pub fn run_grab_hook<H: GrabHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()> {
    let _session = ActiveSession::claim()?;
    provenance::initialize()?;
    let _callbacks = CallbackState::grab(running, handler)?;
    let mut raw_mouse = RawMouseInput::acquire()?;
    let mut hooks = InstalledHooks::install()?;
    let mut enabled = false;

    let mut result = (|| -> Result<()> {
        raw_mouse.drain_pending()?;
        GRAB_READY.store(true, Ordering::SeqCst);

        if let Ok(guard) = GRAB_HANDLER.lock()
            && let Some(ref handler) = *guard
        {
            let _ = handler.handle_event(&Event::hook_enabled());
        }
        enabled = true;

        run_grab_message_loop(&raw_mouse)
    })();

    GRAB_READY.store(false, Ordering::SeqCst);
    let hook_cleanup = hooks.restore();
    if result.is_ok() {
        result = hook_cleanup;
    }
    let raw_cleanup = raw_mouse.restore();
    if result.is_ok() {
        result = raw_cleanup;
    }

    if enabled
        && let Ok(guard) = GRAB_HANDLER.lock()
        && let Some(ref handler) = *guard
    {
        let _ = handler.handle_event(&Event::hook_disabled());
    }

    result
}

/// Stop the event hook.
pub fn stop_hook() -> Result<()> {
    if let Ok(thread_id) = THREAD_ID.lock()
        && *thread_id != 0
    {
        unsafe {
            let _ = PostThreadMessageW(*thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InjectorIdentity, InputOrigin};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{
        KBDLLHOOKSTRUCT_FLAGS, LLKHF_INJECTED, LLMHF_INJECTED,
    };

    fn this_session_origin() -> InputOrigin {
        InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        }
    }

    #[test]
    fn convert_keyboard_event_preserves_this_monio_session_origin() {
        let raw_event = KBDLLHOOKSTRUCT {
            vkCode: 0x41,
            scanCode: 0,
            flags: KBDLLHOOKSTRUCT_FLAGS(LLKHF_INJECTED.0),
            time: 0,
            dwExtraInfo: super::super::provenance::session_tag()
                .expect("session tag should initialize"),
        };

        let converted = unsafe {
            convert_event(
                WPARAM(WM_KEYDOWN as usize),
                LPARAM((&raw_event as *const KBDLLHOOKSTRUCT) as isize),
            )
            .expect("keyboard event should convert")
        };

        assert_eq!(converted.origin, this_session_origin());
    }

    #[test]
    fn convert_mouse_event_preserves_this_monio_session_origin() {
        let raw_event = MSLLHOOKSTRUCT {
            pt: POINT { x: 100, y: 200 },
            mouseData: 0,
            flags: LLMHF_INJECTED,
            time: 0,
            dwExtraInfo: super::super::provenance::session_tag()
                .expect("session tag should initialize"),
        };

        let converted = unsafe {
            convert_event(
                WPARAM(WM_MOUSEMOVE as usize),
                LPARAM((&raw_event as *const MSLLHOOKSTRUCT) as isize),
            )
            .expect("mouse event should convert")
        };

        assert_eq!(converted.origin, this_session_origin());
    }

    #[test]
    fn backend_session_guard_rejects_overlap_and_releases_on_drop() {
        let first = ActiveSession::claim().expect("first session should claim");
        assert!(matches!(ActiveSession::claim(), Err(Error::AlreadyRunning)));
        drop(first);
        ActiveSession::claim().expect("claim should be reusable after drop");
    }

    #[test]
    fn physical_grab_motion_uses_raw_input_path_only_when_ready() {
        assert_eq!(
            mouse_move_route(false, false, false),
            MouseMoveRoute::Legacy
        );
        assert_eq!(mouse_move_route(true, false, false), MouseMoveRoute::Legacy);
        assert_eq!(
            mouse_move_route(true, true, false),
            MouseMoveRoute::RawPhysical
        );
        assert_eq!(mouse_move_route(true, true, true), MouseMoveRoute::Injected);
    }

    #[test]
    fn accepted_relative_event_selects_relative_replay() {
        let event = Event::mouse_moved_relative(100.0, 200.0, 8.0, -3.0);

        assert_eq!(
            replay_for_accepted_event(&event),
            Some(MotionReplay::Relative {
                delta_x: 8.0,
                delta_y: -3.0,
            })
        );
    }

    #[test]
    fn accepted_absolute_event_selects_absolute_replay() {
        let event = Event::mouse_moved(100.0, 200.0);

        assert_eq!(
            replay_for_accepted_event(&event),
            Some(MotionReplay::Absolute { x: 100.0, y: 200.0 })
        );
    }

    #[test]
    fn consumed_raw_motion_calls_handler_once_without_replay() {
        use super::super::raw_input::{DesktopBounds, RawMouseMotion};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = AtomicUsize::new(0);
        let handler = |_: &Event| {
            calls.fetch_add(1, Ordering::SeqCst);
            None
        };

        let replay = accepted_replay_for_raw_motion(
            &handler,
            RawMouseMotion::Relative {
                delta_x: 4,
                delta_y: -2,
            },
            (100.0, 200.0),
            DesktopBounds::default(),
            false,
        );

        assert_eq!(replay, None);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn accepted_raw_motion_replays_the_original_relative_delta() {
        use super::super::raw_input::{DesktopBounds, RawMouseMotion};

        let handler = |event: &Event| Some(event.clone());
        let replay = accepted_replay_for_raw_motion(
            &handler,
            RawMouseMotion::Relative {
                delta_x: 4,
                delta_y: -2,
            },
            (100.0, 200.0),
            DesktopBounds::default(),
            false,
        );

        assert_eq!(
            replay,
            Some(MotionReplay::Relative {
                delta_x: 4.0,
                delta_y: -2.0,
            })
        );
    }
}
