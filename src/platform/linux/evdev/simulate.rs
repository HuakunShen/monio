//! Linux evdev input simulation using uinput.
//!
//! Creates a virtual input device to inject keyboard and mouse events.

#![allow(dead_code)]

use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType};
use crate::keycode::Key;
use crate::platform::linux::keycodes::key_to_evdev_keycode;
use evdev::{
    AttributeSet, EventType as EvdevEventType, InputEvent, Key as EvdevKey, RelativeAxisType,
    uinput::{VirtualDevice, VirtualDeviceBuilder},
};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// Lazy-initialized virtual device for simulation
static VIRTUAL_DEVICE: Mutex<Option<VirtualDevice>> = Mutex::new(None);

/// Emit raw input events directly (for grab mode re-injection).
/// This is an internal function used by the grab mode to pass through events.
pub(crate) fn emit_event(ev: &InputEvent) -> Result<()> {
    let mut guard = get_virtual_device()?;
    let device = guard
        .as_mut()
        .ok_or_else(|| Error::SimulateFailed("Virtual device not initialized".into()))?;

    // Create a new event with current timestamp - don't reuse the original event
    // as it may have stale timestamp or other metadata issues
    let event_type = ev.event_type();
    let code = ev.code();
    let value = ev.value();
    
    let events = [
        InputEvent::new(event_type, code, value),
        InputEvent::new(EvdevEventType::SYNCHRONIZATION, 0, 0),
    ];
    
    device
        .emit(&events)
        .map_err(|e| Error::SimulateFailed(format!("Failed to emit event: {}", e)))?;

    Ok(())
}

/// Get or create the virtual device
fn get_virtual_device() -> Result<std::sync::MutexGuard<'static, Option<VirtualDevice>>> {
    let mut guard = VIRTUAL_DEVICE
        .lock()
        .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;

    if guard.is_none() {
        // Create a virtual device with keyboard and mouse capabilities
        let mut keys = AttributeSet::<EvdevKey>::new();

        // Add common keys
        for code in 1..256 {
            let key = EvdevKey::new(code);
            keys.insert(key);
        }
        // Add mouse buttons
        keys.insert(EvdevKey::BTN_LEFT);
        keys.insert(EvdevKey::BTN_RIGHT);
        keys.insert(EvdevKey::BTN_MIDDLE);
        keys.insert(EvdevKey::BTN_SIDE);
        keys.insert(EvdevKey::BTN_EXTRA);

        let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
        rel_axes.insert(RelativeAxisType::REL_X);
        rel_axes.insert(RelativeAxisType::REL_Y);
        rel_axes.insert(RelativeAxisType::REL_WHEEL);
        rel_axes.insert(RelativeAxisType::REL_HWHEEL);

        let device = VirtualDeviceBuilder::new()
            .map_err(|e| {
                Error::SimulateFailed(format!("Failed to create virtual device builder: {}", e))
            })?
            .name("monio grab passthrough")
            .with_keys(&keys)
            .map_err(|e| Error::SimulateFailed(format!("Failed to add keys: {}", e)))?
            .with_relative_axes(&rel_axes)
            .map_err(|e| Error::SimulateFailed(format!("Failed to add relative axes: {}", e)))?
            .build()
            .map_err(|e| {
                Error::PermissionDenied(format!(
                    "Failed to create virtual device: {}. Make sure /dev/uinput is accessible \
                     (you may need to be in the 'input' group or have appropriate udev rules).",
                    e
                ))
            })?;

        *guard = Some(device);
    }

    Ok(guard)
}

/// Convert Button to evdev key code
fn button_to_evdev_key(button: Button) -> EvdevKey {
    match button {
        Button::Left => EvdevKey::BTN_LEFT,
        Button::Right => EvdevKey::BTN_RIGHT,
        Button::Middle => EvdevKey::BTN_MIDDLE,
        Button::Button4 => EvdevKey::BTN_SIDE,
        Button::Button5 => EvdevKey::BTN_EXTRA,
        Button::Unknown(_) => EvdevKey::BTN_LEFT, // Fallback
    }
}

/// Emit a key event
fn emit_key(key: EvdevKey, pressed: bool) -> Result<()> {
    let mut guard = get_virtual_device()?;
    let device = guard
        .as_mut()
        .ok_or_else(|| Error::SimulateFailed("Virtual device not initialized".into()))?;

    let value = if pressed { 1 } else { 0 };
    let events = [
        InputEvent::new(EvdevEventType::KEY, key.code(), value),
        // SYN_REPORT to flush
        InputEvent::new(EvdevEventType::SYNCHRONIZATION, 0, 0),
    ];

    device
        .emit(&events)
        .map_err(|e| Error::SimulateFailed(format!("Failed to emit key event: {}", e)))?;

    Ok(())
}

/// Emit a relative movement event
fn emit_relative(axis: RelativeAxisType, value: i32) -> Result<()> {
    let mut guard = get_virtual_device()?;
    let device = guard
        .as_mut()
        .ok_or_else(|| Error::SimulateFailed("Virtual device not initialized".into()))?;

    let events = [
        InputEvent::new(EvdevEventType::RELATIVE, axis.0, value),
        InputEvent::new(EvdevEventType::SYNCHRONIZATION, 0, 0),
    ];

    device
        .emit(&events)
        .map_err(|e| Error::SimulateFailed(format!("Failed to emit relative event: {}", e)))?;

    Ok(())
}

/// Simulate an event.
pub fn simulate(event: &Event) -> Result<()> {
    match event.event_type {
        EventType::KeyPressed => {
            if let Some(kb) = &event.keyboard {
                key_press(kb.key)?;
            }
        }
        EventType::KeyReleased => {
            if let Some(kb) = &event.keyboard {
                key_release(kb.key)?;
            }
        }
        EventType::MousePressed => {
            if let Some(mouse) = &event.mouse
                && let Some(button) = &mouse.button
            {
                mouse_press(*button)?;
            }
        }
        EventType::MouseReleased => {
            if let Some(mouse) = &event.mouse
                && let Some(button) = &mouse.button
            {
                mouse_release(*button)?;
            }
        }
        EventType::MouseMoved | EventType::MouseDragged => {
            if let Some(mouse) = &event.mouse {
                mouse_move(mouse.x, mouse.y)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Press a key.
pub fn key_press(key: Key) -> Result<()> {
    let code = key_to_evdev_keycode(key);
    let evdev_key = EvdevKey::new(code);
    emit_key(evdev_key, true)
}

/// Release a key.
pub fn key_release(key: Key) -> Result<()> {
    let code = key_to_evdev_keycode(key);
    let evdev_key = EvdevKey::new(code);
    emit_key(evdev_key, false)
}

/// Press and release a key.
pub fn key_tap(key: Key) -> Result<()> {
    key_press(key)?;
    thread::sleep(Duration::from_millis(10));
    key_release(key)
}

/// Press a mouse button.
pub fn mouse_press(button: Button) -> Result<()> {
    let evdev_key = button_to_evdev_key(button);
    emit_key(evdev_key, true)
}

/// Release a mouse button.
pub fn mouse_release(button: Button) -> Result<()> {
    let evdev_key = button_to_evdev_key(button);
    emit_key(evdev_key, false)
}

/// Click a mouse button (press and release).
pub fn mouse_click(button: Button) -> Result<()> {
    mouse_press(button)?;
    thread::sleep(Duration::from_millis(10));
    mouse_release(button)
}

/// Move the mouse to a position.
///
/// Note: evdev uses relative motion, so we move by the delta.
/// For absolute positioning, the cursor needs to already be at (0,0)
/// or we need to track current position (which is complex).
pub fn mouse_move(x: f64, y: f64) -> Result<()> {
    // For simplicity, we emit relative motion events
    // A full implementation would track current position and emit deltas
    emit_relative(RelativeAxisType::REL_X, x as i32)?;
    emit_relative(RelativeAxisType::REL_Y, y as i32)?;
    Ok(())
}
