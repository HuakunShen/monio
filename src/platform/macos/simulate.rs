//! macOS event simulation using CGEvent.

#![allow(unused_unsafe)]

use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType};
use crate::keycode::Key;
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit,
};
use std::sync::Mutex;

use super::keycodes::key_to_keycode;

/// Track the current modifier flags for simulation
static SIM_FLAGS: Mutex<CGEventFlags> = Mutex::new(CGEventFlags(0));

/// Get current mouse location
fn get_current_mouse_location() -> Result<CGPoint> {
    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;
        let event = CGEvent::new(Some(&source))
            .ok_or_else(|| Error::SimulateFailed("Failed to create event".into()))?;
        Ok(CGEvent::location(Some(&event)))
    }
}

/// Check if a key is a modifier key
fn is_modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::MetaLeft
            | Key::MetaRight
    )
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
                && let Some(button) = mouse.button
            {
                mouse_press(button)?;
            }
        }
        EventType::MouseReleased => {
            if let Some(mouse) = &event.mouse
                && let Some(button) = mouse.button
            {
                mouse_release(button)?;
            }
        }
        EventType::MouseMoved | EventType::MouseDragged => {
            if let Some(mouse) = &event.mouse {
                mouse_move(mouse.x, mouse.y)?;
            }
        }
        EventType::MouseWheel => {
            if let Some(wheel) = &event.wheel {
                mouse_scroll(wheel.delta as i32, 0)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Press a key.
pub fn key_press(key: Key) -> Result<()> {
    let keycode = key_to_keycode(key)
        .ok_or_else(|| Error::SimulateFailed(format!("Unsupported key: {:?}", key)))?;

    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;

        if is_modifier_key(key) {
            // For modifier keys, use FlagsChanged event type
            let event = CGEvent::new(Some(&source))
                .ok_or_else(|| Error::SimulateFailed("Failed to create event".into()))?;
            CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::KeyboardEventKeycode,
                keycode as i64,
            );

            // Update flags
            let mut flags = SIM_FLAGS
                .lock()
                .map_err(|_| Error::SimulateFailed("mutex poisoned".into()))?;
            match key {
                Key::ShiftLeft | Key::ShiftRight => {
                    flags.insert(CGEventFlags::MaskShift);
                }
                Key::ControlLeft | Key::ControlRight => {
                    flags.insert(CGEventFlags::MaskControl);
                }
                Key::AltLeft | Key::AltRight => {
                    flags.insert(CGEventFlags::MaskAlternate);
                }
                Key::MetaLeft | Key::MetaRight => {
                    flags.insert(CGEventFlags::MaskCommand);
                }
                _ => {}
            }
            CGEvent::set_flags(Some(&event), *flags);
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        } else {
            // For regular keys, use keyboard event
            let event = CGEvent::new_keyboard_event(Some(&source), keycode, true)
                .ok_or_else(|| Error::SimulateFailed("Failed to create keyboard event".into()))?;
            let flags = SIM_FLAGS
                .lock()
                .map_err(|_| Error::SimulateFailed("mutex poisoned".into()))?;
            CGEvent::set_flags(Some(&event), *flags);
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
    }
    Ok(())
}

/// Release a key.
pub fn key_release(key: Key) -> Result<()> {
    let keycode = key_to_keycode(key)
        .ok_or_else(|| Error::SimulateFailed(format!("Unsupported key: {:?}", key)))?;

    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;

        if is_modifier_key(key) {
            // For modifier keys, use FlagsChanged event type
            let event = CGEvent::new(Some(&source))
                .ok_or_else(|| Error::SimulateFailed("Failed to create event".into()))?;
            CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::KeyboardEventKeycode,
                keycode as i64,
            );

            // Update flags
            let mut flags = SIM_FLAGS
                .lock()
                .map_err(|_| Error::SimulateFailed("mutex poisoned".into()))?;
            match key {
                Key::ShiftLeft | Key::ShiftRight => {
                    flags.remove(CGEventFlags::MaskShift);
                }
                Key::ControlLeft | Key::ControlRight => {
                    flags.remove(CGEventFlags::MaskControl);
                }
                Key::AltLeft | Key::AltRight => {
                    flags.remove(CGEventFlags::MaskAlternate);
                }
                Key::MetaLeft | Key::MetaRight => {
                    flags.remove(CGEventFlags::MaskCommand);
                }
                _ => {}
            }
            CGEvent::set_flags(Some(&event), *flags);
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        } else {
            // For regular keys, use keyboard event
            let event = CGEvent::new_keyboard_event(Some(&source), keycode, false)
                .ok_or_else(|| Error::SimulateFailed("Failed to create keyboard event".into()))?;
            let flags = SIM_FLAGS
                .lock()
                .map_err(|_| Error::SimulateFailed("mutex poisoned".into()))?;
            CGEvent::set_flags(Some(&event), *flags);
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        }
    }
    Ok(())
}

/// Press and release a key.
pub fn key_tap(key: Key) -> Result<()> {
    key_press(key)?;
    key_release(key)?;
    Ok(())
}

/// Convert our Button to CGMouseButton.
fn button_to_cg_button(button: Button) -> CGMouseButton {
    match button {
        Button::Left => CGMouseButton::Left,
        Button::Right => CGMouseButton::Right,
        Button::Middle => CGMouseButton::Center,
        _ => CGMouseButton::Left,
    }
}

/// Press a mouse button.
pub fn mouse_press(button: Button) -> Result<()> {
    let point = get_current_mouse_location()?;
    let cg_button = button_to_cg_button(button);

    let event_type = match button {
        Button::Left => CGEventType::LeftMouseDown,
        Button::Right => CGEventType::RightMouseDown,
        _ => CGEventType::OtherMouseDown,
    };

    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;
        let event = CGEvent::new_mouse_event(Some(&source), event_type, point, cg_button)
            .ok_or_else(|| Error::SimulateFailed("Failed to create mouse event".into()))?;

        // Set button number for other mouse buttons
        if let Button::Button4 | Button::Button5 | Button::Middle | Button::Unknown(_) = button {
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::MouseEventButtonNumber,
                (button.number() - 1) as i64,
            );
        }

        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
    Ok(())
}

/// Release a mouse button.
pub fn mouse_release(button: Button) -> Result<()> {
    let point = get_current_mouse_location()?;
    let cg_button = button_to_cg_button(button);

    let event_type = match button {
        Button::Left => CGEventType::LeftMouseUp,
        Button::Right => CGEventType::RightMouseUp,
        _ => CGEventType::OtherMouseUp,
    };

    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;
        let event = CGEvent::new_mouse_event(Some(&source), event_type, point, cg_button)
            .ok_or_else(|| Error::SimulateFailed("Failed to create mouse event".into()))?;

        // Set button number for other mouse buttons
        if let Button::Button4 | Button::Button5 | Button::Middle | Button::Unknown(_) = button {
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::MouseEventButtonNumber,
                (button.number() - 1) as i64,
            );
        }

        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
    Ok(())
}

/// Click a mouse button (press and release).
pub fn mouse_click(button: Button) -> Result<()> {
    mouse_press(button)?;
    mouse_release(button)?;
    Ok(())
}

/// Move the mouse to a position.
pub fn mouse_move(x: f64, y: f64) -> Result<()> {
    let point = CGPoint { x, y };

    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;
        let event = CGEvent::new_mouse_event(
            Some(&source),
            CGEventType::MouseMoved,
            point,
            CGMouseButton::Left,
        )
        .ok_or_else(|| Error::SimulateFailed("Failed to create mouse event".into()))?;

        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
    Ok(())
}

/// Scroll the mouse wheel.
pub fn mouse_scroll(delta_y: i32, delta_x: i32) -> Result<()> {
    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;
        let event = CGEvent::new_scroll_wheel_event2(
            Some(&source),
            CGScrollEventUnit::Pixel,
            2, // wheel_count
            delta_y,
            delta_x,
            0,
        )
        .ok_or_else(|| Error::SimulateFailed("Failed to create scroll event".into()))?;

        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    }
    Ok(())
}
