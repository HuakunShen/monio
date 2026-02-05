//! Sync channel example - receive events in the background.
//!
//! Run with: cargo run --example channel_sync
//!
//! This example shows how to use channels to receive events
//! without blocking your main thread.

use monio::EventType;
use monio::channel::listen_channel;
use std::time::Duration;

fn main() {
    println!("monio channel example (sync)");
    println!("==============================\n");
    println!("Events will be received in the background.");
    println!("Press Ctrl+C to exit.\n");

    // Start the hook with a bounded channel (capacity 100)
    let (handle, rx) = listen_channel(100).expect("Failed to start hook");

    println!("Hook started, waiting for events...\n");

    let mut event_count = 0u32;

    loop {
        // Non-blocking receive with timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                event_count += 1;

                match event.event_type {
                    EventType::KeyPressed => {
                        if let Some(kb) = &event.keyboard {
                            println!("[{}] Key pressed: {:?}", event_count, kb.key);
                        }
                    }
                    EventType::KeyReleased => {
                        if let Some(kb) = &event.keyboard {
                            println!("[{}] Key released: {:?}", event_count, kb.key);
                        }
                    }
                    EventType::MousePressed => {
                        if let Some(mouse) = &event.mouse {
                            println!(
                                "[{}] Mouse {:?} pressed at ({:.0}, {:.0})",
                                event_count, mouse.button, mouse.x, mouse.y
                            );
                        }
                    }
                    EventType::MouseDragged => {
                        // Only print every 20th drag event to reduce spam
                        if event_count % 20 == 0 {
                            if let Some(mouse) = &event.mouse {
                                println!(
                                    "[{}] Dragging at ({:.0}, {:.0})",
                                    event_count, mouse.x, mouse.y
                                );
                            }
                        }
                    }
                    EventType::HookEnabled => {
                        println!("[{}] Hook enabled!", event_count);
                    }
                    _ => {
                        // Ignore other events
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // No events - this is where you could do other work
                // For this example, we just continue waiting
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                println!("Channel disconnected, hook stopped.");
                break;
            }
        }
    }

    // Clean up (will happen automatically on Ctrl+C due to Drop)
    let _ = handle.stop();
}
