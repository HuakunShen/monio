//! Asking macOS for Accessibility, at the one moment the answer is known.
//!
//! ## Why ask at all, when the rule is not to guess
//!
//! This backend deliberately does not probe TCC to *report* a permission state:
//! a guess dressed as a status is worse than silence, and a real failure is
//! evidence. That rule is kept. What follows is not a probe — it runs only
//! after `CGEventTapCreate` has already returned null, which is the moment the
//! answer stops being a guess.
//!
//! ## Why the API call and not a link to System Settings
//!
//! Because of where this binary lives. A shipping head sits inside
//! `CrossCopy.app/Contents/MacOS/`, and **no user is going to open a Finder
//! window on the inside of an app bundle and drag a Mach-O into Privacy &
//! Security.** On 2026-08-02 that is exactly what it took to get an entry
//! created by hand, and it needed a shell to reveal the file.
//!
//! `AXIsProcessTrustedWithOptions` with the prompt option posts the standard
//! dialog *and registers the calling code with TCC itself*, so the entry that
//! appears is the one the system chose rather than one a person guessed at by
//! dragging. That difference may also matter for correctness: on that same day,
//! a hand-created entry for this exact binary, toggle on, did not lift the
//! denial — while the identical bytes at a path that had been granted the
//! ordinary way worked. That is unexplained, and it is one of the reasons to
//! let the system create its own entry.
//!
//! Prompting is one-shot per process. macOS shows the dialog once per code
//! identity anyway, and a head that reconnects should not be able to turn that
//! into a stream of dialogs.

use objc2_core_foundation::{CFBoolean, CFDictionary, CFString};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    // Untyped at the boundary on purpose: `CFDictionary` is generic in
    // objc2-core-foundation, and the C function takes a plain CFDictionaryRef.
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: Option<&'static CFString>;
}

static PROMPTED: AtomicBool = AtomicBool::new(false);

/// Whether this process may capture and post input right now.
///
/// Cheap and side-effect free — no dialog, no registration. Safe to call from a
/// diagnostic.
pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Ask the user, once per process, and say whether it was asked.
///
/// Returns false when already trusted (nothing to ask), when the prompt has
/// already been shown by this process, or when the key is unavailable — never
/// as an error, because failing to raise a dialog must not turn into a second
/// failure on top of the one that prompted it.
pub fn prompt_once() -> bool {
    if is_trusted() {
        return false;
    }
    if PROMPTED.swap(true, Ordering::SeqCst) {
        return false;
    }
    let Some(key) = (unsafe { kAXTrustedCheckOptionPrompt }) else {
        return false;
    };
    let yes = CFBoolean::new(true);
    // `&*yes` derefs the `CFRetained` to `&CFBoolean`, which is what
    // `from_slices` wants. Clippy reads it as a redundant reborrow and suggests
    // `&[yes]`; that loses the deref and leaves `V` unconstrained, so the
    // suggestion does not compile (E0283). Silenced rather than restructured —
    // spelling out the generic here would be noisier than the lint.
    #[allow(clippy::borrow_deref_ref)]
    let options = CFDictionary::from_slices(&[key], &[&*yes]);
    unsafe { AXIsProcessTrustedWithOptions(&*options as *const _ as *const c_void) };
    true
}
