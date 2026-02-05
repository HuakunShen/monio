//! Global state tracking for button mask and modifiers.
//!
//! This module provides atomic state tracking that persists across events,
//! enabling proper detection of drag events (mouse movement while buttons held).

use std::sync::atomic::{AtomicU32, Ordering};

/// Global modifier/button mask - persists across events.
static MODIFIER_MASK: AtomicU32 = AtomicU32::new(0);

// Button masks (matches libumonio conventions)
/// Left mouse button mask.
pub const MASK_BUTTON1: u32 = 1 << 8;
/// Right mouse button mask.
pub const MASK_BUTTON2: u32 = 1 << 9;
/// Middle mouse button mask.
pub const MASK_BUTTON3: u32 = 1 << 10;
/// Extra button 1 (X1) mask.
pub const MASK_BUTTON4: u32 = 1 << 11;
/// Extra button 2 (X2) mask.
pub const MASK_BUTTON5: u32 = 1 << 12;

// Keyboard modifier masks
/// Shift key mask.
pub const MASK_SHIFT: u32 = 1 << 0;
/// Control key mask.
pub const MASK_CTRL: u32 = 1 << 1;
/// Alt/Option key mask.
pub const MASK_ALT: u32 = 1 << 2;
/// Meta/Command/Windows key mask.
pub const MASK_META: u32 = 1 << 3;
/// Caps Lock mask.
pub const MASK_CAPS_LOCK: u32 = 1 << 4;
/// Num Lock mask.
pub const MASK_NUM_LOCK: u32 = 1 << 5;
/// Scroll Lock mask.
pub const MASK_SCROLL_LOCK: u32 = 1 << 6;

/// All button masks combined.
pub const MASK_ALL_BUTTONS: u32 =
    MASK_BUTTON1 | MASK_BUTTON2 | MASK_BUTTON3 | MASK_BUTTON4 | MASK_BUTTON5;

/// All modifier masks combined.
pub const MASK_ALL_MODIFIERS: u32 = MASK_SHIFT
    | MASK_CTRL
    | MASK_ALT
    | MASK_META
    | MASK_CAPS_LOCK
    | MASK_NUM_LOCK
    | MASK_SCROLL_LOCK;

/// Set bits in the global mask.
#[inline]
pub fn set_mask(mask: u32) {
    MODIFIER_MASK.fetch_or(mask, Ordering::SeqCst);
}

/// Clear bits in the global mask.
#[inline]
pub fn unset_mask(mask: u32) {
    MODIFIER_MASK.fetch_and(!mask, Ordering::SeqCst);
}

/// Get the current mask value.
#[inline]
pub fn get_mask() -> u32 {
    MODIFIER_MASK.load(Ordering::SeqCst)
}

/// Reset the mask to zero.
#[inline]
pub fn reset_mask() {
    MODIFIER_MASK.store(0, Ordering::SeqCst);
}

/// Check if any mouse button is currently held.
#[inline]
pub fn is_button_held() -> bool {
    (get_mask() & MASK_ALL_BUTTONS) != 0
}

/// Check if a specific button is held.
#[inline]
pub fn is_button_pressed(button_mask: u32) -> bool {
    (get_mask() & button_mask) != 0
}

/// Check if Shift is held.
#[inline]
pub fn is_shift_held() -> bool {
    is_button_pressed(MASK_SHIFT)
}

/// Check if Control is held.
#[inline]
pub fn is_ctrl_held() -> bool {
    is_button_pressed(MASK_CTRL)
}

/// Check if Alt/Option is held.
#[inline]
pub fn is_alt_held() -> bool {
    is_button_pressed(MASK_ALT)
}

/// Check if Meta/Command/Windows is held.
#[inline]
pub fn is_meta_held() -> bool {
    is_button_pressed(MASK_META)
}

/// Get the button mask for a button number (1-indexed).
pub fn button_to_mask(button_num: u8) -> u32 {
    match button_num {
        1 => MASK_BUTTON1,
        2 => MASK_BUTTON2,
        3 => MASK_BUTTON3,
        4 => MASK_BUTTON4,
        5 => MASK_BUTTON5,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_mask_operations() {
        reset_mask();
        assert!(!is_button_held());

        set_mask(MASK_BUTTON1);
        assert!(is_button_held());
        assert!(is_button_pressed(MASK_BUTTON1));
        assert!(!is_button_pressed(MASK_BUTTON2));

        set_mask(MASK_BUTTON2);
        assert!(is_button_pressed(MASK_BUTTON1));
        assert!(is_button_pressed(MASK_BUTTON2));

        unset_mask(MASK_BUTTON1);
        assert!(!is_button_pressed(MASK_BUTTON1));
        assert!(is_button_pressed(MASK_BUTTON2));
        assert!(is_button_held());

        unset_mask(MASK_BUTTON2);
        assert!(!is_button_held());
    }

    #[test]
    fn test_modifier_mask_operations() {
        reset_mask();

        set_mask(MASK_SHIFT);
        assert!(is_shift_held());
        assert!(!is_ctrl_held());

        set_mask(MASK_CTRL);
        assert!(is_shift_held());
        assert!(is_ctrl_held());

        reset_mask();
        assert!(!is_shift_held());
        assert!(!is_ctrl_held());
    }

    #[test]
    fn test_button_to_mask() {
        assert_eq!(button_to_mask(1), MASK_BUTTON1);
        assert_eq!(button_to_mask(2), MASK_BUTTON2);
        assert_eq!(button_to_mask(3), MASK_BUTTON3);
        assert_eq!(button_to_mask(4), MASK_BUTTON4);
        assert_eq!(button_to_mask(5), MASK_BUTTON5);
        assert_eq!(button_to_mask(6), 0);
    }
}
