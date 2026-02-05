# monio-rs

A pure Rust cross-platform input hook library with **proper drag detection**.

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Features

- **Cross-platform**: macOS, Windows, and Linux (X11/evdev) support
- **Proper drag detection**: Distinguishes `MouseDragged` from `MouseMoved` events
- **Event grabbing**: Block events from reaching other applications (global hotkeys)
- **Async/Channel support**: Non-blocking event receiving with std or tokio channels
- **Event recording & playback**: Record and replay macros (requires `recorder` feature)
- **Input statistics**: Analyze typing speed, mouse distance, etc. (requires `statistics` feature)
- **Display queries**: Get monitor info, DPI scale, system settings (multi-monitor support)
- **Pure Rust**: No C dependencies (uses native Rust bindings)
- **Event simulation**: Programmatically generate keyboard and mouse events
- **Thread-safe**: Atomic state tracking for reliable button/modifier detection

## The Problem This Solves

Most input hooking libraries report all mouse movement as `MouseMoved`, even when buttons are held down. This makes implementing drag-and-drop, drawing applications, or gesture recognition difficult.

**monio-rs** tracks button state globally and emits `MouseDragged` events when movement occurs while any mouse button is pressed:

```
Button Down → Move → Move → Button Up
     ↓         ↓      ↓        ↓
 Pressed   Dragged  Dragged  Released
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
monio = "0.1"
```

### Feature Flags

```toml
# Default (X11 on Linux)
monio = "0.1"

# Async channel support with Tokio
monio = { version = "0.1", features = ["tokio"] }

# Event recording and playback (macro scripts)
monio = { version = "0.1", features = ["recorder"] }

# Input statistics collection
monio = { version = "0.1", features = ["statistics"] }

# All features
monio = { version = "0.1", features = ["tokio", "recorder", "statistics"] }

# Linux: evdev support (works on X11 AND Wayland)
monio = { version = "0.1", features = ["evdev"], default-features = false }
```

## Quick Start

### Listening for Events

```rust
use monio::{listen, Event, EventType};

fn main() {
    listen(|event: &Event| {
        match event.event_type {
            EventType::KeyPressed => {
                if let Some(kb) = &event.keyboard {
                    println!("Key pressed: {:?}", kb.key);
                }
            }
            EventType::MouseDragged => {
                if let Some(mouse) = &event.mouse {
                    println!("Dragging at ({}, {})", mouse.x, mouse.y);
                }
            }
            EventType::MouseMoved => {
                if let Some(mouse) = &event.mouse {
                    println!("Moved to ({}, {})", mouse.x, mouse.y);
                }
            }
            _ => {}
        }
    }).expect("Failed to start hook");
}
```

### Grabbing Events (Block Keys/Mouse)

Use `grab()` to intercept events and optionally prevent them from reaching other applications.
Return `None` to consume an event, or `Some(event)` to pass it through.

```rust
use monio::{grab, Event, EventType, Key};

fn main() {
    grab(|event: &Event| {
        // Block the F1 key
        if event.event_type == EventType::KeyPressed {
            if let Some(kb) = &event.keyboard {
                if kb.key == Key::F1 {
                    println!("Blocked F1!");
                    return None; // Consume - don't pass to other apps
                }
            }
        }
        Some(event.clone()) // Pass through
    }).expect("Failed to start grab");
}
```

**Platform Support for Grabbing:**

| Platform | Grab Support |
|----------|--------------|
| macOS | Full support via CGEventTap |
| Windows | Full support via low-level hooks |
| Linux/X11 | Falls back to listen mode (XRecord cannot grab) |

### Channel-Based Listening (Non-Blocking)

For background processing, use channels instead of callbacks:

```rust
use monio::channel::listen_channel;
use monio::EventType;
use std::time::Duration;

fn main() {
    // Start hook with bounded channel (capacity 100)
    let (handle, rx) = listen_channel(100).expect("Failed to start hook");

    // Process events without blocking
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                if event.event_type == EventType::KeyPressed {
                    println!("Key pressed!");
                }
            }
            Err(_) => {
                // Timeout - do other work
            }
        }
    }
}
```

With Tokio (requires `tokio` feature):

```rust
use monio::channel::listen_async_channel;

#[tokio::main]
async fn main() {
    let (handle, mut rx) = listen_async_channel(100).unwrap();

    while let Some(event) = rx.recv().await {
        println!("{:?}", event.event_type);
    }
}
```

### Simulating Events

```rust
use monio::{key_tap, mouse_move, mouse_click, Key, Button};

fn main() -> monio::Result<()> {
    // Move mouse to position
    mouse_move(100.0, 200.0)?;

    // Click
    mouse_click(Button::Left)?;

    // Type a key
    key_tap(Key::KeyA)?;

    Ok(())
}
```

### Using the Hook Struct (Non-blocking)

```rust
use monio::{Hook, Event};
use std::thread;
use std::time::Duration;

fn main() -> monio::Result<()> {
    let hook = Hook::new();

    // Start in background thread
    hook.run_async(|event: &Event| {
        println!("{:?}", event.event_type);
    })?;

    // Do other work...
    thread::sleep(Duration::from_secs(10));

    // Stop the hook
    hook.stop()?;

    Ok(())
}
```

### Display & System Properties

Query display information and system settings:

```rust
use monio::{displays, primary_display, system_settings};

fn main() -> monio::Result<()> {
    // Get all displays
    let all_displays = displays()?;
    for display in all_displays {
        println!("Display {}: {}x{} @ {:?}Hz",
            display.id,
            display.bounds.width,
            display.bounds.height,
            display.refresh_rate
        );
    }

    // Get primary display
    let primary = primary_display()?;
    println!("Primary scale factor: {}", primary.scale_factor);

    // Get system settings
    let settings = system_settings()?;
    println!("Double-click time: {:?}ms", settings.double_click_time);

    Ok(())
}
```

### Recording & Playback (Macros)

Record user actions and replay them later (requires `recorder` feature):

```rust
use monio::recorder::{EventRecorder, Recording};
use std::time::Duration;

fn main() -> monio::Result<()> {
    // Record for 5 seconds
    println!("Recording for 5 seconds...");
    let recording = EventRecorder::record_for(Duration::from_secs(5))?;
    recording.save("macro.json")?;

    // Playback with original timing
    println!("Replaying...");
    let recording = Recording::load("macro.json")?;
    recording.playback()?;

    // Or playback at 2x speed
    recording.playback_with_speed(2.0)?;

    Ok(())
}
```

### Input Statistics

Collect and analyze input patterns (requires `statistics` feature):

```rust
use monio::statistics::StatisticsCollector;
use std::time::Duration;

fn main() -> monio::Result<()> {
    println!("Collecting statistics for 60 seconds...");

    let stats = StatisticsCollector::collect_for(Duration::from_secs(60))?;

    println!("{}", stats.summary());
    println!("Typing speed: {:.1} keys/min", stats.keys_per_minute());
    println!("Mouse distance: {:.0} pixels", stats.total_mouse_distance);

    if let Some((key, count)) = stats.most_frequent_key() {
        println!("Most pressed key: {:?} ({} times)", key, count);
    }

    if stats.needs_break(Duration::from_secs(30)) {
        println!("You've been typing for 30+ seconds. Consider taking a break!");
    }

    Ok(())
}
```

## Event Types

| Event Type | Description |
|------------|-------------|
| `HookEnabled` | Hook started successfully |
| `HookDisabled` | Hook stopped |
| `KeyPressed` | Key pressed down |
| `KeyReleased` | Key released |
| `KeyTyped` | Character typed (after dead key processing) |
| `MousePressed` | Mouse button pressed |
| `MouseReleased` | Mouse button released |
| `MouseClicked` | Button press + release without movement |
| `MouseMoved` | Mouse moved (no buttons held) |
| `MouseDragged` | Mouse moved while button held |
| `MouseWheel` | Scroll wheel rotated |

## Platform Notes

### macOS

Requires **Accessibility permissions**. The app will prompt for permission on first run, or you can grant it manually in System Preferences → Security & Privacy → Privacy → Accessibility.

### Windows

No special permissions required for hooking. Simulation may require the app to be running as Administrator in some contexts.

### Linux

Two backends are available:

**X11 (default)**: Uses XRecord for event capture and XTest for simulation. Works only on X11.

**evdev**: Reads directly from `/dev/input/event*` devices. Works on both X11 and Wayland!

```bash
# Use evdev backend (for Wayland support)
cargo build --features evdev --no-default-features
```

**evdev permissions**: Requires membership in the `input` group:
```bash
sudo usermod -aG input $USER
# Log out and back in for changes to take effect
```

## Examples

```bash
# Basic event logging
cargo run --example basic

# Drag detection demo
cargo run --example drag_detection

# Event simulation
cargo run --example simulate

# Event grabbing (block specific keys)
cargo run --example grab

# Display information
cargo run --example display

# Channel-based (sync)
cargo run --example channel_sync

# Channel-based (async with tokio)
cargo run --example channel_async --features tokio

# Record and playback macros (requires recorder feature)
cargo run --example recorder --features recorder -- record macro.json
cargo run --example recorder --features recorder -- playback macro.json

# Input statistics (requires statistics feature)
cargo run --example statistics --features statistics
```

