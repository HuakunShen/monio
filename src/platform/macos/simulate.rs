//! macOS event simulation using CGEvent.

#![allow(unused_unsafe)]

use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType};
use crate::keycode::Key;
use crate::platform::motion::{Motion, motion_from_event};
use objc2_core_foundation::{CFRetained, CGPoint};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGScrollEventUnit,
};
use std::sync::Mutex;

use super::{keycodes::key_to_keycode, provenance};

/// Track the current modifier flags for simulation
static SIM_FLAGS: Mutex<CGEventFlags> = Mutex::new(CGEventFlags(0));

/// Buttons this process has pressed and not released.
///
/// Needed because motion during a press is a *different event type* on macOS,
/// not a flag — see [`create_mouse_move_event`]. Mirrors `heldButtons` in the
/// Swift injector, which fixed the same defect there in `1f16cdc`.
static SIM_BUTTONS: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn held_buttons() -> Vec<u8> {
    SIM_BUTTONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// The last press per button: which button, when, where, and what click count
/// it carried.
static SIM_CLICKS: Mutex<Vec<(u8, std::time::Instant, f64, f64, i64)>> = Mutex::new(Vec::new());

/// macOS's default double-click interval. Reading the user's own setting needs
/// AppKit, which this crate deliberately does not link; the default is what the
/// overwhelming majority of machines run, and being slightly conservative costs
/// a missed double-click rather than a spurious one.
const DOUBLE_CLICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// How far the pointer may move between two clicks and still be "the same
/// place". macOS is similarly forgiving; a hand on a mouse never lands twice on
/// exactly one pixel.
const DOUBLE_CLICK_SLOP: f64 = 4.0;

/// What click count this press should carry: 1, 2, 3, …
///
/// **Why this has to exist.** macOS does not infer double-clicks from a stream
/// of independent clicks — the *event* carries `kCGMouseEventClickState`, and an
/// injector that never sets it produces events every application reads as a
/// series of single clicks. Measured on 2026-08-01: dragging to select text
/// worked, and double-click-to-select-a-word and triple-click-to-select-a-line
/// did nothing at all, on a machine being driven through CrossFlow.
///
/// Reconstructed here rather than forwarded from the sending machine because
/// the count belongs to the machine the click lands on: it is that machine's
/// double-click interval, and that machine's applications, that decide what a
/// double-click means. Over a LAN the jitter between two clicks is a rounding
/// error against a 500 ms window.
fn next_click_state(button: u8, x: f64, y: f64, now: std::time::Instant) -> i64 {
    let mut clicks = SIM_CLICKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state = match clicks.iter().find(|entry| entry.0 == button) {
        Some((_, when, last_x, last_y, count))
            if now.duration_since(*when) <= DOUBLE_CLICK_INTERVAL
                && (x - last_x).abs() <= DOUBLE_CLICK_SLOP
                && (y - last_y).abs() <= DOUBLE_CLICK_SLOP =>
        {
            // Beyond a triple click macOS keeps counting, and so does this:
            // applications that care read `== 2` or `== 3` and ignore the rest.
            count.saturating_add(1)
        }
        _ => 1,
    };
    clicks.retain(|entry| entry.0 != button);
    clicks.push((button, now, x, y, state));
    state
}

/// The click count the matching press carried, without advancing it.
///
/// A release must repeat its press's count. Recomputing it would make a
/// double-click's mouse-up claim to be click three, and an application pairing
/// down with up by click state would see two unrelated events.
fn current_click_state(button: u8) -> i64 {
    SIM_CLICKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find(|entry| entry.0 == button)
        .map(|entry| entry.4)
        .unwrap_or(1)
}

/// The event type that carries pointer motion, given what is currently held.
///
/// Left wins over right wins over any other button when several are down: only
/// one type can be posted, and that is the order in which applications treat
/// them as the primary drag.
fn motion_event_type(held: &[u8]) -> (CGEventType, CGMouseButton) {
    if held.contains(&Button::Left.number()) {
        (CGEventType::LeftMouseDragged, CGMouseButton::Left)
    } else if held.contains(&Button::Right.number()) {
        (CGEventType::RightMouseDragged, CGMouseButton::Right)
    } else if let Some(other) = held.first() {
        let _ = other;
        (CGEventType::OtherMouseDragged, CGMouseButton::Center)
    } else {
        (CGEventType::MouseMoved, CGMouseButton::Left)
    }
}

fn post_event(event: &CGEvent) -> Result<()> {
    provenance::tag_event(event)?;
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
    Ok(())
}

/// Get current mouse position as (x, y) coordinates.
pub fn mouse_position() -> Result<(f64, f64)> {
    let point = get_current_mouse_location()?;
    Ok((point.x, point.y))
}

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

/// Build the event that carries pointer motion to `point`.
///
/// Motion while a button is held must be posted as a **drag**, not as a move.
/// macOS does not derive that from button state: an application watching for
/// `mouseDragged:` — which is every application that supports selecting text,
/// dragging a file, or moving a window — receives nothing at all from a
/// `mouseMoved` posted mid-press. The remote user sees the button go down, the
/// cursor travel, the button come up, and no drag happen.
///
/// Measured on 2026-08-01 across two Macs running CrossFlow: text could not be
/// selected on the far machine at all, only clicked. The Swift injector had
/// already been fixed for exactly this (`1f16cdc`); this is the Rust half.
///
/// The held modifier flags are applied for the same class of reason: an
/// application reading `NSEvent.modifierFlags` during a drag sees what the
/// event carries, not what some other process believes is held.
fn create_mouse_move_event(
    point: CGPoint,
    relative: Option<(f64, f64)>,
) -> Result<CFRetained<CGEvent>> {
    let held = held_buttons();
    let (event_type, cg_button) = motion_event_type(&held);
    let flags = *SIM_FLAGS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    unsafe {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .ok_or_else(|| Error::SimulateFailed("Failed to create event source".into()))?;
        let event = CGEvent::new_mouse_event(Some(&source), event_type, point, cg_button)
            .ok_or_else(|| Error::SimulateFailed("Failed to create mouse event".into()))?;
        CGEvent::set_flags(Some(&event), flags);

        if let Some((delta_x, delta_y)) = relative {
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::MouseEventDeltaX,
                delta_x as i64,
            );
            CGEvent::set_integer_value_field(
                Some(&event),
                CGEventField::MouseEventDeltaY,
                delta_y as i64,
            );
        }

        Ok(event)
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
        EventType::MouseMoved | EventType::MouseDragged => match motion_from_event(event) {
            Some(Motion::Absolute { x, y }) => mouse_move(x, y)?,
            Some(Motion::Relative { delta_x, delta_y }) => {
                mouse_move_relative(delta_x, delta_y)?;
            }
            None => {}
        },
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
            post_event(&event)?;
        } else {
            // For regular keys, use keyboard event
            let event = CGEvent::new_keyboard_event(Some(&source), keycode, true)
                .ok_or_else(|| Error::SimulateFailed("Failed to create keyboard event".into()))?;
            let flags = SIM_FLAGS
                .lock()
                .map_err(|_| Error::SimulateFailed("mutex poisoned".into()))?;
            CGEvent::set_flags(Some(&event), *flags);
            post_event(&event)?;
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
            post_event(&event)?;
        } else {
            // For regular keys, use keyboard event
            let event = CGEvent::new_keyboard_event(Some(&source), keycode, false)
                .ok_or_else(|| Error::SimulateFailed("Failed to create keyboard event".into()))?;
            let flags = SIM_FLAGS
                .lock()
                .map_err(|_| Error::SimulateFailed("mutex poisoned".into()))?;
            CGEvent::set_flags(Some(&event), *flags);
            post_event(&event)?;
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

        // Without this every injected click is a *first* click, and no
        // application ever sees a double or triple. See `next_click_state`.
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::MouseEventClickState,
            next_click_state(
                button.number(),
                point.x,
                point.y,
                std::time::Instant::now(),
            ),
        );

        post_event(&event)?;
    }
    // Recorded AFTER the post so a failed press does not leave a phantom held
    // button making every later move a drag.
    {
        let mut held = SIM_BUTTONS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !held.contains(&button.number()) {
            held.push(button.number());
        }
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

        // The same count its press carried, not a fresh one.
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::MouseEventClickState,
            current_click_state(button.number()),
        );

        post_event(&event)?;
    }
    // Cleared unconditionally: a release that failed to post still means this
    // process is no longer holding the button, and leaving it recorded would
    // turn every subsequent move into a drag nobody asked for.
    SIM_BUTTONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|held| *held != button.number());
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
    let event = create_mouse_move_event(CGPoint { x, y }, None)?;
    post_event(&event)
}

/// Move the mouse by a relative offset.
pub fn mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    let (x, y) = mouse_position()?;
    let event = create_mouse_move_event(
        CGPoint {
            x: x + delta_x,
            y: y + delta_y,
        },
        Some((delta_x, delta_y)),
    )?;
    post_event(&event)
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

        post_event(&event)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_mouse_event_keeps_absolute_target_and_delta_fields() {
        let event = create_mouse_move_event(CGPoint { x: 120.0, y: 240.0 }, Some((-9.0, 6.0)))
            .expect("mouse CGEvent should be available");

        assert_eq!(
            CGEvent::location(Some(&event)),
            CGPoint { x: 120.0, y: 240.0 }
        );
        assert_eq!(
            CGEvent::integer_value_field(Some(&event), CGEventField::MouseEventDeltaX,),
            -9
        );
        assert_eq!(
            CGEvent::integer_value_field(Some(&event), CGEventField::MouseEventDeltaY,),
            6
        );
    }
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    use std::time::Duration;

    /// Motion while a button is held is a *different event type* on macOS, not
    /// a flag. An application watching `mouseDragged:` — every application that
    /// supports selecting text or dragging a file — receives nothing at all
    /// from a `mouseMoved` posted mid-press.
    ///
    /// Measured across two Macs on 2026-08-01: text on the far machine could be
    /// clicked but never selected.
    #[test]
    fn motion_with_no_button_held_is_a_move() {
        let (event_type, _) = motion_event_type(&[]);
        assert_eq!(event_type, CGEventType::MouseMoved);
    }

    #[test]
    fn motion_while_the_left_button_is_held_is_a_drag() {
        let (event_type, button) = motion_event_type(&[Button::Left.number()]);
        assert_eq!(event_type, CGEventType::LeftMouseDragged);
        assert_eq!(button, CGMouseButton::Left);
    }

    #[test]
    fn motion_while_the_right_button_is_held_is_a_right_drag() {
        let (event_type, button) = motion_event_type(&[Button::Right.number()]);
        assert_eq!(event_type, CGEventType::RightMouseDragged);
        assert_eq!(button, CGMouseButton::Right);
    }

    /// Only one type can be posted, so several buttons down needs a defined
    /// winner — left, because that is the one applications treat as the primary
    /// drag.
    #[test]
    fn the_left_button_wins_when_several_are_held() {
        let (event_type, _) = motion_event_type(&[Button::Right.number(), Button::Left.number()]);
        assert_eq!(event_type, CGEventType::LeftMouseDragged);
    }

    /// macOS does not infer a double-click from two single clicks — the event
    /// carries the count, and an injector that never sets it makes
    /// double-click-to-select-a-word and triple-click-to-select-a-line silently
    /// impossible. Measured through CrossFlow on 2026-08-01: dragging to select
    /// text worked, double and triple click did nothing at all.
    ///
    /// `SIM_CLICKS` is process-global, so these run as one test rather than
    /// several that would race each other through it.
    #[test]
    fn clicks_in_the_same_place_in_quick_succession_count_up() {
        let button = Button::Left.number();
        let start = std::time::Instant::now();

        assert_eq!(next_click_state(button, 100.0, 100.0, start), 1);
        assert_eq!(
            next_click_state(button, 100.0, 100.0, start + Duration::from_millis(120)),
            2,
            "a second click in the same place is a double click"
        );
        assert_eq!(
            next_click_state(button, 100.0, 100.0, start + Duration::from_millis(240)),
            3,
            "a third is a triple click, which is what selects a line"
        );

        // A release repeats its press's count rather than advancing it.
        assert_eq!(current_click_state(button), 3);
        assert_eq!(current_click_state(button), 3, "reading must not advance it");

        // Too slow: a deliberate second click much later is a new first click.
        assert_eq!(
            next_click_state(button, 100.0, 100.0, start + Duration::from_secs(5)),
            1,
            "past the double-click interval this starts over"
        );

        // Too far: two quick clicks in different places are two single clicks,
        // which is what stops a fast mouse sweep from selecting words.
        let later = start + Duration::from_secs(5);
        assert_eq!(
            next_click_state(button, 400.0, 100.0, later + Duration::from_millis(50)),
            1,
            "a click far from the last one starts over"
        );

        // Buttons are counted independently: a right click does not advance the
        // left button's run.
        let right = Button::Right.number();
        let base = later + Duration::from_millis(100);
        assert_eq!(next_click_state(right, 400.0, 100.0, base), 1);
        assert_eq!(
            next_click_state(button, 400.0, 100.0, base + Duration::from_millis(50)),
            2,
            "the left button's own run continued across a right click"
        );
    }
}
