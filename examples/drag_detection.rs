//! Drag detection example - the key feature of monio.
//!
//! This example demonstrates proper drag detection by tracking:
//! - MousePressed (button down)
//! - MouseDragged (movement while button held) - NOT MouseMoved!
//! - MouseReleased (button up)
//!
//! Run with: cargo run --example drag_detection
//!
//! Expected output when dragging:
//!   MousePressed at (100, 100)
//!   MouseDragged at (102, 101)
//!   MouseDragged at (105, 103)
//!   ...
//!   MouseReleased at (200, 150)

use monio::{Event, EventType, listen};
use std::sync::atomic::{AtomicU32, Ordering};

// Track statistics
static DRAG_COUNT: AtomicU32 = AtomicU32::new(0);
static MOVE_COUNT: AtomicU32 = AtomicU32::new(0);

fn main() {
    println!("monio drag detection example");
    println!("==============================\n");
    println!("This example demonstrates the key fix: proper drag vs move detection.\n");
    println!("Instructions:");
    println!("1. Move your mouse without pressing any button - you should see 'Moved' events");
    println!("2. Press and hold a mouse button, then move - you should see 'DRAGGED' events");
    println!("3. Release the button\n");
    println!("Press Ctrl+C to exit\n");

    if let Err(e) = listen(|event: &Event| {
        match event.event_type {
            EventType::MousePressed => {
                if let Some(mouse) = &event.mouse {
                    println!(
                        ">>> PRESSED {:?} at ({:.0}, {:.0})",
                        mouse
                            .button
                            .as_ref()
                            .map(|b| format!("{:?}", b))
                            .unwrap_or_default(),
                        mouse.x,
                        mouse.y
                    );
                }
            }
            EventType::MouseReleased => {
                if let Some(mouse) = &event.mouse {
                    println!(
                        "<<< RELEASED {:?} at ({:.0}, {:.0})",
                        mouse
                            .button
                            .as_ref()
                            .map(|b| format!("{:?}", b))
                            .unwrap_or_default(),
                        mouse.x,
                        mouse.y
                    );
                    // Print stats on release
                    println!(
                        "    Stats - Moves: {}, Drags: {}",
                        MOVE_COUNT.load(Ordering::SeqCst),
                        DRAG_COUNT.load(Ordering::SeqCst)
                    );
                }
            }
            EventType::MouseMoved => {
                MOVE_COUNT.fetch_add(1, Ordering::SeqCst);
                if let Some(mouse) = &event.mouse {
                    // Only print occasionally to avoid spam
                    if MOVE_COUNT.load(Ordering::SeqCst).is_multiple_of(50) {
                        println!("    Moved to ({:.0}, {:.0})", mouse.x, mouse.y);
                    }
                }
            }
            EventType::MouseDragged => {
                DRAG_COUNT.fetch_add(1, Ordering::SeqCst);
                if let Some(mouse) = &event.mouse {
                    // Print every 10th drag event
                    let count = DRAG_COUNT.load(Ordering::SeqCst);
                    if count.is_multiple_of(10) || count <= 3 {
                        println!("*** DRAGGED to ({:.0}, {:.0}) ***", mouse.x, mouse.y);
                    }
                }
            }
            _ => {}
        }
    }) {
        eprintln!("Error: {}", e);
    }
}
