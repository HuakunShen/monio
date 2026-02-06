//! X11 event simulation using XTest.

use crate::error::{Error, Result};
use crate::event::{Button, Event, EventType};
use crate::keycode::Key;
use std::os::raw::c_int;
use std::ptr::null;
use x11::xlib;
use x11::xtest;

use crate::platform::linux::keycodes::key_to_keycode;

const TRUE: c_int = 1;
const FALSE: c_int = 0;

/// Get current mouse position as (x, y) coordinates.
pub fn mouse_position() -> Result<(f64, f64)> {
    let display = open_display()?;
    let screen = unsafe { xlib::XDefaultScreen(display) };
    let root = unsafe { xlib::XRootWindow(display, screen) };

    let mut root_return = 0u64;
    let mut child_return = 0u64;
    let mut root_x: c_int = 0;
    let mut root_y: c_int = 0;
    let mut win_x: c_int = 0;
    let mut win_y: c_int = 0;
    let mut mask: u32 = 0;

    let result = unsafe {
        xlib::XQueryPointer(
            display,
            root,
            &mut root_return,
            &mut child_return,
            &mut root_x,
            &mut root_y,
            &mut win_x,
            &mut win_y,
            &mut mask,
        )
    };

    unsafe { xlib::XCloseDisplay(display) };

    if result == FALSE {
        Err(Error::SimulateFailed("XQueryPointer failed".into()))
    } else {
        Ok((root_x as f64, root_y as f64))
    }
}

/// Open a display connection
fn open_display() -> Result<*mut xlib::Display> {
    let display = unsafe { xlib::XOpenDisplay(null()) };
    if display.is_null() {
        Err(Error::SimulateFailed("Failed to open X display".into()))
    } else {
        Ok(display)
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

    let display = open_display()?;
    let result = unsafe { xtest::XTestFakeKeyEvent(display, keycode, TRUE, 0) };

    unsafe {
        xlib::XFlush(display);
        xlib::XSync(display, 0);
        xlib::XCloseDisplay(display);
    }

    if result == 0 {
        Err(Error::SimulateFailed("XTestFakeKeyEvent failed".into()))
    } else {
        Ok(())
    }
}

/// Release a key.
pub fn key_release(key: Key) -> Result<()> {
    let keycode = key_to_keycode(key)
        .ok_or_else(|| Error::SimulateFailed(format!("Unsupported key: {:?}", key)))?;

    let display = open_display()?;
    let result = unsafe { xtest::XTestFakeKeyEvent(display, keycode, FALSE, 0) };

    unsafe {
        xlib::XFlush(display);
        xlib::XSync(display, 0);
        xlib::XCloseDisplay(display);
    }

    if result == 0 {
        Err(Error::SimulateFailed("XTestFakeKeyEvent failed".into()))
    } else {
        Ok(())
    }
}

/// Press and release a key.
pub fn key_tap(key: Key) -> Result<()> {
    key_press(key)?;
    key_release(key)?;
    Ok(())
}

/// Get X11 button code
fn button_to_code(button: Button) -> u32 {
    match button {
        Button::Left => 1,
        Button::Middle => 2,
        Button::Right => 3,
        Button::Button4 => 8,
        Button::Button5 => 9,
        Button::Unknown(code) => code as u32,
    }
}

/// Press a mouse button.
pub fn mouse_press(button: Button) -> Result<()> {
    let code = button_to_code(button);
    let display = open_display()?;
    let result = unsafe { xtest::XTestFakeButtonEvent(display, code, TRUE, 0) };

    unsafe {
        xlib::XFlush(display);
        xlib::XSync(display, 0);
        xlib::XCloseDisplay(display);
    }

    if result == 0 {
        Err(Error::SimulateFailed("XTestFakeButtonEvent failed".into()))
    } else {
        Ok(())
    }
}

/// Release a mouse button.
pub fn mouse_release(button: Button) -> Result<()> {
    let code = button_to_code(button);
    let display = open_display()?;
    let result = unsafe { xtest::XTestFakeButtonEvent(display, code, FALSE, 0) };

    unsafe {
        xlib::XFlush(display);
        xlib::XSync(display, 0);
        xlib::XCloseDisplay(display);
    }

    if result == 0 {
        Err(Error::SimulateFailed("XTestFakeButtonEvent failed".into()))
    } else {
        Ok(())
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
    let display = open_display()?;

    let x_int = if x.is_finite() {
        x.clamp(c_int::MIN as f64, c_int::MAX as f64).round() as c_int
    } else {
        0
    };
    let y_int = if y.is_finite() {
        y.clamp(c_int::MIN as f64, c_int::MAX as f64).round() as c_int
    } else {
        0
    };

    let result = unsafe { xtest::XTestFakeMotionEvent(display, 0, x_int, y_int, 0) };

    unsafe {
        xlib::XFlush(display);
        xlib::XSync(display, 0);
        xlib::XCloseDisplay(display);
    }

    if result == 0 {
        Err(Error::SimulateFailed("XTestFakeMotionEvent failed".into()))
    } else {
        Ok(())
    }
}

/// Scroll the mouse wheel.
pub fn mouse_scroll(delta_y: i32, delta_x: i32) -> Result<()> {
    let display = open_display()?;
    let mut success = true;

    // X11 scroll is done via button events (4=up, 5=down, 6=left, 7=right)
    unsafe {
        // Vertical scroll
        if delta_y != 0 {
            let button = if delta_y > 0 { 4 } else { 5 }; // Up or Down
            for _ in 0..delta_y.abs() {
                let r1 = xtest::XTestFakeButtonEvent(display, button, TRUE, 0);
                let r2 = xtest::XTestFakeButtonEvent(display, button, FALSE, 0);
                if r1 == 0 || r2 == 0 {
                    success = false;
                }
            }
        }

        // Horizontal scroll
        if delta_x != 0 {
            let button = if delta_x > 0 { 7 } else { 6 }; // Right or Left
            for _ in 0..delta_x.abs() {
                let r1 = xtest::XTestFakeButtonEvent(display, button, TRUE, 0);
                let r2 = xtest::XTestFakeButtonEvent(display, button, FALSE, 0);
                if r1 == 0 || r2 == 0 {
                    success = false;
                }
            }
        }

        xlib::XFlush(display);
        xlib::XSync(display, 0);
        xlib::XCloseDisplay(display);
    }

    if success {
        Ok(())
    } else {
        Err(Error::SimulateFailed("XTestFakeButtonEvent failed".into()))
    }
}
