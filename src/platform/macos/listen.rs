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

use super::{keycodes::keycode_to_key, provenance};

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

/// Wrapper for raw CFRunLoop pointer that implements Send + Sync.
/// Safety: CFRunLoopStop() is documented as thread-safe by Apple.
struct RunLoopRef(*const CFRunLoop);
unsafe impl Send for RunLoopRef {}
unsafe impl Sync for RunLoopRef {}

/// Stored reference to the hook thread's CFRunLoop, so `stop_hook()` can
/// stop the correct run loop instead of the main thread's.
static HOOK_RUN_LOOP: Mutex<Option<RunLoopRef>> = Mutex::new(None);

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

/// The device-dependent flag bit that belongs to one modifier keycode.
///
/// These are the `NX_DEVICE*` masks from IOKit's event system. Unlike
/// `MaskShift` and friends they distinguish the left key from the right one,
/// which is the whole point: a pair sharing one mask makes the first release
/// invisible while the second is still held.
///
/// `None` for keys that have no device-specific bit — Caps Lock and Fn — whose
/// general mask is unambiguous anyway because they are not a pair.
fn device_flag_for_keycode(code: u16) -> Option<u64> {
    Some(match code {
        0x38 => 0x0000_0002, // NX_DEVICELSHIFTKEYMASK
        0x3C => 0x0000_0004, // NX_DEVICERSHIFTKEYMASK
        0x3B => 0x0000_0001, // NX_DEVICELCTLKEYMASK
        0x3E => 0x0000_2000, // NX_DEVICERCTLKEYMASK
        0x3A => 0x0000_0020, // NX_DEVICELALTKEYMASK
        0x3D => 0x0000_0040, // NX_DEVICERALTKEYMASK
        0x37 => 0x0000_0008, // NX_DEVICELCMDKEYMASK
        0x36 => 0x0000_0010, // NX_DEVICERCMDKEYMASK
        _ => return None,
    })
}

fn mouse_motion_event(cg_event: &CGEvent, dragged: bool) -> Event {
    let point = CGEvent::location(Some(cg_event));
    let delta_x =
        CGEvent::integer_value_field(Some(cg_event), CGEventField::MouseEventDeltaX) as f64;
    let delta_y =
        CGEvent::integer_value_field(Some(cg_event), CGEventField::MouseEventDeltaY) as f64;

    if dragged {
        Event::mouse_dragged_relative(point.x, point.y, delta_x, delta_y)
    } else {
        Event::mouse_moved_relative(point.x, point.y, delta_x, delta_y)
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
    let event = match event_type {
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

            // Ask about THIS key's own bit, not the general mask.
            //
            // The general masks (`MaskShift`, `MaskControl`, …) are shared by
            // the left and right half of each pair, so while one half is held
            // the mask stays set and the other half's release is unreadable.
            // That is the mechanism behind a modifier that sticks on a remote
            // machine — measured on 2026-08-01, where releasing Shift left the
            // far Mac typing capitals forever.
            //
            // macOS also publishes device-dependent bits (the `NX_DEVICE*`
            // masks), which say exactly which physical key is down. Testing the
            // bit that belongs to the keycode in hand is exact, needs no
            // remembered previous state, and cannot be confused by the other
            // half of the pair.
            let bit = device_flag_for_keycode(code as u16);
            let is_press = match bit {
                Some(bit) => flags.0 & bit != 0,
                // No device-specific bit exists for these, so the general mask
                // IS the whole truth for them.
                None => match code {
                    0x39 => flags.contains(CGEventFlags::MaskAlphaShift), // Caps Lock
                    0x3F => flags.contains(CGEventFlags::MaskSecondaryFn), // Fn
                    _ => return None,
                },
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
            // THE KEY FIX: Check button state for drag detection
            Some(mouse_motion_event(
                cg_event.as_ref(),
                state::is_button_held(),
            ))
        }

        CGEventType::LeftMouseDragged
        | CGEventType::RightMouseDragged
        | CGEventType::OtherMouseDragged => Some(mouse_motion_event(cg_event.as_ref(), true)),

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
    };

    event.map(|mut event| {
        event.origin = provenance::event_origin(cg_event.as_ref());
        event
    })
}

/// Run the event hook (blocking).
pub fn run_hook<H: EventHandler + 'static>(running: &Arc<AtomicBool>, handler: H) -> Result<()> {
    provenance::initialize()?;

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

        // Store run loop reference so stop_hook() can stop the correct run loop
        {
            let mut rl = HOOK_RUN_LOOP
                .lock()
                .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
            *rl = Some(RunLoopRef(&*current_loop as *const CFRunLoop));
        }

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
        let mut rl = HOOK_RUN_LOOP
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *rl = None;
    }
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
    provenance::initialize()?;

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

        // Store run loop reference so stop_hook() can stop the correct run loop
        {
            let mut rl = HOOK_RUN_LOOP
                .lock()
                .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
            *rl = Some(RunLoopRef(&*current_loop as *const CFRunLoop));
        }

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
        let mut rl = HOOK_RUN_LOOP
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *rl = None;
    }
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

/// Stop the event hook by stopping the hook thread's run loop.
///
/// Previously this incorrectly called `CFRunLoop::main()` which returns the
/// main thread's run loop — not the background hook thread's. In Electron,
/// this would attempt to stop Chromium's main run loop.
pub fn stop_hook() -> Result<()> {
    if let Ok(guard) = HOOK_RUN_LOOP.lock()
        && let Some(ref rl) = *guard
        && !rl.0.is_null()
    {
        // Safety: CFRunLoopStop is thread-safe per Apple docs.
        // The pointer is valid because the hook thread's run loop is
        // still alive (Hook::stop sets the flag, calls us, then joins).
        unsafe {
            (&*rl.0).stop();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InjectorIdentity, InputOrigin, RelativeMotion};
    use objc2_core_foundation::CGPoint;
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGMouseButton};

    #[test]
    fn convert_event_preserves_this_monio_session_origin() {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .expect("CGEventSource should be available");
        let event = CGEvent::new_keyboard_event(Some(&source), 0, true)
            .expect("keyboard CGEvent should be available");
        super::super::provenance::tag_event(&event).expect("event should be tagged");

        let converted = unsafe {
            convert_event(CGEventType::KeyDown, NonNull::from(&*event))
                .expect("event should convert")
        };

        assert_eq!(
            converted.origin,
            InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession,
            }
        );
    }

    /// Left and right must be distinguishable, or the release of one is
    /// invisible while the other is held — the mechanism behind a modifier that
    /// sticks on a remote machine.
    #[test]
    fn each_half_of_a_modifier_pair_has_its_own_bit() {
        let left = device_flag_for_keycode(0x38).expect("left shift");
        let right = device_flag_for_keycode(0x3C).expect("right shift");
        assert_ne!(left, right, "a shared bit is exactly the bug");
        assert_eq!(left & right, 0, "the halves must not overlap");

        for (a, b) in [(0x3B, 0x3E), (0x3A, 0x3D), (0x37, 0x36)] {
            let a = device_flag_for_keycode(a).expect("left half");
            let b = device_flag_for_keycode(b).expect("right half");
            assert_eq!(a & b, 0, "modifier pair shares a bit");
        }
    }

    /// Holding one half must not make the other half look held.
    #[test]
    fn one_half_held_does_not_mask_the_other_half_s_release() {
        let left = device_flag_for_keycode(0x38).unwrap();
        let right = device_flag_for_keycode(0x3C).unwrap();
        // Both down, then the LEFT one comes up: flags still carry right.
        let after_left_release = right;
        assert_eq!(after_left_release & left, 0, "left reads as released");
        assert_ne!(after_left_release & right, 0, "right still reads as held");
    }

    /// Caps Lock and Fn are not pairs, so the general mask is the whole truth
    /// and they deliberately have no device bit.
    #[test]
    fn unpaired_modifiers_have_no_device_bit() {
        assert!(device_flag_for_keycode(0x39).is_none(), "caps lock");
        assert!(device_flag_for_keycode(0x3F).is_none(), "fn");
    }

    #[test]
    fn convert_mouse_event_preserves_absolute_position_and_relative_delta() {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .expect("CGEventSource should be available");
        let event = CGEvent::new_mouse_event(
            Some(&source),
            CGEventType::MouseMoved,
            CGPoint { x: 120.0, y: 240.0 },
            CGMouseButton::Left,
        )
        .expect("mouse CGEvent should be available");

        CGEvent::set_integer_value_field(Some(&event), CGEventField::MouseEventDeltaX, -9);
        CGEvent::set_integer_value_field(Some(&event), CGEventField::MouseEventDeltaY, 6);

        let converted = unsafe {
            convert_event(CGEventType::MouseMoved, NonNull::from(&*event))
                .expect("event should convert")
        };
        let mouse = converted
            .mouse
            .expect("motion event should contain mouse data");

        assert_eq!((mouse.x, mouse.y), (120.0, 240.0));
        assert_eq!(
            mouse.relative,
            Some(RelativeMotion {
                delta_x: -9.0,
                delta_y: 6.0,
            })
        );
    }
}
