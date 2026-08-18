//! The keys above the number row that are not keys.
//!
//! Volume, brightness and playback are not `CGEventCreateKeyboardEvent` with an
//! unusual key code. They are `NSEventTypeSystemDefined` (14) with subtype 8,
//! and the *system* is what acts on them: it changes the volume, and it draws
//! the on-screen display while doing it.
//!
//! # Why this and not `osascript`
//!
//! `set volume` changes the level and shows nothing, which is a different
//! behaviour rather than the same behaviour more quietly. Somebody pressing F12
//! sees the overlay and knows the press landed; the same change made through
//! AppleScript is invisible, and on a machine being driven from across the room
//! the overlay is the only acknowledgement there is.
//!
//! Posting the key the keyboard posts gets the whole behaviour — the level, the
//! overlay, the quarter-steps under ⌥⇧, and whatever a future macOS decides the
//! key should do — for less code than reading and writing the level by hand.
//!
//! # Where the numbers come from
//!
//! Mac Mouse Fix, `Helper/Core/Actions/Actions.m:188-215` and
//! `Shared/Constants.h:262-292`, symlinked in `references/`. `data1` packs the
//! key in bits 16 and up, a two-bit base that is always set, and one bit that
//! says whether this is the press or the release.
//!
//! # Why AppKit
//!
//! There is no CoreGraphics constructor for a system-defined event: `data1` and
//! `data2` are `NSEvent` properties with no `CGEventField` behind them, so the
//! event has to be built as an `NSEvent` and converted. `+otherEventWithType:…`
//! is a factory — it touches no window, no run loop and no shared application
//! state — which is what makes it callable from a daemon with no UI.

use std::ptr::null_mut;

use objc2::msg_send;
use objc2::rc::autoreleasepool;
use objc2::runtime::AnyClass;
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{CGEvent, CGEventTapLocation};

use crate::error::{Error, Result};
use crate::media::MediaKey;

// Linked explicitly rather than relied on: `NSEvent` is AppKit's, and a binary
// that happens to pull AppKit in through something else today would lose the
// class the day that something else changes. Linking a framework costs nothing
// at runtime until a symbol from it is used.
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

/// `NSEventTypeSystemDefined`.
const SYSTEM_DEFINED: usize = 14;
/// The subtype every one of these carries.
const AUX_CONTROL: i16 = 8;
/// Bits 9 and 11 of `data1`, always set.
const BASE: isize = (1 << 9) | (1 << 11);
/// Bit 8 of `data1`: clear on the press, set on the release.
const RELEASED: isize = 1 << 8;

impl MediaKey {
    /// `NX_KEYTYPE_*`, from `Constants.h:262-284`.
    fn code(self) -> isize {
        match self {
            MediaKey::VolumeUp => 0,
            MediaKey::VolumeDown => 1,
            MediaKey::Mute => 7,
            MediaKey::BrightnessUp => 2,
            MediaKey::BrightnessDown => 3,
            MediaKey::PlayPause => 16,
            MediaKey::Next => 19,
            MediaKey::Previous => 20,
        }
    }
}

/// Press and release one of them.
///
/// Both halves are posted. The release is what several receivers use to decide
/// a key was tapped rather than held — a held volume key repeats — and a press
/// without one can leave a repeat running.
pub fn media_key(key: MediaKey) -> Result<()> {
    let class = AnyClass::get(c"NSEvent")
        .ok_or_else(|| Error::SimulateFailed("NSEvent is unavailable".into()))?;

    autoreleasepool(|_| {
        let data = BASE | (key.code() << 16);
        for state in [data, data | RELEASED] {
            // `windowNumber: -1` and a zero timestamp, as in the reference: the
            // system reads the key out of `data1` and nothing else here.
            let event: *mut objc2::runtime::AnyObject = unsafe {
                msg_send![
                    class,
                    otherEventWithType: SYSTEM_DEFINED,
                    location: CGPoint { x: 0.0, y: 0.0 },
                    modifierFlags: 0usize,
                    timestamp: 0.0f64,
                    windowNumber: -1isize,
                    context: null_mut::<objc2::runtime::AnyObject>(),
                    subtype: AUX_CONTROL,
                    data1: state,
                    data2: -1isize,
                ]
            };
            if event.is_null() {
                return Err(Error::SimulateFailed(
                    "NSEvent refused to build a system-defined event".into(),
                ));
            }

            // Autoreleased, which is why the whole loop is inside a pool.
            let cg: *mut CGEvent = unsafe { msg_send![event, CGEvent] };
            if cg.is_null() {
                return Err(Error::SimulateFailed(
                    "a system-defined NSEvent carried no CGEvent".into(),
                ));
            }
            let cg = unsafe { &*cg };

            // The session tap, not the HID tap. A system-defined event posted
            // at the HID level is re-interpreted on its way up and the overlay
            // does not appear; the reference posts to the session tap for the
            // same reason.
            super::provenance::tag_event(cg)?;
            CGEvent::post(CGEventTapLocation::SessionEventTap, Some(cg));
        }
        Ok(())
    })
}

/// Kept honest at compile time: the selector must exist on this SDK.
#[cfg(test)]
mod tests {
    use super::*;
    use objc2::sel;

    #[test]
    fn the_factory_selector_still_exists() {
        let class = AnyClass::get(c"NSEvent").expect("NSEvent");
        // The metaclass, because this is a class method. Asking the class
        // itself answers about instance methods and is quietly always false.
        assert!(
            class.metaclass().responds_to(sel!(
                otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:
            )),
            "AppKit renamed the system-defined event factory"
        );
    }

    #[test]
    fn every_key_has_its_own_code() {
        let all = [
            MediaKey::VolumeUp,
            MediaKey::VolumeDown,
            MediaKey::Mute,
            MediaKey::BrightnessUp,
            MediaKey::BrightnessDown,
            MediaKey::PlayPause,
            MediaKey::Next,
            MediaKey::Previous,
        ];
        for (index, one) in all.iter().enumerate() {
            for other in &all[index + 1..] {
                assert_ne!(one.code(), other.code(), "{one:?} and {other:?} collide");
            }
        }
    }
}
