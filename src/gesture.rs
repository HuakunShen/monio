//! Pinch and twist — the gestures a trackpad has and a mouse does not.
//!
//! Separate from [`mod@crate::scroll`] because they are a different event on the
//! wire and a different thing to an application. A scroll moves content; a
//! magnification changes its scale, and every macOS application that can zoom
//! at all reads these events, including ones with no zoom command in any menu.
//!
//! # This is not `⌃`+scroll
//!
//! Holding Control and scrolling is how zoom is asked for on Windows, and web
//! pages have taught a generation of people that it is universal. It is not.
//! On macOS that combination drives the accessibility screen zoom when it is
//! turned on, and does nothing when it is not; a native application zooming
//! from it is the exception. What every application does respond to is the
//! magnification event below, because that is what the trackpad sends.
//!
//! # Where the zoom lands
//!
//! Nowhere in particular, which is the useful answer: the event carries no
//! coordinates, and the application zooms around the pointer, exactly as it
//! does for a finger. A caller that wants to zoom somewhere specific moves the
//! cursor there first.
//!
//! # Platform support
//!
//! | Platform | State |
//! |---|---|
//! | macOS | implemented — `NSEventTypeGesture` carrying `kIOHIDEventTypeZoom` |
//! | Windows | not implemented — Precision Touchpad gestures are delivered by the driver stack and `SendInput` cannot inject one; the nearest thing is `WM_GESTURE`, which is per-window |
//! | X11 | not implemented — XTest has no gesture concept; XInput 2.4 pinch events cannot be faked through it |
//! | Wayland | not implemented — libei has no pinch interface today |
//! | HarmonyOS | not surveyed |

use crate::error::Result;

/// Where in a gesture one event sits.
///
/// The same three-part shape as [`crate::ScrollPhase`]'s finger half, and
/// deliberately not the same type: a scroll has momentum phases and a
/// magnification has none, so sharing the enum would offer four variants that
/// cannot be sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GesturePhase {
    /// Fingers down. Usually carries a zero amount.
    Began,
    /// Fingers moving.
    Changed,
    /// Fingers lifted. Usually carries a zero amount.
    Ended,
}

/// Pinch, by a fraction of the current scale.
///
/// `amount` is a *delta*, not a scale: 0.02 makes whatever is under the cursor
/// two percent bigger, and the sum over a gesture is roughly the total change.
/// A real trackpad sends a stream of small values at report rate; one large
/// value is a jump, and applications that animate their zoom will show it as
/// one.
///
/// An `Ended` with a zero amount is not optional. An application that has been
/// told a pinch began and never told it stopped can leave a zoom gesture live —
/// the same contract as a scroll phase.
///
/// ```no_run
/// # fn main() -> monio::Result<()> {
/// use monio::GesturePhase;
/// monio::magnify(0.0, GesturePhase::Began)?;
/// monio::magnify(0.05, GesturePhase::Changed)?;
/// monio::magnify(0.0, GesturePhase::Ended)?;
/// # Ok(())
/// # }
/// ```
pub fn magnify(amount: f64, phase: GesturePhase) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        crate::platform::magnify(amount, phase)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (amount, phase);
        Err(unsupported("magnify"))
    }
}

/// Twist, in degrees, positive counter-clockwise.
///
/// The same phase contract as [`magnify`]. Fewer applications listen — Preview,
/// Photos and most drawing tools do — but the event costs the same to send and
/// an application that ignores it is not disturbed by it.
pub fn rotate(degrees: f64, phase: GesturePhase) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        crate::platform::rotate(degrees, phase)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (degrees, phase);
        Err(unsupported("rotate"))
    }
}

/// The two-finger double tap that zooms to fit and back.
///
/// One event with no amount and no phase: the application decides what to zoom
/// to, which is why it lands sensibly in a browser, a PDF and a map without
/// the sender knowing which it is talking to.
pub fn smart_magnify() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        crate::platform::smart_magnify()
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(unsupported("smart_magnify"))
    }
}

#[cfg(not(target_os = "macos"))]
fn unsupported(what: &str) -> crate::error::Error {
    crate::error::Error::NotSupported(format!(
        "{what} is implemented for macOS only; {} has no injectable pinch — \
         see this module's documentation",
        std::env::consts::OS,
    ))
}
