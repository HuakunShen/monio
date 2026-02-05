//! macOS input listening using CGEventTap.

#![allow(improper_ctypes_definitions)]
#![allow(unsafe_op_in_unsafe_fn)]

use crate::error::{Error, Result};
use crate::event::{Button, Event, ScrollDirection};
use crate::hook::{EventHandler, GrabHandler};
use crate::state::{
    self, MASK_ALT, MASK_BUTTON1, MASK_BUTTON2, MASK_BUTTON3, MASK_BUTTON4, MASK_BUTTON5,
    MASK_CTRL, MASK_META, MASK_SHIFT,
};
use core::ptr::NonNull;
use objc2_core_foundation::{CFMachPort, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapCallBack, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType, kCGEventMaskForAllEvents,
};
use objc2_foundation::NSAutoreleasePool;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::keycodes::keycode_to_key;

/// Stored handler for the callback (listen mode)
static HANDLER: Mutex<Option<Box<dyn EventHandler>>> = Mutex::new(None);

/// Stored handler for the callback (grab mode)
static GRAB_HANDLER: Mutex<Option<Box<dyn GrabHandler>>> = Mutex::new(None);

/// Flag to signal the run loop to stop
static STOP_FLAG: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Last seen flags for detecting modifier key press/release
static LAST_FLAGS: Mutex<CGEventFlags> = Mutex::new(CGEventFlags(0));

/// Wrapper for raw pointer to CFMachPort that implements Send + Sync
/// Safety: The pointer is only accessed from the callback which runs on the same thread
struct TapPointer(*const CFMachPort);
unsafe impl Send for TapPointer {}
unsafe impl Sync for TapPointer {}

/// Stored event tap for timeout recovery
static EVENT_TAP: Mutex<Option<TapPointer>> = Mutex::new(None);

/// Flag indicating whether we're in grab mode
static GRAB_MODE: AtomicBool = AtomicBool::new(false);

#[link(name = "Cocoa", kind = "framework")]
unsafe extern "C" {}

/// Convert CGEventFlags to our modifier mask
fn flags_to_mask(flags: CGEventFlags) -> u32 {
    let mut mask = 0u32;

    if flags.contains(CGEventFlags::MaskShift) {
        mask |= MASK_SHIFT;
    }
    if flags.contains(CGEventFlags::MaskControl) {
        mask |= MASK_CTRL;
    }
    if flags.contains(CGEventFlags::MaskAlternate) {
        mask |= MASK_ALT;
    }
    if flags.contains(CGEventFlags::MaskCommand) {
        mask |= MASK_META;
    }

    mask
}

/// Update modifier mask from event flags
fn update_modifiers(flags: CGEventFlags) {
    let new_mods = flags_to_mask(flags);
    let current = state::get_mask();

    // Update only modifier bits, preserve button bits
    let buttons = current & state::MASK_ALL_BUTTONS;
    let new_mask = new_mods | buttons;

    // Clear all and set new
    state::reset_mask();
    state::set_mask(new_mask);
}

/// Get button mask for a button number
fn button_to_mask(button: i64) -> u32 {
    match button {
        0 => MASK_BUTTON1,
        1 => MASK_BUTTON2,
        2 => MASK_BUTTON3,
        3 => MASK_BUTTON4,
        4 => MASK_BUTTON5,
        _ => 0,
    }
}

/// Convert button number to Button enum
fn number_to_button(button: i64) -> Button {
    match button {
        0 => Button::Left,
        1 => Button::Right,
        2 => Button::Middle,
        3 => Button::Button4,
        4 => Button::Button5,
        n => Button::Unknown(n as u8),
    }
}

/// The CGEventTap callback
unsafe extern "C-unwind" fn event_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    cg_event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    // Check if we should stop
    if let Ok(guard) = STOP_FLAG.lock()
        && let Some(ref flag) = *guard
        && !flag.load(Ordering::SeqCst)
    {
        if let Some(run_loop) = CFRunLoop::current() {
            run_loop.stop();
        }
        return cg_event.as_ptr();
    }

    // Handle event tap timeout - macOS disables the tap if callback takes too long
    // Re-enable it to maintain hook functionality (matches libumonio behavior)
    if event_type == CGEventType::TapDisabledByTimeout
        || event_type == CGEventType::TapDisabledByUserInput
    {
        if let Ok(guard) = EVENT_TAP.lock()
            && let Some(ref tap_ptr) = *guard
        {
            log::warn!("Event tap was disabled (timeout or user input), re-enabling...");
            if !tap_ptr.0.is_null() {
                CGEvent::tap_enable(&*tap_ptr.0, true);
            }
        }
        return cg_event.as_ptr();
    }

    // Get event flags and update modifier state
    let flags = CGEvent::flags(Some(cg_event.as_ref()));
    update_modifiers(flags);

    let event = convert_event(event_type, cg_event);

    // Check if we're in grab mode
    if GRAB_MODE.load(Ordering::SeqCst) {
        // Grab mode: handler decides whether to consume event
        if let Some(evt) = event
            && let Ok(guard) = GRAB_HANDLER.lock()
            && let Some(ref handler) = *guard
            && handler.handle_event(&evt).is_none()
        {
            // Handler returned None - consume the event
            return null_mut();
        }
    } else {
        // Listen mode: just dispatch, always pass through
        if let Some(evt) = event
            && let Ok(guard) = HANDLER.lock()
            && let Some(ref handler) = *guard
        {
            handler.handle_event(&evt);
        }
    }

    cg_event.as_ptr()
}

/// Convert a CGEvent to our Event type
unsafe fn convert_event(event_type: CGEventType, cg_event: NonNull<CGEvent>) -> Option<Event> {
    match event_type {
        CGEventType::KeyDown => {
            let code = CGEvent::integer_value_field(
                Some(cg_event.as_ref()),
                CGEventField::KeyboardEventKeycode,
            );
            let key = keycode_to_key(code as u16);
            Some(Event::key_pressed(key, code as u32))
        }

        CGEventType::KeyUp => {
            let code = CGEvent::integer_value_field(
                Some(cg_event.as_ref()),
                CGEventField::KeyboardEventKeycode,
            );
            let key = keycode_to_key(code as u16);
            Some(Event::key_released(key, code as u32))
        }

        CGEventType::FlagsChanged => {
            let code = CGEvent::integer_value_field(
                Some(cg_event.as_ref()),
                CGEventField::KeyboardEventKeycode,
            );
            let key = keycode_to_key(code as u16);
            let flags = CGEvent::flags(Some(cg_event.as_ref()));

            // Determine if this is a press or release based on flag changes
            let mut last_flags = LAST_FLAGS.lock().ok()?;
            let is_press = if flags.contains(CGEventFlags::MaskShift)
                && !last_flags.contains(CGEventFlags::MaskShift)
            {
                *last_flags = flags;
                true
            } else if !flags.contains(CGEventFlags::MaskShift)
                && last_flags.contains(CGEventFlags::MaskShift)
            {
                *last_flags = flags;
                false
            } else if flags.contains(CGEventFlags::MaskControl)
                && !last_flags.contains(CGEventFlags::MaskControl)
            {
                *last_flags = flags;
                true
            } else if !flags.contains(CGEventFlags::MaskControl)
                && last_flags.contains(CGEventFlags::MaskControl)
            {
                *last_flags = flags;
                false
            } else if flags.contains(CGEventFlags::MaskAlternate)
                && !last_flags.contains(CGEventFlags::MaskAlternate)
            {
                *last_flags = flags;
                true
            } else if !flags.contains(CGEventFlags::MaskAlternate)
                && last_flags.contains(CGEventFlags::MaskAlternate)
            {
                *last_flags = flags;
                false
            } else if flags.contains(CGEventFlags::MaskCommand)
                && !last_flags.contains(CGEventFlags::MaskCommand)
            {
                *last_flags = flags;
                true
            } else if !flags.contains(CGEventFlags::MaskCommand)
                && last_flags.contains(CGEventFlags::MaskCommand)
            {
                *last_flags = flags;
                false
            } else {
                return None;
            };

            if is_press {
                Some(Event::key_pressed(key, code as u32))
            } else {
                Some(Event::key_released(key, code as u32))
            }
        }

        CGEventType::LeftMouseDown => {
            state::set_mask(MASK_BUTTON1);
            let point = CGEvent::location(Some(cg_event.as_ref()));
            Some(Event::mouse_pressed(Button::Left, point.x, point.y))
        }

        CGEventType::LeftMouseUp => {
            state::unset_mask(MASK_BUTTON1);
            let point = CGEvent::location(Some(cg_event.as_ref()));
            Some(Event::mouse_released(Button::Left, point.x, point.y))
        }

        CGEventType::RightMouseDown => {
            state::set_mask(MASK_BUTTON2);
            let point = CGEvent::location(Some(cg_event.as_ref()));
            Some(Event::mouse_pressed(Button::Right, point.x, point.y))
        }

        CGEventType::RightMouseUp => {
            state::unset_mask(MASK_BUTTON2);
            let point = CGEvent::location(Some(cg_event.as_ref()));
            Some(Event::mouse_released(Button::Right, point.x, point.y))
        }

        CGEventType::OtherMouseDown => {
            let button_num = CGEvent::integer_value_field(
                Some(cg_event.as_ref()),
                CGEventField::MouseEventButtonNumber,
            );
            let mask = button_to_mask(button_num);
            if mask != 0 {
                state::set_mask(mask);
            }
            let button = number_to_button(button_num);
            let point = CGEvent::location(Some(cg_event.as_ref()));
            Some(Event::mouse_pressed(button, point.x, point.y))
        }

        CGEventType::OtherMouseUp => {
            let button_num = CGEvent::integer_value_field(
                Some(cg_event.as_ref()),
                CGEventField::MouseEventButtonNumber,
            );
            let mask = button_to_mask(button_num);
            if mask != 0 {
                state::unset_mask(mask);
            }
            let button = number_to_button(button_num);
            let point = CGEvent::location(Some(cg_event.as_ref()));
            Some(Event::mouse_released(button, point.x, point.y))
        }

        CGEventType::MouseMoved => {
            let point = CGEvent::location(Some(cg_event.as_ref()));
            // THE KEY FIX: Check button state for drag detection
            if state::is_button_held() {
                Some(Event::mouse_dragged(point.x, point.y))
            } else {
                Some(Event::mouse_moved(point.x, point.y))
            }
        }

        CGEventType::LeftMouseDragged
        | CGEventType::RightMouseDragged
        | CGEventType::OtherMouseDragged => {
            let point = CGEvent::location(Some(cg_event.as_ref()));
            Some(Event::mouse_dragged(point.x, point.y))
        }

        CGEventType::ScrollWheel => {
            let point = CGEvent::location(Some(cg_event.as_ref()));
            let delta_y = CGEvent::integer_value_field(
                Some(cg_event.as_ref()),
                CGEventField::ScrollWheelEventDeltaAxis1,
            );
            let delta_x = CGEvent::integer_value_field(
                Some(cg_event.as_ref()),
                CGEventField::ScrollWheelEventDeltaAxis2,
            );

            let (direction, delta) = if delta_y.abs() > delta_x.abs() {
                if delta_y > 0 {
                    (ScrollDirection::Up, delta_y as f64)
                } else {
                    (ScrollDirection::Down, -delta_y as f64)
                }
            } else if delta_x > 0 {
                (ScrollDirection::Left, delta_x as f64)
            } else {
                (ScrollDirection::Right, -delta_x as f64)
            };

            Some(Event::mouse_wheel(point.x, point.y, direction, delta))
        }

        _ => None,
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
    {
        let mut f = LAST_FLAGS
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *f = CGEventFlags(0);
    }

    unsafe {
        let _pool = NSAutoreleasePool::new();

        let callback: CGEventTapCallBack = Some(event_callback);
        let tap = CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            kCGEventMaskForAllEvents.into(),
            callback,
            null_mut(),
        )
        .ok_or_else(|| {
            Error::PermissionDenied(
                "Failed to create event tap. Make sure Accessibility permissions are granted."
                    .into(),
            )
        })?;

        // Store the tap reference for timeout recovery
        {
            let mut tap_guard = EVENT_TAP
                .lock()
                .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
            *tap_guard = Some(TapPointer(&*tap as *const CFMachPort));
        }

        let source = CFMachPort::new_run_loop_source(None, Some(&tap), 0)
            .ok_or_else(|| Error::HookStartFailed("Failed to create run loop source".into()))?;

        let current_loop = CFRunLoop::current()
            .ok_or_else(|| Error::HookStartFailed("Failed to get current run loop".into()))?;

        current_loop.add_source(Some(&source), kCFRunLoopCommonModes);

        // Enable the tap
        CGEvent::tap_enable(&tap, true);

        // Send hook enabled event
        {
            if let Ok(guard) = HANDLER.lock()
                && let Some(ref handler) = *guard
            {
                handler.handle_event(&Event::hook_enabled());
            }
        }

        // Run the loop
        CFRunLoop::run();

        // Send hook disabled event
        {
            if let Ok(guard) = HANDLER.lock()
                && let Some(ref handler) = *guard
            {
                handler.handle_event(&Event::hook_disabled());
            }
        }
    }

    // Clean up
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
        let mut t = EVENT_TAP
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *t = None;
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
    {
        let mut f = LAST_FLAGS
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *f = CGEventFlags(0);
    }

    // Enable grab mode
    GRAB_MODE.store(true, Ordering::SeqCst);

    unsafe {
        let _pool = NSAutoreleasePool::new();

        let callback: CGEventTapCallBack = Some(event_callback);
        // Use Default (not ListenOnly) to allow consuming events
        let tap = CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default, // Allows modification/consumption
            kCGEventMaskForAllEvents.into(),
            callback,
            null_mut(),
        )
        .ok_or_else(|| {
            Error::PermissionDenied(
                "Failed to create event tap. Make sure Accessibility permissions are granted."
                    .into(),
            )
        })?;

        // Store the tap reference for timeout recovery
        {
            let mut tap_guard = EVENT_TAP
                .lock()
                .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
            *tap_guard = Some(TapPointer(&*tap as *const CFMachPort));
        }

        let source = CFMachPort::new_run_loop_source(None, Some(&tap), 0)
            .ok_or_else(|| Error::HookStartFailed("Failed to create run loop source".into()))?;

        let current_loop = CFRunLoop::current()
            .ok_or_else(|| Error::HookStartFailed("Failed to get current run loop".into()))?;

        current_loop.add_source(Some(&source), kCFRunLoopCommonModes);

        // Enable the tap
        CGEvent::tap_enable(&tap, true);

        // Send hook enabled event
        {
            if let Ok(guard) = GRAB_HANDLER.lock()
                && let Some(ref handler) = *guard
            {
                let _ = handler.handle_event(&Event::hook_enabled());
            }
        }

        // Run the loop
        CFRunLoop::run();

        // Send hook disabled event
        {
            if let Ok(guard) = GRAB_HANDLER.lock()
                && let Some(ref handler) = *guard
            {
                let _ = handler.handle_event(&Event::hook_disabled());
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
    {
        let mut t = EVENT_TAP
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *t = None;
    }

    Ok(())
}

/// Stop the event hook.
pub fn stop_hook() -> Result<()> {
    if let Some(run_loop) = CFRunLoop::main() {
        run_loop.stop();
    }
    Ok(())
}
