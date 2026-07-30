//! HarmonyOS input simulation through Input Kit.

use super::display::mouse_position;
use super::result::simulate_error;
use super::translate::{SimulationSpec, simulation_spec};
use crate::error::{Error, Result};
use crate::event::{Button, Event};
use crate::keycode::Key;
use ohos_input_sys::input_manager::{
    OH_Input_CreateKeyEvent, OH_Input_CreateMouseEvent, OH_Input_DestroyKeyEvent,
    OH_Input_DestroyMouseEvent, OH_Input_InjectKeyEvent, OH_Input_InjectMouseEventGlobal,
    OH_Input_SetKeyEventAction, OH_Input_SetKeyEventKeyCode, OH_Input_SetMouseEventAction,
    OH_Input_SetMouseEventButton, OH_Input_SetMouseEventGlobalX, OH_Input_SetMouseEventGlobalY,
};
use std::ffi::c_void;
use std::ptr;

struct NativeKeyEvent(*mut c_void);

impl NativeKeyEvent {
    fn new() -> Result<Self> {
        // SAFETY: The returned pointer is checked for null and exclusively
        // owned by this RAII wrapper until Drop destroys it.
        let event = unsafe { OH_Input_CreateKeyEvent() };
        if event.is_null() {
            Err(Error::SimulateFailed(
                "OH_Input_CreateKeyEvent returned a null pointer".into(),
            ))
        } else {
            Ok(Self(event.cast()))
        }
    }

    fn inject(&self, action: i32, keycode: i32) -> Result<()> {
        // SAFETY: self owns a live Input_KeyEvent for the duration of these
        // synchronous setter and injection calls.
        let code = unsafe {
            let event = self.0.cast();
            OH_Input_SetKeyEventAction(event, action);
            OH_Input_SetKeyEventKeyCode(event, keycode);
            OH_Input_InjectKeyEvent(event)
        };
        injection_result("OH_Input_InjectKeyEvent", code)
    }
}

impl Drop for NativeKeyEvent {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }

        // SAFETY: this wrapper owns the pointer and calls the matching
        // destructor exactly once. Input Kit is allowed to null the local copy.
        unsafe {
            let mut event = self.0.cast();
            OH_Input_DestroyKeyEvent(&mut event);
        }
        self.0 = ptr::null_mut();
    }
}

struct NativeMouseEvent(*mut c_void);

impl NativeMouseEvent {
    fn new() -> Result<Self> {
        // SAFETY: The returned pointer is checked for null and exclusively
        // owned by this RAII wrapper until Drop destroys it.
        let event = unsafe { OH_Input_CreateMouseEvent() };
        if event.is_null() {
            Err(Error::SimulateFailed(
                "OH_Input_CreateMouseEvent returned a null pointer".into(),
            ))
        } else {
            Ok(Self(event.cast()))
        }
    }

    fn inject(&self, action: i32, button: i32, x: i32, y: i32) -> Result<()> {
        // SAFETY: self owns a live Input_MouseEvent for the duration of these
        // synchronous setter and injection calls.
        let code = unsafe {
            let event = self.0.cast();
            OH_Input_SetMouseEventAction(event, action);
            OH_Input_SetMouseEventButton(event, button);
            OH_Input_SetMouseEventGlobalX(event, x);
            OH_Input_SetMouseEventGlobalY(event, y);
            OH_Input_InjectMouseEventGlobal(event)
        };
        injection_result("OH_Input_InjectMouseEventGlobal", code)
    }
}

impl Drop for NativeMouseEvent {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }

        // SAFETY: this wrapper owns the pointer and calls the matching
        // destructor exactly once. Input Kit is allowed to null the local copy.
        unsafe {
            let mut event = self.0.cast();
            OH_Input_DestroyMouseEvent(&mut event);
        }
        self.0 = ptr::null_mut();
    }
}

fn injection_result(operation: &str, code: i32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(simulate_error(operation, code as u32))
    }
}

pub fn simulate(event: &Event) -> Result<()> {
    match simulation_spec(event)? {
        SimulationSpec::Key { action, keycode } => NativeKeyEvent::new()?.inject(action, keycode),
        SimulationSpec::Mouse {
            action,
            button,
            x,
            y,
        } => NativeMouseEvent::new()?.inject(action, button, x, y),
    }
}

pub fn key_press(key: Key) -> Result<()> {
    simulate(&Event::key_pressed(key, 0))
}

pub fn key_release(key: Key) -> Result<()> {
    simulate(&Event::key_released(key, 0))
}

pub fn key_tap(key: Key) -> Result<()> {
    key_press(key)?;
    key_release(key)
}

pub fn mouse_press(button: Button) -> Result<()> {
    let (x, y) = mouse_position()?;
    simulate(&Event::mouse_pressed(button, x, y))
}

pub fn mouse_release(button: Button) -> Result<()> {
    let (x, y) = mouse_position()?;
    simulate(&Event::mouse_released(button, x, y))
}

pub fn mouse_click(button: Button) -> Result<()> {
    mouse_press(button)?;
    mouse_release(button)
}

pub fn mouse_move(x: f64, y: f64) -> Result<()> {
    simulate(&Event::mouse_moved(x, y))
}
