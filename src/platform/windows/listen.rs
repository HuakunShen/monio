//! Windows input listening using SetWindowsHookEx.

use crate::error::{Error, Result};
use crate::event::{Button, Event, ScrollDirection};
use crate::hook::{EventHandler, GrabHandler};
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
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::keycodes::keycode_to_key;

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

    match msg {
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
    }
}

/// Keyboard hook callback
unsafe extern "system" fn keyboard_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // Check stop flag
        if let Ok(guard) = STOP_FLAG.lock() {
            if let Some(ref flag) = *guard {
                if !flag.load(Ordering::SeqCst) {
                    // Stop requested
                    if let Ok(thread_id) = THREAD_ID.lock() {
                        let _ = unsafe { PostThreadMessageW(*thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
                    }
                }
            }
        }

        if let Some(event) = unsafe { convert_event(wparam, lparam) } {
            // Check if we're in grab mode
            if GRAB_MODE.load(Ordering::SeqCst) {
                if let Ok(guard) = GRAB_HANDLER.lock() {
                    if let Some(ref handler) = *guard {
                        if handler.handle_event(&event).is_none() {
                            // Handler returned None - consume the event
                            return LRESULT(1);
                        }
                    }
                }
            } else {
                // Listen mode: just dispatch
                if let Ok(guard) = HANDLER.lock() {
                    if let Some(ref handler) = *guard {
                        handler.handle_event(&event);
                    }
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
        if let Ok(guard) = STOP_FLAG.lock() {
            if let Some(ref flag) = *guard {
                if !flag.load(Ordering::SeqCst) {
                    // Stop requested
                    if let Ok(thread_id) = THREAD_ID.lock() {
                        let _ = unsafe { PostThreadMessageW(*thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
                    }
                }
            }
        }

        if let Some(event) = unsafe { convert_event(wparam, lparam) } {
            // Check if we're in grab mode
            if GRAB_MODE.load(Ordering::SeqCst) {
                if let Ok(guard) = GRAB_HANDLER.lock() {
                    if let Some(ref handler) = *guard {
                        if handler.handle_event(&event).is_none() {
                            // Handler returned None - consume the event
                            return LRESULT(1);
                        }
                    }
                }
            } else {
                // Listen mode: just dispatch
                if let Ok(guard) = HANDLER.lock() {
                    if let Some(ref handler) = *guard {
                        handler.handle_event(&event);
                    }
                }
            }
        }
    }

    let hook = MOUSE_HOOK.lock().ok().and_then(|g| g.map(|h| h.0));
    unsafe { CallNextHookEx(hook, code, wparam, lparam) }
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

    // Store current thread ID for stopping
    {
        let mut tid = THREAD_ID
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *tid = unsafe { GetCurrentThreadId() };
    }

    // Set up keyboard hook
    let keyboard_hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_callback), None, 0)
            .map_err(|e| Error::HookStartFailed(format!("Failed to set keyboard hook: {}", e)))?
    };
    {
        let mut kh = KEYBOARD_HOOK
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *kh = Some(SendableHHOOK(keyboard_hook));
    }

    // Set up mouse hook
    let mouse_hook = unsafe {
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_callback), None, 0)
            .map_err(|e| Error::HookStartFailed(format!("Failed to set mouse hook: {}", e)))?
    };
    {
        let mut mh = MOUSE_HOOK
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *mh = Some(SendableHHOOK(mouse_hook));
    }

    // Send hook enabled event
    {
        if let Ok(guard) = HANDLER.lock() {
            if let Some(ref handler) = *guard {
                handler.handle_event(&Event::hook_enabled());
            }
        }
    }

    // Message loop
    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            // Check stop flag
            if let Ok(guard) = STOP_FLAG.lock() {
                if let Some(ref flag) = *guard {
                    if !flag.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
        }
    }

    // Send hook disabled event
    {
        if let Ok(guard) = HANDLER.lock() {
            if let Some(ref handler) = *guard {
                handler.handle_event(&Event::hook_disabled());
            }
        }
    }

    // Clean up hooks
    unsafe {
        if let Ok(mut kh) = KEYBOARD_HOOK.lock() {
            if let Some(hook) = kh.take() {
                let _ = UnhookWindowsHookEx(hook.0);
            }
        }
        if let Ok(mut mh) = MOUSE_HOOK.lock() {
            if let Some(hook) = mh.take() {
                let _ = UnhookWindowsHookEx(hook.0);
            }
        }
    }

    // Clean up handler
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

    Ok(())
}

/// Run the event hook with grab capability (blocking).
///
/// Similar to `run_hook`, but allows the handler to consume events by returning `None`.
pub fn run_grab_hook<H: GrabHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()> {
    // Store handler and stop flag
    {
        let mut h = GRAB_HANDLER
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

    // Enable grab mode
    GRAB_MODE.store(true, Ordering::SeqCst);

    // Store current thread ID for stopping
    {
        let mut tid = THREAD_ID
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *tid = unsafe { GetCurrentThreadId() };
    }

    // Set up keyboard hook
    let keyboard_hook = unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_callback), None, 0)
            .map_err(|e| Error::HookStartFailed(format!("Failed to set keyboard hook: {}", e)))?
    };
    {
        let mut kh = KEYBOARD_HOOK
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *kh = Some(SendableHHOOK(keyboard_hook));
    }

    // Set up mouse hook
    let mouse_hook = unsafe {
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_callback), None, 0)
            .map_err(|e| Error::HookStartFailed(format!("Failed to set mouse hook: {}", e)))?
    };
    {
        let mut mh = MOUSE_HOOK
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *mh = Some(SendableHHOOK(mouse_hook));
    }

    // Send hook enabled event
    {
        if let Ok(guard) = GRAB_HANDLER.lock() {
            if let Some(ref handler) = *guard {
                let _ = handler.handle_event(&Event::hook_enabled());
            }
        }
    }

    // Message loop
    let mut msg = windows::Win32::UI::WindowsAndMessaging::MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            // Check stop flag
            if let Ok(guard) = STOP_FLAG.lock() {
                if let Some(ref flag) = *guard {
                    if !flag.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
        }
    }

    // Send hook disabled event
    {
        if let Ok(guard) = GRAB_HANDLER.lock() {
            if let Some(ref handler) = *guard {
                let _ = handler.handle_event(&Event::hook_disabled());
            }
        }
    }

    // Clean up hooks
    unsafe {
        if let Ok(mut kh) = KEYBOARD_HOOK.lock() {
            if let Some(hook) = kh.take() {
                let _ = UnhookWindowsHookEx(hook.0);
            }
        }
        if let Ok(mut mh) = MOUSE_HOOK.lock() {
            if let Some(hook) = mh.take() {
                let _ = UnhookWindowsHookEx(hook.0);
            }
        }
    }

    // Clean up
    GRAB_MODE.store(false, Ordering::SeqCst);
    {
        let mut h = GRAB_HANDLER
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

    Ok(())
}

/// Stop the event hook.
pub fn stop_hook() -> Result<()> {
    if let Ok(thread_id) = THREAD_ID.lock() {
        if *thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(*thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
    }
    Ok(())
}
