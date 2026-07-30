//! HarmonyOS input simulation and pointer-position queries.

use crate::error::{Error, Result};
use crate::event::{Button, Event};
use crate::keycode::Key;

fn unsupported(operation: &str) -> Error {
    Error::NotSupported(format!("HarmonyOS {operation} is not implemented"))
}

pub fn simulate(_event: &Event) -> Result<()> {
    Err(unsupported("input simulation"))
}

pub fn key_press(_key: Key) -> Result<()> {
    Err(unsupported("key press simulation"))
}

pub fn key_release(_key: Key) -> Result<()> {
    Err(unsupported("key release simulation"))
}

pub fn key_tap(_key: Key) -> Result<()> {
    Err(unsupported("key tap simulation"))
}

pub fn mouse_press(_button: Button) -> Result<()> {
    Err(unsupported("mouse press simulation"))
}

pub fn mouse_release(_button: Button) -> Result<()> {
    Err(unsupported("mouse release simulation"))
}

pub fn mouse_click(_button: Button) -> Result<()> {
    Err(unsupported("mouse click simulation"))
}

pub fn mouse_position() -> Result<(f64, f64)> {
    Err(unsupported("pointer position query"))
}

pub fn mouse_move(_x: f64, _y: f64) -> Result<()> {
    Err(unsupported("mouse move simulation"))
}
