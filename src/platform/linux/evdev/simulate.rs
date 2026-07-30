//! Linux evdev input simulation using uinput.
//!
//! Creates a virtual input device to inject keyboard and mouse events.

#![allow(dead_code)]

use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType};
use crate::keycode::Key;
use crate::platform::linux::evdev::provenance::InjectorDeviceIdentity;
use crate::platform::linux::keycodes::key_to_evdev_keycode;
use crate::platform::motion::{Motion, motion_from_event};
use evdev::{
    AttributeSet, EventType as EvdevEventType, InputEvent, Key as EvdevKey, RelativeAxisType,
    uinput::{VirtualDevice, VirtualDeviceBuilder},
};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

const DEVICE_NODE_ATTEMPTS: usize = 100;
const DEVICE_NODE_RETRY_DELAY: Duration = Duration::from_millis(10);

struct VirtualDeviceState {
    device: VirtualDevice,
    identity: InjectorDeviceIdentity,
}

/// Process-scoped virtual device for simulation and grab pass-through.
static VIRTUAL_DEVICE: Mutex<Option<VirtualDeviceState>> = Mutex::new(None);

/// Emit raw input events directly (for grab mode re-injection).
/// This is an internal function used by the grab mode to pass through events.
pub(crate) fn emit_event(ev: &InputEvent) -> Result<()> {
    let mut guard = get_virtual_device()?;
    let state = guard
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

    state
        .device
        .emit(&events)
        .map_err(|e| Error::SimulateFailed(format!("Failed to emit event: {}", e)))?;

    Ok(())
}

pub(super) fn initialize() -> Result<InjectorDeviceIdentity> {
    let guard = get_virtual_device()?;
    guard
        .as_ref()
        .map(|state| state.identity.clone())
        .ok_or_else(|| Error::SimulateFailed("Virtual device not initialized".into()))
}

fn resolve_device_identity(device: &mut VirtualDevice) -> Result<InjectorDeviceIdentity> {
    resolve_device_identity_with_retry(
        || {
            let event_nodes = device
                .enumerate_dev_nodes_blocking()
                .and_then(|nodes| nodes.collect::<std::io::Result<Vec<_>>>());

            event_nodes.and_then(|nodes| InjectorDeviceIdentity::from_event_nodes(&nodes))
        },
        DEVICE_NODE_ATTEMPTS,
        DEVICE_NODE_RETRY_DELAY,
    )
}

fn resolve_device_identity_with_retry<F>(
    mut resolve: F,
    attempts: usize,
    retry_delay: Duration,
) -> Result<InjectorDeviceIdentity>
where
    F: FnMut() -> std::io::Result<InjectorDeviceIdentity>,
{
    let mut last_error = None;

    for attempt in 0..attempts {
        match resolve() {
            Ok(identity) => return Ok(identity),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                last_error = Some(error);
            }
            Err(error) => {
                return Err(Error::SimulateFailed(format!(
                    "Failed to inspect the Monio uinput device node: {}",
                    error
                )));
            }
        }

        if attempt + 1 < attempts {
            thread::sleep(retry_delay);
        }
    }

    match last_error {
        Some(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            Err(Error::PermissionDenied(format!(
                "Cannot open the Monio uinput device node after waiting for udev permissions: {}. \
                 Make sure the current user can read /dev/input/event*.",
                error
            )))
        }
        last_error => Err(Error::SimulateFailed(format!(
            "Failed to resolve Monio uinput device identity: {}",
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "no input device node appeared".into())
        ))),
    }
}

/// Get or create the process-scoped virtual device.
fn get_virtual_device() -> Result<std::sync::MutexGuard<'static, Option<VirtualDeviceState>>> {
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

        let mut device = VirtualDeviceBuilder::new()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    Error::PermissionDenied(format!(
                        "Cannot access /dev/uinput: {}. Make sure the current user has \
                         appropriate udev permissions.",
                        e
                    ))
                } else {
                    Error::SimulateFailed(format!("Failed to create virtual device builder: {}", e))
                }
            })?
            .name("monio session injector")
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

        let identity = resolve_device_identity(&mut device)?;
        *guard = Some(VirtualDeviceState { device, identity });
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
    let state = guard
        .as_mut()
        .ok_or_else(|| Error::SimulateFailed("Virtual device not initialized".into()))?;

    let value = if pressed { 1 } else { 0 };
    let events = [
        InputEvent::new(EvdevEventType::KEY, key.code(), value),
        // SYN_REPORT to flush
        InputEvent::new(EvdevEventType::SYNCHRONIZATION, 0, 0),
    ];

    state
        .device
        .emit(&events)
        .map_err(|e| Error::SimulateFailed(format!("Failed to emit key event: {}", e)))?;

    Ok(())
}

/// Emit a relative movement event
fn emit_relative(axis: RelativeAxisType, value: i32) -> Result<()> {
    let mut guard = get_virtual_device()?;
    let state = guard
        .as_mut()
        .ok_or_else(|| Error::SimulateFailed("Virtual device not initialized".into()))?;

    let events = [
        InputEvent::new(EvdevEventType::RELATIVE, axis.0, value),
        InputEvent::new(EvdevEventType::SYNCHRONIZATION, 0, 0),
    ];

    state
        .device
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
        EventType::MouseMoved | EventType::MouseDragged => match motion_from_event(event) {
            Some(Motion::Absolute { x, y }) => mouse_move(x, y)?,
            Some(Motion::Relative { delta_x, delta_y }) => {
                mouse_move_relative(delta_x, delta_y)?;
            }
            None => {}
        },
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
/// Get current mouse position.
///
/// Note: evdev does not support querying cursor position directly.
/// This function is not supported on the evdev backend.
pub fn mouse_position() -> Result<(f64, f64)> {
    Err(Error::NotSupported(
        "mouse_position is not supported on evdev backend. Use X11 backend instead.".into(),
    ))
}

/// Note: evdev uses relative motion, so we move by the delta.
/// For absolute positioning, the cursor needs to already be at (0,0)
/// or we need to track current position (which is complex).
pub fn mouse_move(x: f64, y: f64) -> Result<()> {
    mouse_move_relative(x, y)
}

/// Move the mouse by a relative offset.
pub fn mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    // For simplicity, we emit relative motion events
    // A full implementation would track current position and emit deltas
    emit_relative(RelativeAxisType::REL_X, delta_x as i32)?;
    emit_relative(RelativeAxisType::REL_Y, delta_y as i32)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn retries_injector_identity_while_udev_permissions_settle() {
        let mut attempts = 0;

        let identity = resolve_device_identity_with_retry(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
                } else {
                    InjectorDeviceIdentity::from_event_nodes(&[PathBuf::from("/dev/null")])
                }
            },
            3,
            Duration::ZERO,
        );

        assert!(identity.is_ok(), "{identity:?}");
        assert_eq!(attempts, 3);
    }
}
