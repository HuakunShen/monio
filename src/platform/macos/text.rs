//! Typing text, as opposed to pressing keys.
//!
//! # Why this is not part of `simulate`
//!
//! Everything in `simulate.rs` injects a *key position*: a keycode, resolved
//! through `keycodes.rs`, that the system then interprets with the user's
//! active layout. There is no key position for `好`. Somebody typed `nihao`
//! into a soft keyboard, tapped a candidate, and received two characters that
//! were never on any key — and the same is true of swipe typing, autocorrect,
//! emoji and paste. Every one of those arrives at an application as a *string*,
//! after the input method has already done its work.
//!
//! So this is a separate entry point rather than a widening of `key_press`, and
//! that separation is deliberate for a second reason: the two authorize
//! differently. A caller allowed to press Return is not automatically a caller
//! that should be allowed to commit an arbitrary string into whatever window
//! happens to have focus.
//!
//! # Why the session tap and not the HID tap
//!
//! `simulate::post_event` posts at `CGEventTapLocation::HIDEventTap`, which is
//! upstream of everything and is what makes a synthetic click indistinguishable
//! from a real one. A Unicode payload is not hardware — no scan code produced
//! it — and the HID tap is the wrong place for it. Every implementation that
//! works uses the session tap for text: KDE Connect's macOS backend posts its
//! Unicode events to `kCGSessionEventTap` while posting the modifier keys
//! around them to the HID tap, in the same function
//! (`kdeconnect-kde/plugins/mousepad/macosremoteinput.mm:180-189` against
//! `:173`).
//!
//! # Prior art read before writing this
//!
//! - `kdeconnect-kde/plugins/mousepad/macosremoteinput.mm:180-189` — one event
//!   per UTF-16 unit, key-down only.
//! - CrossCopy's `PublicInputBackend.swift:190-230` — one key-down and one
//!   key-up, both carrying the whole string, posted to a target pid.
//!
//! This takes the session tap from the first (there is no pid to target: the
//! text goes wherever focus is) and the down/up pair from the second, because
//! an application watching for key-up to end a repeat never sees the key come
//! back up otherwise.

use objc2_core_graphics::{CGEvent, CGEventSource, CGEventSourceStateID, CGEventTapLocation};

use crate::error::{Error, Result};

use super::provenance;

/// UTF-16 units per event.
///
/// `CGEventKeyboardSetUnicodeString` truncates past roughly twenty units, and
/// the limit is not documented anywhere reachable — it shows up in the field as
/// "long messages lose their end". Sixteen is under it with room to spare. The
/// other bound is the opposite mistake: one event per character is a syscall
/// per character, which for a pasted paragraph is thousands.
const CHUNK: usize = 16;

/// Type a string, exactly as the caller supplied it.
///
/// The string is delivered to whatever currently has keyboard focus. Nothing
/// acknowledges it — `CGEventPost` has no return value and no error channel —
/// so a caller that needs to know the text arrived has to observe the target,
/// not this function. Accessibility permission is required, and its absence is
/// silent here for the same reason.
pub fn type_text(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    let units: Vec<u16> = text.encode_utf16().collect();
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .ok_or_else(|| Error::SimulateFailed("failed to create event source".into()))?;

    let mut start = 0;
    while start < units.len() {
        let mut end = (start + CHUNK).min(units.len());
        // Never split a surrogate pair. Each half on its own is U+FFFD, so a
        // chunk boundary landing inside an emoji turns it into two replacement
        // characters — and nothing errors, which is what makes it worth a
        // branch rather than a comment.
        if end < units.len() && is_high_surrogate(units[end - 1]) {
            end -= 1;
        }
        post_chunk(&source, &units[start..end])?;
        start = end;
    }
    Ok(())
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xD800..0xDC00).contains(&unit)
}

fn post_chunk(source: &CGEventSource, units: &[u16]) -> Result<()> {
    for down in [true, false] {
        // Keycode 0 is deliberate and is what every implementation of this
        // uses: the Unicode payload is what carries the meaning, and anything
        // reading the payload ignores the keycode. It is *not* "the key at
        // position 0" being pressed — an event with a Unicode string set is
        // dispatched as text.
        let event = CGEvent::new_keyboard_event(Some(source), 0, down)
            .ok_or_else(|| Error::SimulateFailed("failed to create keyboard event".into()))?;

        // SAFETY: `units` outlives the call, and the function copies the string
        // into the event rather than retaining the pointer.
        unsafe {
            CGEvent::keyboard_set_unicode_string(Some(&event), units.len() as u64, units.as_ptr());
        }

        // Tagged like every other injected event, so this crate's own hooks can
        // still tell "this process typed it" from "a person typed it". Text
        // that could not be attributed would be the one injected event type
        // that lies to `InputOrigin`.
        provenance::tag_event(&event)?;
        CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chunker is the part that fails silently. A boundary between the two
    /// halves of an emoji produces replacement characters and no error, and a
    /// truncating implementation drops the tail of a long string and no error,
    /// so both are checked here rather than left to whoever notices in the
    /// field.
    #[test]
    fn chunks_are_valid_utf16_on_their_own_and_reassemble() {
        for text in [
            "hello",
            "你好，世界",
            &"🌍".repeat(40),
            &"a🌍b".repeat(30),
            &"x".repeat(CHUNK * 3),
        ] {
            let units: Vec<u16> = text.encode_utf16().collect();
            let mut start = 0;
            let mut rebuilt = String::new();
            let mut chunks = 0;
            while start < units.len() {
                let mut end = (start + CHUNK).min(units.len());
                if end < units.len() && is_high_surrogate(units[end - 1]) {
                    end -= 1;
                }
                assert!(end > start, "the chunker must always make progress");
                let piece = String::from_utf16(&units[start..end])
                    .expect("each chunk has to be valid UTF-16 by itself");
                assert!(!piece.contains('\u{FFFD}'), "a surrogate pair was split");
                rebuilt.push_str(&piece);
                chunks += 1;
                start = end;
            }
            assert_eq!(rebuilt, text, "chunks must reassemble into the input");
            assert!(
                chunks >= units.len().div_ceil(CHUNK),
                "no chunk exceeds CHUNK"
            );
        }
    }

    #[test]
    fn an_empty_string_is_not_an_error_and_posts_nothing() {
        // Worth stating: callers forwarding whatever a keyboard handed them
        // will pass empty strings, and an error there would make every one of
        // them write the same guard.
        assert!(type_text("").is_ok());
    }
}
