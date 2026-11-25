//! HarmonyOS media/system key injection through Input Kit.

use super::constants::{KEY_ACTION_DOWN, KEY_ACTION_UP};
use super::keycodes::media_key_to_keycode;
use super::simulate::NativeKeyEvent;
use crate::error::Result;
use crate::media::MediaKey;

/// Press and release one media/system key via `OH_Input_InjectKeyEvent`.
pub fn media_key(key: MediaKey) -> Result<()> {
    let keycode = media_key_to_keycode(key);
    let event = NativeKeyEvent::new()?;
    event.inject(KEY_ACTION_DOWN, keycode)?;
    event.inject(KEY_ACTION_UP, keycode)
}
