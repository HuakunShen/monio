//! Event grabbing example - block specific keys from reaching other apps.
//!
//! Run with: cargo run --example grab
//!
//! This example demonstrates the grab feature which allows you to:
//! - Intercept keyboard and mouse events
//! - Block specific events from reaching other applications
//! - Create global hotkeys
//!
//! IMPORTANT: This will actually block keys! Press Ctrl+C to exit.
//!
//! In this example, the Q, W, and E keys are blocked. Try typing them -
//! they won't work in other applications while this example is running!
//!
//! Platform support:
//! - macOS: Full support (CGEventTap)
//! - Windows: Full support (low-level hooks)
//! - Linux/X11: Falls back to listen mode (XRecord cannot grab)

use monio::{Event, EventType, Key, grab};
use std::sync::atomic::{AtomicU32, Ordering};

static BLOCKED_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    println!("monio grab example");
    println!("===================\n");
    println!("This example blocks the following keys:");
    println!("  - Q key (completely blocked)");
    println!("  - W key (completely blocked)");
    println!("  - E key (completely blocked)");
    println!("\nTry typing q, w, or e - they won't appear in other apps!");
    println!("Press Ctrl+C to exit.\n");

    if let Err(e) = grab(|event: &Event| {
        match event.event_type {
            EventType::KeyPressed => {
                if let Some(kb) = &event.keyboard {
                    match kb.key {
                        Key::KeyQ => {
                            let count = BLOCKED_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
                            println!("BLOCKED Q key! (total blocked: {})", count);
                            return None; // Consume - don't pass to other apps
                        }
                        Key::KeyW => {
                            let count = BLOCKED_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
                            println!("BLOCKED W key! (total blocked: {})", count);
                            return None; // Consume
                        }
                        Key::KeyE => {
                            let count = BLOCKED_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
                            println!("BLOCKED E key! (total blocked: {})", count);
                            return None; // Consume
                        }
                        Key::Escape => {
                            println!("Escape pressed - passing through");
                            // Return Some to pass through
                        }
                        _ => {
                            // Other keys pass through silently
                        }
                    }
                }
            }
            EventType::MousePressed => {
                if let Some(mouse) = &event.mouse {
                    println!(
                        "Mouse {:?} pressed at ({:.0}, {:.0}) - passing through",
                        mouse.button, mouse.x, mouse.y
                    );
                }
            }
            _ => {}
        }

        // Return Some(event) to pass the event through to other apps
        Some(event.clone())
    }) {
        eprintln!("Error: {}", e);
    }
}
