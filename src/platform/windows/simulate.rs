//! Windows event simulation using SendInput.

use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType};
use crate::keycode::Key;
use crate::platform::motion::{Motion, motion_from_event};
use std::mem::size_of;
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
};

use super::{keycodes::key_to_keycode, provenance};

const WHEEL_DELTA: u32 = 120;

/// Get current mouse position as (x, y) coordinates.
pub fn mouse_position() -> Result<(f64, f64)> {
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        GetCursorPos(&mut point)
            .map_err(|e| Error::SimulateFailed(format!("Failed to get cursor position: {}", e)))?;
    }
    Ok((point.x as f64, point.y as f64))
}

fn build_mouse_input(flags: MOUSE_EVENT_FLAGS, data: u32, dx: i32, dy: i32) -> Result<INPUT> {
    let session_tag = provenance::session_tag()?;
    Ok(build_mouse_input_with_tag(flags, data, dx, dy, session_tag))
}

fn build_mouse_input_with_tag(
    flags: MOUSE_EVENT_FLAGS,
    data: u32,
    dx: i32,
    dy: i32,
    tag: usize,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: tag,
            },
        },
    }
}

fn normalize_relative_axis(value: f64) -> i32 {
    if !value.is_finite() {
        0
    } else {
        value.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }
}

fn build_relative_mouse_input(delta_x: f64, delta_y: f64, tag: usize) -> Result<INPUT> {
    Ok(build_mouse_input_with_tag(
        MOUSEEVENTF_MOVE,
        0,
        normalize_relative_axis(delta_x),
        normalize_relative_axis(delta_y),
        tag,
    ))
}

fn send_input(input: INPUT, context: &str) -> Result<()> {
    let result = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };

    if result != 1 {
        Err(Error::SimulateFailed(format!(
            "SendInput failed for {context}"
        )))
    } else {
        Ok(())
    }
}

/// Send a mouse event
fn sim_mouse_event(flags: MOUSE_EVENT_FLAGS, data: u32, dx: i32, dy: i32) -> Result<()> {
    let input = build_mouse_input(flags, data, dx, dy)?;
    send_input(input, "mouse event")
}

fn build_keyboard_input(vk: u16, flags: u32) -> Result<INPUT> {
    let session_tag = provenance::session_tag()?;
    let mut dwflags = windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0);
    if flags != 0 {
        dwflags = KEYEVENTF_KEYUP;
    }

    Ok(INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: dwflags,
                time: 0,
                dwExtraInfo: session_tag,
            },
        },
    })
}

/// Send a keyboard event
fn sim_keyboard_event(vk: u16, flags: u32) -> Result<()> {
    let input = build_keyboard_input(vk, flags)?;
    let inputs = [input];
    let result = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };

    if result != 1 {
        Err(Error::SimulateFailed(
            "SendInput failed for keyboard event".into(),
        ))
    } else {
        Ok(())
    }
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
    sim_keyboard_event(keycode, 0)
}

/// Release a key.
pub fn key_release(key: Key) -> Result<()> {
    let keycode = key_to_keycode(key)
        .ok_or_else(|| Error::SimulateFailed(format!("Unsupported key: {:?}", key)))?;
    sim_keyboard_event(keycode, 1)
}

/// Press and release a key.
pub fn key_tap(key: Key) -> Result<()> {
    key_press(key)?;
    key_release(key)?;
    Ok(())
}

/// Press a mouse button.
pub fn mouse_press(button: Button) -> Result<()> {
    match button {
        Button::Left => sim_mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0),
        Button::Right => sim_mouse_event(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0),
        Button::Middle => sim_mouse_event(MOUSEEVENTF_MIDDLEDOWN, 0, 0, 0),
        Button::Button4 => sim_mouse_event(MOUSEEVENTF_XDOWN, 1, 0, 0),
        Button::Button5 => sim_mouse_event(MOUSEEVENTF_XDOWN, 2, 0, 0),
        Button::Unknown(code) => sim_mouse_event(MOUSEEVENTF_XDOWN, code as u32, 0, 0),
    }
}

/// Release a mouse button.
pub fn mouse_release(button: Button) -> Result<()> {
    match button {
        Button::Left => sim_mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0),
        Button::Right => sim_mouse_event(MOUSEEVENTF_RIGHTUP, 0, 0, 0),
        Button::Middle => sim_mouse_event(MOUSEEVENTF_MIDDLEUP, 0, 0, 0),
        Button::Button4 => sim_mouse_event(MOUSEEVENTF_XUP, 1, 0, 0),
        Button::Button5 => sim_mouse_event(MOUSEEVENTF_XUP, 2, 0, 0),
        Button::Unknown(code) => sim_mouse_event(MOUSEEVENTF_XUP, code as u32, 0, 0),
    }
}

/// Click a mouse button (press and release).
pub fn mouse_click(button: Button) -> Result<()> {
    mouse_press(button)?;
    mouse_release(button)?;
    Ok(())
}

/// Move the mouse to a position.
pub fn mouse_move(x: f64, y: f64) -> Result<()> {
    let (normalized_x, normalized_y) = normalized_absolute_position(x, y)?;

    sim_mouse_event(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        0,
        normalized_x,
        normalized_y,
    )
}

fn normalized_absolute_position(x: f64, y: f64) -> Result<(i32, i32)> {
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if width == 0 || height == 0 {
        return Err(Error::SimulateFailed("Failed to get screen metrics".into()));
    }

    let normalized_x = ((x as i32 + 1) * 65535) / width;
    let normalized_y = ((y as i32 + 1) * 65535) / height;

    Ok((normalized_x, normalized_y))
}

/// Move the mouse by a relative offset.
pub fn mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    let input = build_relative_mouse_input(delta_x, delta_y, provenance::session_tag()?)?;
    let mouse = unsafe { input.Anonymous.mi };
    if mouse.dx == 0 && mouse.dy == 0 {
        return Ok(());
    }
    send_input(input, "relative mouse movement")
}

pub(super) fn replay_mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    let input = build_relative_mouse_input(delta_x, delta_y, provenance::grab_replay_tag()?)?;
    send_input(input, "grab relative mouse replay")
}

pub(super) fn replay_mouse_move_absolute(x: f64, y: f64) -> Result<()> {
    let (dx, dy) = normalized_absolute_position(x, y)?;
    let input = build_mouse_input_with_tag(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        0,
        dx,
        dy,
        provenance::grab_replay_tag()?,
    );
    send_input(input, "grab absolute mouse replay")
}

/// Scroll the mouse wheel.
pub fn mouse_scroll(delta_y: i32, delta_x: i32) -> Result<()> {
    if delta_y != 0 {
        sim_mouse_event(
            MOUSEEVENTF_WHEEL,
            delta_y.wrapping_mul(WHEEL_DELTA as i32) as u32,
            0,
            0,
        )?;
    }
    if delta_x != 0 {
        sim_mouse_event(
            MOUSEEVENTF_HWHEEL,
            delta_x.wrapping_mul(WHEEL_DELTA as i32) as u32,
            0,
            0,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_input_carries_this_session_tag() {
        let input = build_keyboard_input(0x41, 0).expect("keyboard input should build");
        let keyboard = unsafe { input.Anonymous.ki };

        assert_eq!(
            keyboard.dwExtraInfo,
            provenance::session_tag().expect("session tag should initialize")
        );
    }

    #[test]
    fn mouse_input_carries_this_session_tag() {
        let input =
            build_mouse_input(MOUSEEVENTF_MOVE, 0, 100, 200).expect("mouse input should build");
        let mouse = unsafe { input.Anonymous.mi };

        assert_eq!(
            mouse.dwExtraInfo,
            provenance::session_tag().expect("session tag should initialize")
        );
    }

    #[test]
    fn relative_axis_values_are_rounded_clamped_and_finite() {
        assert_eq!(normalize_relative_axis(4.4), 4);
        assert_eq!(normalize_relative_axis(4.6), 5);
        assert_eq!(normalize_relative_axis(-4.6), -5);
        assert_eq!(normalize_relative_axis(f64::NAN), 0);
        assert_eq!(normalize_relative_axis(f64::INFINITY), 0);
        assert_eq!(normalize_relative_axis(f64::MAX), i32::MAX);
        assert_eq!(normalize_relative_axis(f64::MIN), i32::MIN);
    }

    #[test]
    fn relative_mouse_input_has_no_absolute_flags() {
        let input = build_relative_mouse_input(12.0, -7.0, provenance::session_tag().unwrap())
            .expect("relative input should build");
        let mouse = unsafe { input.Anonymous.mi };

        assert!(mouse.dwFlags.contains(MOUSEEVENTF_MOVE));
        assert!(!mouse.dwFlags.contains(MOUSEEVENTF_ABSOLUTE));
        assert!(!mouse.dwFlags.contains(MOUSEEVENTF_VIRTUALDESK));
        assert_eq!((mouse.dx, mouse.dy), (12, -7));
    }

    #[test]
    fn grab_replay_input_uses_the_private_tag() {
        let input = build_relative_mouse_input(1.0, 2.0, provenance::grab_replay_tag().unwrap())
            .expect("replay input should build");
        let mouse = unsafe { input.Anonymous.mi };

        assert_eq!(mouse.dwExtraInfo, provenance::grab_replay_tag().unwrap());
    }
}
