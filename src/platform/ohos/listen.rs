//! HarmonyOS input monitoring and keyboard-grab lifecycle.

use crate::error::{Error, Result};
use crate::hook::{EventHandler, GrabHandler};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub fn run_hook<H: EventHandler + 'static>(
    _running: &Arc<AtomicBool>,
    _handler: H,
) -> Result<()> {
    Err(Error::NotSupported(
        "HarmonyOS input monitoring is not implemented".into(),
    ))
}

pub fn run_grab_hook<H: GrabHandler + 'static>(
    _running: &Arc<AtomicBool>,
    _handler: H,
) -> Result<()> {
    Err(Error::NotSupported(
        "HarmonyOS keyboard grab is not implemented".into(),
    ))
}

pub fn stop_hook() -> Result<()> {
    Ok(())
}
