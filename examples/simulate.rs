//! Event simulation example.
//!
//! Run with: cargo run --example simulate
//!
//! WARNING: This will actually move your mouse and type keys!

use monio::{Button, Key, key_press, key_release, key_tap, mouse_click, mouse_move};
use std::thread::sleep;
use std::time::Duration;

fn main() {
    println!("monio simulation example");
    println!("=========================\n");
    println!("WARNING: This will move your mouse and simulate key presses!\n");
    println!("Starting in 3 seconds... (Press Ctrl+C to cancel)\n");

    sleep(Duration::from_secs(3));

    // Example 1: Move mouse
    println!("1. Moving mouse to (100, 100)...");
    if let Err(e) = mouse_move(100.0, 100.0) {
        eprintln!("   Error: {}", e);
    } else {
        println!("   Done!");
    }
    sleep(Duration::from_millis(500));

    // Example 2: Move mouse again
    println!("2. Moving mouse to (200, 200)...");
    if let Err(e) = mouse_move(200.0, 200.0) {
        eprintln!("   Error: {}", e);
    } else {
        println!("   Done!");
    }
    sleep(Duration::from_millis(500));

    // Example 3: Mouse click
    println!("3. Left clicking...");
    if let Err(e) = mouse_click(Button::Left) {
        eprintln!("   Error: {}", e);
    } else {
        println!("   Done!");
    }
    sleep(Duration::from_millis(500));

    // Example 4: Type a key
    println!("4. Pressing and releasing 'A' key...");
    if let Err(e) = key_tap(Key::KeyA) {
        eprintln!("   Error: {}", e);
    } else {
        println!("   Done!");
    }
    sleep(Duration::from_millis(500));

    // Example 5: Modifier + key combination (Shift+A = 'A')
    println!("5. Pressing Shift+A (types uppercase 'A')...");
    if let Err(e) = key_press(Key::ShiftLeft) {
        eprintln!("   Error pressing Shift: {}", e);
    }
    sleep(Duration::from_millis(50));
    if let Err(e) = key_tap(Key::KeyA) {
        eprintln!("   Error pressing A: {}", e);
    }
    if let Err(e) = key_release(Key::ShiftLeft) {
        eprintln!("   Error releasing Shift: {}", e);
    }
    println!("   Done!");

    println!("\nSimulation complete!");
}
