//! # monio
//!
//! A pure Rust cross-platform input monitoring library with proper drag detection.
//!
//! ## Features
//!
//! - Cross-platform support (macOS, Windows, Linux, plus a compile-checked
//!   HarmonyOS PC/2in1 backend)
//! - Proper drag detection (distinguishes `MouseDragged` from `MouseMoved`)
//! - Event grabbing (consume events to prevent them from reaching other apps)
//! - Explicit relative-pointer capture leases for remote-control sessions
//! - Clean, Rust-idiomatic API with traits and enums
//! - Thread-safe design with atomic state tracking
//! - Event simulation support
//! - Input provenance where the active platform exposes reliable evidence
//!
//! The HarmonyOS backend targets PC/2in1 devices running API 26.0.0 or newer.
//! Its generic grab support can consume keyboard events; pointer events remain
//! observe-only. Captured HarmonyOS events report [`InputOrigin::Unknown`].
//!
//! ## Quick Start
//!
//! ### Listening for Events
//!
//! ```no_run
//! use monio::{listen, Event, EventType};
//!
//! listen(|event: &Event| {
//!     match event.event_type {
//!         EventType::MouseDragged => {
//!             if let Some(mouse) = &event.mouse {
//!                 println!("Dragging at ({}, {})", mouse.x, mouse.y);
//!             }
//!         }
//!         EventType::KeyPressed => {
//!             if let Some(kb) = &event.keyboard {
//!                 println!("Key pressed: {:?}", kb.key);
//!             }
//!         }
//!         _ => {}
//!     }
//! }).expect("Failed to start hook");
//! ```
//!
//! ### Grabbing Events (Blocking Keys/Mouse)
//!
//! ```no_run
//! use monio::{grab, Event, EventType, Key};
//!
//! grab(|event: &Event| {
//!     // Block the Escape key
//!     if event.event_type == EventType::KeyPressed {
//!         if let Some(kb) = &event.keyboard {
//!             if kb.key == Key::Escape {
//!                 println!("Blocked Escape key!");
//!                 return None; // Consume the event
//!             }
//!         }
//!     }
//!     Some(event.clone()) // Pass through
//! }).expect("Failed to start grab");
//! ```
//!
//! ## Architecture
//!
//! The library uses global atomic state tracking (see [`state`] module) to
//! maintain button/modifier state across events. This enables proper detection
//! of drag events - when a mouse move occurs while a button is held, we emit
//! `MouseDragged` instead of `MouseMoved`.

pub mod channel;
pub mod display;
pub mod error;
pub mod event;
pub mod hook;
pub mod keycode;
pub mod pointer_capture;
#[cfg(feature = "recorder")]
pub mod recorder;
pub mod state;
#[cfg(feature = "statistics")]
pub mod statistics;

mod platform;

// Re-exports
pub use display::{
    DisplayInfo, Rect, SystemSettings, display_at_point, displays, primary_display, system_settings,
};
pub use error::{Error, Result};
pub use event::{
    Button, Event, EventType, InjectorIdentity, InputOrigin, KeyboardData, MouseData,
    RelativeMotion, ScrollDirection, WheelData,
};
pub use hook::{EventHandler, GrabHandler, Hook, grab, listen};
pub use keycode::Key;
pub use pointer_capture::RelativePointerCapture;
#[cfg(feature = "recorder")]
pub use recorder::{EventRecorder, RecordedEvent, Recording};
#[cfg(feature = "statistics")]
pub use statistics::{EventStatistics, StatisticsCollector};

// Simulation functions
pub use platform::{
    key_press, key_release, key_tap, mouse_click, mouse_move, mouse_move_relative, mouse_position,
    mouse_press, mouse_release, simulate,
};

#[cfg(all(test, not(target_os = "windows")))]
mod relative_pointer_capture_api_tests {
    use super::{Error, RelativePointerCapture};

    #[test]
    fn relative_pointer_capture_is_process_exclusive() {
        let _test_guard = crate::pointer_capture::TEST_MUTEX.lock().unwrap();
        let first =
            RelativePointerCapture::acquire().expect("first capture lease should be acquired");

        let error =
            RelativePointerCapture::acquire().expect_err("a second capture lease must be rejected");
        assert!(matches!(error, Error::RelativePointerCaptureAlreadyActive));

        first
            .release()
            .expect("explicit release should restore native pointer state");
    }
}
