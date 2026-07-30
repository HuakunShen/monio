use super::constants::{
    AXIS_TYPE_SCROLL_HORIZONTAL, AXIS_TYPE_SCROLL_VERTICAL, KEY_ACTION_CANCEL,
    KEY_ACTION_DOWN, KEY_ACTION_UP, MOUSE_ACTION_BUTTON_DOWN, MOUSE_ACTION_BUTTON_UP,
    MOUSE_ACTION_CANCEL, MOUSE_ACTION_MOVE, MOUSE_BUTTON_NONE,
};
use super::keycodes::{
    button_from_native, button_to_native, key_to_keycode, keycode_to_key,
};
use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType, ScrollDirection};
use crate::keycode::Key;
use crate::state::{
    self, MASK_ALT, MASK_CTRL, MASK_META, MASK_SHIFT, button_to_mask,
};

fn modifier_mask(key: Key) -> u32 {
    match key {
        Key::ShiftLeft | Key::ShiftRight => MASK_SHIFT,
        Key::ControlLeft | Key::ControlRight => MASK_CTRL,
        Key::AltLeft | Key::AltRight => MASK_ALT,
        Key::MetaLeft | Key::MetaRight => MASK_META,
        _ => 0,
    }
}

pub(crate) fn translate_key(action: i32, keycode: i32) -> Option<Event> {
    let key = keycode_to_key(keycode);
    let mask = modifier_mask(key);

    match action {
        KEY_ACTION_DOWN => {
            state::set_mask(mask);
            Some(Event::key_pressed(key, keycode as u32))
        }
        KEY_ACTION_CANCEL | KEY_ACTION_UP => {
            state::unset_mask(mask);
            Some(Event::key_released(key, keycode as u32))
        }
        _ => None,
    }
}

pub(crate) fn translate_mouse(
    action: i32,
    button: i32,
    x: f64,
    y: f64,
) -> Option<Event> {
    match action {
        MOUSE_ACTION_MOVE => Some(if state::is_button_held() {
            Event::mouse_dragged(x, y)
        } else {
            Event::mouse_moved(x, y)
        }),
        MOUSE_ACTION_BUTTON_DOWN => {
            let button = button_from_native(button);
            state::set_mask(button_to_mask(button.number()));
            Some(Event::mouse_pressed(button, x, y))
        }
        MOUSE_ACTION_CANCEL | MOUSE_ACTION_BUTTON_UP if button != MOUSE_BUTTON_NONE => {
            let button = button_from_native(button);
            state::unset_mask(button_to_mask(button.number()));
            Some(Event::mouse_released(button, x, y))
        }
        _ => None,
    }
}

pub(crate) fn translate_axis(
    axis_type: u32,
    x: f64,
    y: f64,
    value: f64,
) -> Option<Event> {
    if value == 0.0 || !value.is_finite() {
        return None;
    }

    let direction = match (axis_type, value.is_sign_positive()) {
        (AXIS_TYPE_SCROLL_VERTICAL, true) => ScrollDirection::Up,
        (AXIS_TYPE_SCROLL_VERTICAL, false) => ScrollDirection::Down,
        (AXIS_TYPE_SCROLL_HORIZONTAL, true) => ScrollDirection::Right,
        (AXIS_TYPE_SCROLL_HORIZONTAL, false) => ScrollDirection::Left,
        _ => return None,
    };

    Some(Event::mouse_wheel(x, y, direction, value.abs()))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SimulationSpec {
    Key {
        action: i32,
        keycode: i32,
    },
    Mouse {
        action: i32,
        button: i32,
        x: i32,
        y: i32,
    },
}

pub(crate) fn simulation_spec(event: &Event) -> Result<SimulationSpec> {
    match event.event_type {
        EventType::KeyPressed | EventType::KeyReleased => {
            let keyboard = event.keyboard.as_ref().ok_or_else(|| {
                Error::NotSupported("HarmonyOS key event is missing keyboard data".into())
            })?;
            let keycode = key_to_keycode(keyboard.key).ok_or_else(|| {
                Error::NotSupported(format!(
                    "HarmonyOS cannot simulate key {:?}",
                    keyboard.key
                ))
            })?;
            let action = if event.event_type == EventType::KeyPressed {
                KEY_ACTION_DOWN
            } else {
                KEY_ACTION_UP
            };

            Ok(SimulationSpec::Key { action, keycode })
        }
        EventType::MousePressed | EventType::MouseReleased => {
            let mouse = event.mouse.as_ref().ok_or_else(|| {
                Error::NotSupported("HarmonyOS mouse event is missing mouse data".into())
            })?;
            let button = mouse.button.ok_or_else(|| {
                Error::NotSupported("HarmonyOS mouse button event has no button".into())
            })?;
            if matches!(button, Button::Unknown(_)) {
                return Err(Error::NotSupported(format!(
                    "HarmonyOS cannot simulate mouse button {button:?}"
                )));
            }
            let button = button_to_native(button).ok_or_else(|| {
                Error::NotSupported(format!(
                    "HarmonyOS cannot simulate mouse button {button:?}"
                ))
            })?;
            let (x, y) = checked_coordinates(mouse.x, mouse.y)?;
            let action = if event.event_type == EventType::MousePressed {
                MOUSE_ACTION_BUTTON_DOWN
            } else {
                MOUSE_ACTION_BUTTON_UP
            };

            Ok(SimulationSpec::Mouse {
                action,
                button,
                x,
                y,
            })
        }
        EventType::MouseMoved => {
            let mouse = event.mouse.as_ref().ok_or_else(|| {
                Error::NotSupported("HarmonyOS mouse move is missing mouse data".into())
            })?;
            let (x, y) = checked_coordinates(mouse.x, mouse.y)?;
            Ok(SimulationSpec::Mouse {
                action: MOUSE_ACTION_MOVE,
                button: MOUSE_BUTTON_NONE,
                x,
                y,
            })
        }
        _ => Err(Error::NotSupported(format!(
            "HarmonyOS cannot simulate {:?} events",
            event.event_type
        ))),
    }
}

fn checked_coordinates(x: f64, y: f64) -> Result<(i32, i32)> {
    if !x.is_finite()
        || !y.is_finite()
        || x < i32::MIN as f64
        || x > i32::MAX as f64
        || y < i32::MIN as f64
        || y > i32::MAX as f64
    {
        return Err(Error::SimulateFailed(format!(
            "HarmonyOS global coordinates are outside the i32 range: ({x}, {y})"
        )));
    }

    Ok((x as i32, y as i32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::event::{Button, EventType, InputOrigin, ScrollDirection};
    use crate::keycode::Key;
    use crate::state::{
        self, MASK_BUTTON1, MASK_SHIFT, is_button_pressed, is_shift_held,
    };

    #[test]
    fn translates_native_primitives_to_owned_events_and_updates_state() {
        state::reset_mask();

        let pressed = translate_key(1, 2047).expect("key down should translate");
        assert_eq!(pressed.event_type, EventType::KeyPressed);
        assert_eq!(pressed.keyboard.as_ref().unwrap().key, Key::ShiftLeft);
        assert_eq!(pressed.keyboard.as_ref().unwrap().raw_code, 2047);
        assert!(pressed.mask & MASK_SHIFT != 0);
        assert!(is_shift_held());
        assert_eq!(pressed.origin, InputOrigin::Unknown);

        let released = translate_key(2, 2047).expect("key up should translate");
        assert_eq!(released.event_type, EventType::KeyReleased);
        assert_eq!(released.mask & MASK_SHIFT, 0);
        assert!(!is_shift_held());

        let cancelled = translate_key(0, 2017).expect("key cancel should release");
        assert_eq!(cancelled.event_type, EventType::KeyReleased);
        assert_eq!(cancelled.keyboard.as_ref().unwrap().key, Key::KeyA);
        assert!(translate_key(99, 2017).is_none());

        let mouse_down =
            translate_mouse(2, 0, 10.0, 20.0).expect("button down should translate");
        assert_eq!(mouse_down.event_type, EventType::MousePressed);
        assert_eq!(mouse_down.mouse.as_ref().unwrap().button, Some(Button::Left));
        assert!(mouse_down.mask & MASK_BUTTON1 != 0);
        assert!(is_button_pressed(MASK_BUTTON1));

        let dragged = translate_mouse(1, -1, 11.0, 21.0).expect("move should translate");
        assert_eq!(dragged.event_type, EventType::MouseDragged);

        let mouse_up =
            translate_mouse(3, 0, 12.0, 22.0).expect("button up should translate");
        assert_eq!(mouse_up.event_type, EventType::MouseReleased);
        assert_eq!(mouse_up.mask & MASK_BUTTON1, 0);
        assert!(!is_button_pressed(MASK_BUTTON1));

        let moved = translate_mouse(1, -1, 13.0, 23.0).expect("move should translate");
        assert_eq!(moved.event_type, EventType::MouseMoved);
        assert!(translate_mouse(99, 0, 10.0, 20.0).is_none());

        let vertical_up =
            translate_axis(1, 10.0, 20.0, 2.5).expect("vertical axis should translate");
        assert_eq!(vertical_up.event_type, EventType::MouseWheel);
        assert_eq!(
            vertical_up.wheel.as_ref().unwrap().direction,
            ScrollDirection::Up
        );
        assert_eq!(vertical_up.wheel.as_ref().unwrap().delta, 2.5);

        let vertical_down =
            translate_axis(1, 10.0, 20.0, -2.5).expect("vertical axis should translate");
        assert_eq!(
            vertical_down.wheel.as_ref().unwrap().direction,
            ScrollDirection::Down
        );
        assert_eq!(vertical_down.wheel.as_ref().unwrap().delta, 2.5);

        let horizontal_right =
            translate_axis(2, 10.0, 20.0, 3.0).expect("horizontal axis should translate");
        assert_eq!(
            horizontal_right.wheel.as_ref().unwrap().direction,
            ScrollDirection::Right
        );

        let horizontal_left =
            translate_axis(2, 10.0, 20.0, -3.0).expect("horizontal axis should translate");
        assert_eq!(
            horizontal_left.wheel.as_ref().unwrap().direction,
            ScrollDirection::Left
        );

        assert!(translate_axis(0, 10.0, 20.0, 3.0).is_none());
        assert!(translate_axis(3, 10.0, 20.0, 3.0).is_none());
        assert!(translate_axis(1, 10.0, 20.0, 0.0).is_none());
        state::reset_mask();
    }

    #[test]
    fn simulation_specs_validate_supported_events() {
        assert_eq!(
            simulation_spec(&Event::key_pressed(Key::KeyA, 2017)).unwrap(),
            SimulationSpec::Key {
                action: 1,
                keycode: 2017
            }
        );
        assert_eq!(
            simulation_spec(&Event::key_released(Key::KeyA, 2017)).unwrap(),
            SimulationSpec::Key {
                action: 2,
                keycode: 2017
            }
        );
        assert_eq!(
            simulation_spec(&Event::mouse_pressed(Button::Left, 10.75, -20.25)).unwrap(),
            SimulationSpec::Mouse {
                action: 2,
                button: 0,
                x: 10,
                y: -20,
            }
        );
        assert_eq!(
            simulation_spec(&Event::mouse_released(Button::Button4, 30.0, 40.0)).unwrap(),
            SimulationSpec::Mouse {
                action: 3,
                button: 4,
                x: 30,
                y: 40,
            }
        );
        assert_eq!(
            simulation_spec(&Event::mouse_moved(50.0, 60.0)).unwrap(),
            SimulationSpec::Mouse {
                action: 1,
                button: -1,
                x: 50,
                y: 60,
            }
        );
    }

    #[test]
    fn simulation_specs_reject_unsupported_or_invalid_events() {
        let unsupported = [
            Event::key_pressed(Key::F13, 0),
            Event::mouse_pressed(Button::Unknown(42), 1.0, 2.0),
            Event::mouse_clicked(Button::Left, 1.0, 2.0, 1),
            Event::mouse_dragged(1.0, 2.0),
            Event::key_typed(Key::KeyA, 2017, 'a'),
            Event::hook_enabled(),
            Event::mouse_wheel(1.0, 2.0, ScrollDirection::Up, 1.0),
        ];

        for event in unsupported {
            assert!(matches!(
                simulation_spec(&event),
                Err(Error::NotSupported(_))
            ));
        }

        for event in [
            Event::mouse_moved(f64::NAN, 1.0),
            Event::mouse_moved(1.0, f64::INFINITY),
            Event::mouse_moved(i32::MAX as f64 + 1.0, 1.0),
            Event::mouse_moved(1.0, i32::MIN as f64 - 1.0),
        ] {
            assert!(matches!(
                simulation_spec(&event),
                Err(Error::SimulateFailed(_))
            ));
        }
    }
}
