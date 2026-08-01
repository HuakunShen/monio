use crate::{Error, Result};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{
    CGAssociateMouseAndMouseCursorPosition, CGDirectDisplayID, CGDisplayHideCursor,
    CGDisplayShowCursor, CGError, CGEvent, CGEventSource, CGEventSourceStateID,
    CGGetDisplaysWithPoint, CGMainDisplayID, CGWarpMouseCursorPosition,
};
use std::sync::{Mutex, MutexGuard};

static CAPTURE_STATE: Mutex<Option<CaptureState>> = Mutex::new(None);

struct CaptureState {
    saved_position: CGPoint,
    display_id: CGDirectDisplayID,
    associated: bool,
    position_restored: bool,
    cursor_hidden: bool,
    /// Where the cursor is pinned while captured.
    ///
    /// `CGAssociateMouseAndMouseCursorPosition(false)` is documented to
    /// disconnect the mouse from the cursor *"while an application is in the
    /// foreground"*. This process is a background LaunchAgent and never is one.
    /// The call returns success — the request is accepted — but the window
    /// server is under no obligation to keep the disassociation, and measured on
    /// two Macs on 2026-08-01, it does not: the local cursor went on following
    /// the user's hand for the entire time input was being forwarded to another
    /// machine.
    ///
    /// Deleting the events at the tap does not help either, and the shape of the
    /// symptom is what proves it: **the keyboard was suppressed perfectly while
    /// the cursor kept moving.** Returning NULL from a tap deletes an event from
    /// the stream delivered to applications; it does not roll back the cursor
    /// position the window server has already applied. A key has no separate
    /// visible position state, so deleting it is enough. A pointer does.
    ///
    /// So the cursor is pinned the way the Synergy/Barrier lineage pins it:
    /// hidden, and warped back to this anchor after every movement.
    /// `CGWarpMouseCursorPosition` has no foreground requirement and generates
    /// no events, so it cannot feed itself.
    anchor: CGPoint,
}

impl CaptureState {
    fn is_restored(&self) -> bool {
        self.associated && self.position_restored && !self.cursor_hidden
    }
}

fn capture_state() -> MutexGuard<'static, Option<CaptureState>> {
    CAPTURE_STATE.lock().unwrap_or_else(|poisoned| {
        log::warn!("recovering poisoned macOS relative pointer capture state");
        poisoned.into_inner()
    })
}

fn cg_result(operation: &str, status: CGError) -> Result<()> {
    if status == CGError::Success {
        Ok(())
    } else {
        Err(Error::Platform(format!(
            "{operation} failed with CoreGraphics error {}",
            status.0
        )))
    }
}

fn current_mouse_location() -> Result<CGPoint> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .ok_or_else(|| Error::Platform("failed to create CoreGraphics event source".into()))?;
    let event = CGEvent::new(Some(&source))
        .ok_or_else(|| Error::Platform("failed to create CoreGraphics event".into()))?;
    Ok(CGEvent::location(Some(&event)))
}

fn display_for_point(point: CGPoint) -> CGDirectDisplayID {
    let mut display_id = 0;
    let mut display_count = 0;
    let status = unsafe { CGGetDisplaysWithPoint(point, 1, &mut display_id, &mut display_count) };

    if status == CGError::Success && display_count > 0 {
        display_id
    } else {
        CGMainDisplayID()
    }
}

fn restore_with<A, W, S>(
    state: &mut CaptureState,
    mut associate: A,
    mut warp: W,
    mut show: S,
) -> Result<()>
where
    A: FnMut() -> Result<()>,
    W: FnMut(CGPoint) -> Result<()>,
    S: FnMut(CGDirectDisplayID) -> Result<()>,
{
    let mut first_error = None;

    if !state.associated {
        match associate() {
            Ok(()) => state.associated = true,
            Err(error) => first_error = Some(error),
        }
    }

    if !state.position_restored {
        match warp(state.saved_position) {
            Ok(()) => state.position_restored = true,
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }

    if state.cursor_hidden {
        match show(state.display_id) {
            Ok(()) => state.cursor_hidden = false,
            Err(error) if first_error.is_none() => first_error = Some(error),
            Err(_) => {}
        }
    }

    first_error.map_or(Ok(()), Err)
}

fn restore(state: &mut CaptureState) -> Result<()> {
    restore_with(
        state,
        || {
            cg_result(
                "CGAssociateMouseAndMouseCursorPosition(true)",
                CGAssociateMouseAndMouseCursorPosition(true),
            )
        },
        |point| {
            cg_result(
                "CGWarpMouseCursorPosition",
                CGWarpMouseCursorPosition(point),
            )
        },
        |display_id| cg_result("CGDisplayShowCursor", CGDisplayShowCursor(display_id)),
    )
}

pub(crate) fn begin_relative_pointer_capture() -> Result<()> {
    // Finish any restoration left behind by a previous best-effort Drop.
    end_relative_pointer_capture()?;

    let saved_position = current_mouse_location()?;
    let display_id = display_for_point(saved_position);
    let mut state = CaptureState {
        saved_position,
        display_id,
        associated: true,
        position_restored: true,
        cursor_hidden: false,
        // Pinned where the cursor already is. Anywhere would do — it is hidden —
        // but staying put means that if the hide ever fails, the visible result
        // is a cursor that sits still rather than one that jumps across the
        // screen the instant a crossing starts.
        anchor: saved_position,
    };

    cg_result("CGDisplayHideCursor", CGDisplayHideCursor(display_id))?;
    state.cursor_hidden = true;

    if let Err(error) = cg_result(
        "CGAssociateMouseAndMouseCursorPosition(false)",
        CGAssociateMouseAndMouseCursorPosition(false),
    ) {
        if let Err(cleanup_error) = restore(&mut state) {
            log::error!(
                "failed to restore cursor after relative pointer capture acquisition error: \
                 {cleanup_error}"
            );
        }
        if !state.is_restored() {
            *capture_state() = Some(state);
        }
        return Err(error);
    }

    state.associated = false;
    state.position_restored = false;
    *capture_state() = Some(state);
    Ok(())
}

/// Put the cursor back on its anchor, if a relative capture is active.
///
/// Called for every physical pointer movement while captured. This — not
/// `CGAssociateMouseAndMouseCursorPosition` — is what actually holds the cursor
/// still in a background LaunchAgent; see [`CaptureState::anchor`].
///
/// Cheap and silent by design: `CGWarpMouseCursorPosition` generates no events,
/// so warping from inside an event callback cannot feed itself, and it must stay
/// fast or the event tap is disabled for timing out.
///
/// A no-op when nothing is captured, so the caller does not have to ask first.
pub(crate) fn recenter_captured_pointer() {
    let state = capture_state();
    let Some(state) = state.as_ref() else {
        return;
    };
    if state.position_restored {
        // Not currently captured: this is state left behind for restoration.
        return;
    }
    // Deliberately ignoring the result. This runs per movement event; a failure
    // here is not actionable, and logging it would be a log line per mouse
    // sample.
    let _ = CGWarpMouseCursorPosition(state.anchor);
}

pub(crate) fn end_relative_pointer_capture() -> Result<()> {
    let mut capture_state = capture_state();
    let Some(state) = capture_state.as_mut() else {
        return Ok(());
    };

    let result = restore(state);
    if state.is_restored() {
        *capture_state = None;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{CaptureState, restore_with};
    use crate::Error;
    use objc2_core_foundation::CGPoint;
    use std::cell::Cell;

    #[test]
    fn restoration_retries_only_incomplete_steps() {
        let mut state = CaptureState {
            saved_position: CGPoint::new(120.0, 80.0),
            display_id: 7,
            associated: false,
            position_restored: false,
            cursor_hidden: true,
        };
        let associate_attempts = Cell::new(0);
        let warp_attempts = Cell::new(0);
        let show_attempts = Cell::new(0);

        let first = restore_with(
            &mut state,
            || {
                associate_attempts.set(associate_attempts.get() + 1);
                Err(Error::Platform("associate failed".into()))
            },
            |_| {
                warp_attempts.set(warp_attempts.get() + 1);
                Ok(())
            },
            |_| {
                show_attempts.set(show_attempts.get() + 1);
                Ok(())
            },
        );

        assert!(first.is_err());
        assert!(!state.associated);
        assert!(state.position_restored);
        assert!(!state.cursor_hidden);

        restore_with(
            &mut state,
            || {
                associate_attempts.set(associate_attempts.get() + 1);
                Ok(())
            },
            |_| {
                warp_attempts.set(warp_attempts.get() + 1);
                Ok(())
            },
            |_| {
                show_attempts.set(show_attempts.get() + 1);
                Ok(())
            },
        )
        .expect("retry should finish the remaining restoration step");

        assert!(state.is_restored());
        assert_eq!(associate_attempts.get(), 2);
        assert_eq!(warp_attempts.get(), 1);
        assert_eq!(show_attempts.get(), 1);
    }
}
