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
