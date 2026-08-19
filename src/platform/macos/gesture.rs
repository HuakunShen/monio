//! Pinch and rotate, which are neither mouse events nor scroll events.
//!
//! # Where the field numbers come from
//!
//! There is no public API for synthesising a magnify. `NSEvent` exposes
//! `magnification` for reading and nothing for writing, and CoreGraphics has no
//! `CGEventCreateGestureEvent`. What does exist is a CGEvent of type 29
//! (`NSEventTypeGesture`) carrying a handful of undocumented fields, which is
//! what the trackpad driver itself produces and what AppKit turns back into an
//! `NSEventTypeMagnify`.
//!
//! The numbers below are not guesses. Two independent open-source
//! implementations agree on them:
//!
//! - Mac Mouse Fix, `Helper/Core/Touch/TouchSimulator.m:76-94`
//!   (`postMagnificationEventWithMagnification:phase:`) — a maintained
//!   application whose whole purpose is to make a mouse produce trackpad
//!   gestures, so these events are exercised on every macOS release.
//! - Calf Trail's TouchEvents (`External/TouchEvents.c:344-395`, as vendored by
//!   SensibleSideButtons), which arrives at the same event by the much longer
//!   road of serialising a whole IOHID event queue element by hand. Its
//!   `kTLInfoSubtypeMagnify = 0x08` is this file's `hid_type::ZOOM`, and its
//!   `kTLInfoKeyGesturePhase` is field `0x84` — 132.
//!
//! Both are in `references/` for the next reader.
//!
//! The long road is not taken here. TouchEvents constructs the touch data as
//! well, because it needs the *touches* to be believable — it is simulating
//! two fingers so that Mission Control's swipe recogniser accepts them. A
//! magnification needs no such thing: AppKit reads the magnification and the
//! phase, and nothing downstream asks where the fingers were.
//!
//! # What this is not
//!
//! Not `⌃`+scroll. That is Windows' zoom gesture and a browser convention;
//! on macOS it is bound to the system-wide accessibility zoom when that is
//! enabled and to nothing at all when it is not, and no native application
//! zooms from it. A real trackpad sends the events below, which is why Preview,
//! Photos, Maps, Finder and every browser respond to them without knowing
//! anything about who sent them.

use objc2_core_graphics::{CGEvent, CGEventField, CGEventTapLocation, CGEventType};

use crate::error::{Error, Result};
use crate::gesture::GesturePhase;

use super::provenance;

/// `NSEventTypeGesture`. AppKit re-labels it from the HID type below.
const GESTURE: CGEventType = CGEventType(29);

/// The undocumented fields, named. See this module's header for the sources.
mod field {
    use objc2_core_graphics::CGEventField;

    /// Which `IOHIDEventType` this gesture is.
    pub const HID_TYPE: CGEventField = CGEventField(110);
    /// The magnification delta, as a fraction.
    pub const MAGNIFICATION: CGEventField = CGEventField(113);
    /// The rotation delta, in degrees.
    pub const ROTATION: CGEventField = CGEventField(114);
    /// `IOHIDEventPhaseBits`.
    pub const PHASE: CGEventField = CGEventField(132);
}

/// `IOHIDEventType`, from `IOHIDEventTypes.h`.
mod hid_type {
    pub const ROTATION: i64 = 5;
    pub const ZOOM: i64 = 8;
    pub const ZOOM_TOGGLE: i64 = 22;
}

/// Pinch by a fraction of the current scale.
pub fn magnify(amount: f64, phase: GesturePhase) -> Result<()> {
    post(
        hid_type::ZOOM,
        Some((field::MAGNIFICATION, amount)),
        Some(phase),
    )
}

/// Twist, in degrees.
pub fn rotate(degrees: f64, phase: GesturePhase) -> Result<()> {
    post(
        hid_type::ROTATION,
        Some((field::ROTATION, degrees)),
        Some(phase),
    )
}

/// The two-finger double tap: one event, no phase, no amount.
///
/// The application decides what it means — a browser zooms to the block under
/// the cursor, Preview fits the page — which is exactly why there is nothing
/// here to tune.
pub fn smart_magnify() -> Result<()> {
    post(hid_type::ZOOM_TOGGLE, None, None)
}

fn post(hid: i64, amount: Option<(CGEventField, f64)>, phase: Option<GesturePhase>) -> Result<()> {
    // No source. A gesture is not attributed to a keyboard or a mouse state,
    // and `CGEventCreate(NULL)` is what both references use.
    let event = CGEvent::new(None)
        .ok_or_else(|| Error::SimulateFailed("failed to create gesture event".into()))?;

    CGEvent::set_type(Some(&event), GESTURE);
    CGEvent::set_integer_value_field(Some(&event), field::HID_TYPE, hid);
    if let Some(phase) = phase {
        CGEvent::set_integer_value_field(Some(&event), field::PHASE, phase.bits());
    }
    if let Some((field, value)) = amount {
        CGEvent::set_double_value_field(Some(&event), field, value);
    }

    // Tagged like everything else this crate injects, so a listener in the same
    // process can tell its own gesture from a finger on the glass.
    provenance::tag_event(&event)?;
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
    Ok(())
}

impl GesturePhase {
    /// `IOHIDEventPhaseBits`, from `IOHIDEventTypes.h:620-629`.
    ///
    /// Bits rather than an ordinal — `Ended` is 4, not 3 — and a value invented
    /// by counting would be `Cancelled`.
    fn bits(self) -> i64 {
        match self {
            GesturePhase::Began => 1,
            GesturePhase::Changed => 2,
            GesturePhase::Ended => 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::GesturePhase;

    /// The one thing that would be silently wrong: `Ended` is a bit, and three
    /// consecutive integers is the shape a careless reimplementation would
    /// produce. An application receiving 3 sees `Began | Changed`.
    #[test]
    fn the_phases_are_bits_and_not_an_ordinal() {
        assert_eq!(GesturePhase::Began.bits(), 1);
        assert_eq!(GesturePhase::Changed.bits(), 2);
        assert_eq!(GesturePhase::Ended.bits(), 4);
        assert_eq!(
            GesturePhase::Began.bits() | GesturePhase::Changed.bits(),
            3,
            "3 is a pair of phases, which is why nothing may be numbered 3"
        );
    }
}
