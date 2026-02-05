//! Basic example demonstrating event listening.
//!
//! Run with: cargo run --example basic
//!
//! Note: On macOS, you need to grant Accessibility permissions to the terminal.

use monio::{Event, EventType, listen};

fn main() {
    println!("monio basic example");
    println!("Press Ctrl+C to exit\n");

    if let Err(e) = listen(|event: &Event| match event.event_type {
        EventType::HookEnabled => {
            println!("Hook enabled!");
        }
        EventType::HookDisabled => {
            println!("Hook disabled!");
        }
        EventType::KeyPressed => {
            if let Some(kb) = &event.keyboard {
                println!("Key pressed: {:?} (raw: {})", kb.key, kb.raw_code);
            }
        }
        EventType::KeyReleased => {
            if let Some(kb) = &event.keyboard {
                println!("Key released: {:?}", kb.key);
            }
        }
        EventType::MousePressed => {
            if let Some(mouse) = &event.mouse {
                println!(
                    "Mouse pressed: {:?} at ({:.0}, {:.0})",
                    mouse.button, mouse.x, mouse.y
                );
            }
        }
        EventType::MouseReleased => {
            if let Some(mouse) = &event.mouse {
                println!(
                    "Mouse released: {:?} at ({:.0}, {:.0})",
                    mouse.button, mouse.x, mouse.y
                );
            }
        }
        EventType::MouseMoved => {
            if let Some(mouse) = &event.mouse {
                println!("Mouse moved to ({:.0}, {:.0})", mouse.x, mouse.y);
            }
        }
        EventType::MouseDragged => {
            if let Some(mouse) = &event.mouse {
                println!("Mouse DRAGGED to ({:.0}, {:.0})", mouse.x, mouse.y);
            }
        }
        EventType::MouseWheel => {
            if let Some(wheel) = &event.wheel {
                println!("Wheel: {:?} delta={:.1}", wheel.direction, wheel.delta);
            }
        }
        _ => {}
    }) {
        eprintln!("Error: {}", e);
    }
}
