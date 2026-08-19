//! Scrolling as a gesture rather than as a series of notches.
//!
//! [`scroll()`] exists beside `simulate`'s wheel support for the same reason
//! [`crate::text`] exists beside `key_press`: the two describe different things
//! and an application treats them differently.
//!
//! A wheel notch is one discrete step from a device with detents. A trackpad
//! scroll is a *phase* — a finger arrives, moves for a while, leaves, and is
//! usually followed by a decaying tail the driver synthesises. Applications on
//! every platform behave differently when they can tell which they are looking
//! at: rubber-banding at a document edge, smooth versus stepped scrolling, and
//! whether a scroll view should snap all read the phase.
//!
//! **The phase is a label, not a motor.** Marking events as momentum does not
//! make the system generate any: on a real trackpad the decaying deltas come
//! from the driver, and a caller that wants a flick to keep going has to keep
//! sending. What the label buys is that the events it sends are treated as a
//! trackpad's rather than as a burst of wheel notches.
//!
//! # Platform support
//!
//! | Platform | State |
//! |---|---|
//! | macOS | implemented — `kCGScrollWheelEventIsContinuous`, `…ScrollPhase`, `…MomentumPhase` |
//! | Windows | not implemented — needs `SendInput` with `MOUSEEVENTF_WHEEL`/`HWHEEL`, which has no phase concept; Precision Touchpad gestures arrive through a different stack entirely |
//! | X11 | not implemented — needs `XTestFakeButtonEvent` on buttons 4–7, which is notches only |
//! | Wayland | not implemented — needs libei's `ei_scroll` plus `ei_scroll_discrete`; libei does have the axis-stop concept a phase needs |
//! | HarmonyOS | not surveyed |

use crate::error::Result;

/// Where in a gesture one scroll event sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPhase {
    /// No gesture. One isolated step, the way a mouse wheel with detents
    /// behaves. This is what `simulate`'s wheel events have always been.
    Discrete,
    /// A finger has arrived and this is the first delta of the gesture.
    Began,
    /// The finger is still down.
    Changed,
    /// The finger has left. Carries a zero delta as often as not.
    Ended,
    /// The first delta after the finger left — the flick's tail begins.
    MomentumBegan,
    /// The tail continues, decaying.
    MomentumChanged,
    /// The tail has run out.
    MomentumEnded,
}

/// Scroll by a pixel delta, labelled with where in a gesture it sits.
///
/// Positive `delta_y` scrolls content the way a finger moving up a trackpad
/// does; positive `delta_x` is the same for a finger moving left. Both are the
/// signs `CGEventCreateScrollWheelEvent2` uses, and both are the signs the
/// caller has to get right, because there is no acknowledgement to check them
/// against.
///
/// A caller producing sub-pixel deltas should accumulate the remainder itself:
/// an application reading the integer delta fields sees nothing below one
/// point, so a slow drag rounded away every frame scrolls not at all rather
/// than slowly.
///
/// ```no_run
/// # fn main() -> monio::Result<()> {
/// use monio::ScrollPhase;
/// monio::scroll(0.0, -12.0, ScrollPhase::Began)?;
/// monio::scroll(0.0, -30.0, ScrollPhase::Changed)?;
/// monio::scroll(0.0, 0.0, ScrollPhase::Ended)?;
/// # Ok(())
/// # }
/// ```
pub fn scroll(delta_x: f64, delta_y: f64, phase: ScrollPhase) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        crate::platform::scroll(delta_x, delta_y, phase)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (delta_x, delta_y, phase);
        Err(crate::error::Error::NotSupported(format!(
            "phased scrolling is implemented for macOS only; {} needs {}",
            std::env::consts::OS,
            NEEDED
        )))
    }
}

#[cfg(not(target_os = "macos"))]
const NEEDED: &str = "SendInput with MOUSEEVENTF_WHEEL on Windows (no phases), \
                      XTestFakeButtonEvent on X11 (notches only), or ei_scroll on Wayland \
                      — see this module's documentation";

#[cfg(test)]
mod tests {
    use super::ScrollPhase;

    /// Guards the one thing a caller can get wrong without noticing: the seven
    /// variants are not interchangeable, and a `match` that collapses two of
    /// them would compile.
    #[test]
    fn the_seven_phases_are_distinct() {
        let all = [
            ScrollPhase::Discrete,
            ScrollPhase::Began,
            ScrollPhase::Changed,
            ScrollPhase::Ended,
            ScrollPhase::MomentumBegan,
            ScrollPhase::MomentumChanged,
            ScrollPhase::MomentumEnded,
        ];
        for (index, one) in all.iter().enumerate() {
            for other in &all[index + 1..] {
                assert_ne!(one, other);
            }
        }
    }
}
