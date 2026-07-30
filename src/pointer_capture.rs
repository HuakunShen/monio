//! Process-wide relative pointer capture.
//!
//! This lease is intended for remote-control flows that need physical mouse
//! motion without allowing the local cursor to move. Event capture remains a
//! separate concern: start a [`crate::Hook`] or channel hook to receive
//! [`crate::MouseData::relative`] deltas.

use crate::{Error, Result, platform};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static LIFECYCLE: Mutex<()> = Mutex::new(());

#[cfg(all(test, not(target_os = "windows")))]
pub(crate) static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lifecycle() -> MutexGuard<'static, ()> {
    LIFECYCLE.lock().unwrap_or_else(|poisoned| {
        log::warn!("recovering poisoned relative pointer capture lifecycle");
        poisoned.into_inner()
    })
}

/// A process-wide lease that decouples physical pointer motion from the cursor.
///
/// On macOS, acquisition hides the cursor and disassociates mouse movement from
/// the cursor. Releasing or dropping the lease re-associates them, restores the
/// saved cursor position, and shows the cursor.
///
/// Linux's existing X11/evdev capture paths already produce relative motion, so
/// this lease is a no-op there. Windows currently returns
/// [`Error::NotSupported`] until a Raw Input capture path is implemented.
///
/// Only one lease may be active in a process at a time.
#[must_use = "dropping the lease immediately restores normal cursor behavior"]
#[derive(Debug)]
pub struct RelativePointerCapture {
    owns_lease: bool,
}

impl RelativePointerCapture {
    /// Acquire the process-wide relative pointer capture lease.
    pub fn acquire() -> Result<Self> {
        let _lifecycle = lifecycle();
        ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::RelativePointerCaptureAlreadyActive)?;

        if let Err(error) = platform::begin_relative_pointer_capture() {
            ACTIVE.store(false, Ordering::Release);
            return Err(error);
        }

        Ok(Self { owns_lease: true })
    }

    /// Restore normal cursor behavior and release the lease.
    ///
    /// This consumes the guard. If restoration fails, its [`Drop`]
    /// implementation makes one best-effort retry.
    pub fn release(mut self) -> Result<()> {
        let _lifecycle = lifecycle();
        if !self.is_active() {
            self.owns_lease = false;
            return Ok(());
        }

        platform::end_relative_pointer_capture()?;
        ACTIVE.store(false, Ordering::Release);
        self.owns_lease = false;
        Ok(())
    }

    /// Return whether this guard still owns the active process-wide lease.
    pub fn is_active(&self) -> bool {
        self.owns_lease && ACTIVE.load(Ordering::Acquire)
    }
}

impl Drop for RelativePointerCapture {
    fn drop(&mut self) {
        let _lifecycle = lifecycle();
        if !self.is_active() {
            return;
        }

        if let Err(error) = platform::end_relative_pointer_capture() {
            log::error!("failed to restore relative pointer capture on drop: {error}");
        }

        // A subsequent acquisition asks the platform backend to finish any
        // stale partial restoration before starting a new capture.
        ACTIVE.store(false, Ordering::Release);
        self.owns_lease = false;
    }
}

pub(crate) fn release_active() -> Result<()> {
    let _lifecycle = lifecycle();
    if !ACTIVE.load(Ordering::Acquire) {
        return Ok(());
    }

    platform::end_relative_pointer_capture()?;
    ACTIVE.store(false, Ordering::Release);
    Ok(())
}

pub(crate) fn finish_hook_result(result: Result<()>) -> Result<()> {
    let cleanup_result = release_active();

    match (result, cleanup_result) {
        (Err(error), Err(cleanup_error)) => {
            log::error!(
                "relative pointer capture cleanup also failed after hook error: {cleanup_error}"
            );
            Err(error)
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), cleanup_result) => cleanup_result,
    }
}

#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::{RelativePointerCapture, TEST_MUTEX, release_active};

    #[test]
    fn dropping_owner_allows_reacquisition() {
        let _test_guard = TEST_MUTEX.lock().unwrap();

        drop(RelativePointerCapture::acquire().unwrap());
        RelativePointerCapture::acquire()
            .unwrap()
            .release()
            .unwrap();
    }

    #[test]
    fn hook_shutdown_releases_relative_pointer_capture() {
        let _test_guard = TEST_MUTEX.lock().unwrap();
        let owner = RelativePointerCapture::acquire().unwrap();

        release_active().unwrap();

        assert!(!owner.is_active());
        drop(owner);
        RelativePointerCapture::acquire()
            .unwrap()
            .release()
            .unwrap();
    }
}
