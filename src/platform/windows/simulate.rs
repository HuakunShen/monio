//! Windows event simulation using SendInput.

use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType};
use crate::keycode::Key;
use std::mem::size_of;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
};

use super::keycodes::key_to_keycode;

const WHEEL_DELTA: u32 = 120;

/// Send a mouse event
fn sim_mouse_event(flags: MOUSE_EVENT_FLAGS, data: u32, dx: i32, dy: i32) -> Result<()> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let inputs = [input];
    let result = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };

    if result != 1 {
        Err(Error::SimulateFailed(
            "SendInput failed for mouse event".into(),
        ))
    } else {
        Ok(())
    }
}

/// Send a keyboard event
fn sim_keyboard_event(vk: u16, flags: u32) -> Result<()> {
    let mut dwflags = windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0);
    if flags != 0 {
        dwflags = KEYEVENTF_KEYUP;
    }

    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: dwflags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

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
            if let Some(mouse) = &event.mouse {
                if let Some(button) = mouse.button {
                    mouse_press(button)?;
                }
            }
        }
        EventType::MouseReleased => {
            if let Some(mouse) = &event.mouse {
                if let Some(button) = mouse.button {
                    mouse_release(button)?;
                }
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
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if width == 0 || height == 0 {
        return Err(Error::SimulateFailed("Failed to get screen metrics".into()));
    }

    let normalized_x = ((x as i32 + 1) * 65535) / width;
    let normalized_y = ((y as i32 + 1) * 65535) / height;

    sim_mouse_event(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        0,
        normalized_x,
        normalized_y,
    )
}

/// Scroll the mouse wheel.
pub fn mouse_scroll(delta_y: i32, delta_x: i32) -> Result<()> {
    if delta_y != 0 {
        sim_mouse_event(MOUSEEVENTF_WHEEL, (delta_y as i32).wrapping_mul(WHEEL_DELTA as i32) as u32, 0, 0)?;
    }
    if delta_x != 0 {
        sim_mouse_event(MOUSEEVENTF_HWHEEL, (delta_x as i32).wrapping_mul(WHEEL_DELTA as i32) as u32, 0, 0)?;
    }
    Ok(())
}
