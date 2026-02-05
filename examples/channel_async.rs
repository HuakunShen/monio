//! Async channel example with Tokio.
//!
//! Run with: cargo run --example channel_async --features tokio
//!
//! This example shows how to use async channels with Tokio
//! to receive events in an async context.

use monio::EventType;
use monio::channel::listen_async_channel;
use std::time::Duration;
use tokio::time::interval;

#[tokio::main]
async fn main() {
    println!("monio channel example (async/tokio)");
    println!("=====================================\n");
    println!("Events will be received asynchronously.");
    println!("Press Ctrl+C to exit.\n");

    // Start the hook with an async channel
    let (handle, mut rx) = listen_async_channel(100).expect("Failed to start hook");

    println!("Hook started, waiting for events...\n");

    let mut event_count = 0u32;
    let mut heartbeat = interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            // Receive events from the hook
            event = rx.recv() => {
                match event {
                    Some(event) => {
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
                                // Only print every 20th drag event
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
                            _ => {}
                        }
                    }
                    None => {
                        println!("Channel closed, hook stopped.");
                        break;
                    }
                }
            }

            // Periodic heartbeat to show the async loop is responsive
            _ = heartbeat.tick() => {
                println!("... heartbeat (received {} events so far)", event_count);
            }
        }
    }

    // Clean up
    let _ = handle.stop();
}
