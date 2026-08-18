//! Continuous scrolling, with the phases a trackpad sends.
//!
//! # Why this is not `mouse_scroll`
//!
//! `simulate::mouse_scroll` creates a scroll event and posts it, and that is
//! all it does. An application receiving one sees an isolated pixel delta with
//! nothing to say whether it belongs to a gesture, whether the gesture is over,
//! or whether it is the decaying tail of a flick. macOS applications behave
//! differently when they know: `NSScrollView` rubber-bands at a document edge
//! only during a gesture it has been told the boundaries of, and web content
//! decides between smooth and notched scrolling on exactly these fields.
//!
//! So this sets three things `mouse_scroll` does not:
//!
//! - `kCGScrollWheelEventIsContinuous` — this is a surface, not a notched wheel.
//! - `kCGScrollWheelEventScrollPhase` — began / changed / ended, for the part
//!   of the gesture a finger is actually on the glass for.
//! - `kCGScrollWheelEventMomentumPhase` — begin / continue / end, for the part
//!   after it has left.
//!
//! # What phases do *not* do
//!
//! They do not make the system generate momentum. This is worth stating because
//! it is the natural assumption and it is wrong: on a real trackpad the
//! momentum events are synthesised by the driver, not by the window server, and
//! an injector that sets `MomentumPhase` without also sending decaying deltas
//! produces one event and then silence. The phase is a *label*. The decay stays
//! the caller's job.
//!
//! # Prior art read before writing this
//!
//! - Mos (`Mos/Utils/ScrollUtils.swift`) and LinearMouse both synthesise the
//!   decay themselves and tag it, which is the arrangement described above.
//! - `kdeconnect-kde/plugins/mousepad/macosremoteinput.mm` does *not* do this:
//!   it posts plain wheel events, which is why its scrolling feels like a mouse
//!   on a machine whose every other scroll is a trackpad's.

use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventSource, CGEventSourceStateID, CGScrollEventUnit,
};

use crate::error::{Error, Result};
use crate::scroll::ScrollPhase;

use super::provenance;

/// `CGScrollPhase`, which is not an enum in the bindings.
mod cg_scroll_phase {
    pub const BEGAN: i64 = 1;
    pub const CHANGED: i64 = 2;
    pub const ENDED: i64 = 4;
}

/// `CGMomentumScrollPhase`.
mod cg_momentum_phase {
    pub const NONE: i64 = 0;
    pub const BEGIN: i64 = 1;
    pub const CONTINUE: i64 = 2;
    pub const END: i64 = 3;
}

/// Post one scroll event.
///
/// `delta_x` and `delta_y` are in points, positive up and left — the same signs
/// `CGEventCreateScrollWheelEvent2` uses. Fractions below a point are dropped
/// from the integer axis fields and preserved in the fixed-point ones; a caller
/// scrolling slowly should still carry its own residual, because an application
/// that reads only the integer fields sees nothing at all below one point.
pub fn scroll(delta_x: f64, delta_y: f64, phase: ScrollPhase) -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .ok_or_else(|| Error::SimulateFailed("failed to create event source".into()))?;

    // Two axes, vertical first: `wheel1` is axis 1 (vertical) and `wheel2` is
    // axis 2 (horizontal). Getting these the wrong way round scrolls sideways
    // for a vertical gesture, which is the one bug in this file that a test
    // cannot see and a person notices instantly.
    let event = CGEvent::new_scroll_wheel_event2(
        Some(&source),
        CGScrollEventUnit::Pixel,
        2,
        delta_y as i32,
        delta_x as i32,
        0,
    )
    .ok_or_else(|| Error::SimulateFailed("failed to create scroll event".into()))?;

    let (scroll_phase, momentum_phase) = phase.fields();

    CGEvent::set_integer_value_field(
        Some(&event),
        CGEventField::ScrollWheelEventIsContinuous,
        1,
    );
    if scroll_phase != 0 {
        CGEvent::set_integer_value_field(
            Some(&event),
            CGEventField::ScrollWheelEventScrollPhase,
            scroll_phase,
        );
    }
    CGEvent::set_integer_value_field(
        Some(&event),
        CGEventField::ScrollWheelEventMomentumPhase,
        momentum_phase,
    );

    // The sub-point remainder, for applications that read the fixed-point
    // fields. `set_double_value_field` writes the fixed-point representation.
    CGEvent::set_double_value_field(
        Some(&event),
        CGEventField::ScrollWheelEventFixedPtDeltaAxis1,
        delta_y,
    );
    CGEvent::set_double_value_field(
        Some(&event),
        CGEventField::ScrollWheelEventFixedPtDeltaAxis2,
        delta_x,
    );

    provenance::tag_event(&event)?;
    // The HID tap, unlike text: a scroll *is* hardware-shaped. Nothing about it
    // needs interpreting by a layout or an input method, and posting it
    // upstream is what makes it indistinguishable from the trackpad's own.
    CGEvent::post(objc2_core_graphics::CGEventTapLocation::HIDEventTap, Some(&event));
    Ok(())
}

impl ScrollPhase {
    /// `(kCGScrollWheelEventScrollPhase, kCGScrollWheelEventMomentumPhase)`.
    ///
    /// A zero scroll phase means "do not set the field at all" rather than a
    /// phase named zero: an event carrying `ScrollPhase = 0` alongside a
    /// momentum phase is not a shape the system ever produces, and some
    /// applications read the pair rather than either field alone.
    fn fields(self) -> (i64, i64) {
        match self {
            ScrollPhase::Discrete => (0, cg_momentum_phase::NONE),
            ScrollPhase::Began => (cg_scroll_phase::BEGAN, cg_momentum_phase::NONE),
            ScrollPhase::Changed => (cg_scroll_phase::CHANGED, cg_momentum_phase::NONE),
            ScrollPhase::Ended => (cg_scroll_phase::ENDED, cg_momentum_phase::NONE),
            ScrollPhase::MomentumBegan => (0, cg_momentum_phase::BEGIN),
            ScrollPhase::MomentumChanged => (0, cg_momentum_phase::CONTINUE),
            ScrollPhase::MomentumEnded => (0, cg_momentum_phase::END),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping is the part a reader has to trust, and it is also the part
    /// that would be silently wrong if the two enums were confused — every
    /// value in both is a small integer, so a swap compiles.
    #[test]
    fn a_finger_on_the_glass_and_a_finger_gone_never_share_a_field() {
        for phase in [ScrollPhase::Began, ScrollPhase::Changed, ScrollPhase::Ended] {
            let (scroll, momentum) = phase.fields();
            assert_ne!(scroll, 0, "{phase:?} must carry a scroll phase");
            assert_eq!(momentum, cg_momentum_phase::NONE, "{phase:?} is not momentum");
        }
        for phase in [
            ScrollPhase::MomentumBegan,
            ScrollPhase::MomentumChanged,
            ScrollPhase::MomentumEnded,
        ] {
            let (scroll, momentum) = phase.fields();
            assert_eq!(scroll, 0, "{phase:?} has no finger on the glass");
            assert_ne!(momentum, cg_momentum_phase::NONE, "{phase:?} must be momentum");
        }
        assert_eq!(ScrollPhase::Discrete.fields(), (0, cg_momentum_phase::NONE));
    }
}
