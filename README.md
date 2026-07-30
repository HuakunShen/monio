# monio-rs

[![Crates.io](https://img.shields.io/crates/v/monio.svg)](https://crates.io/crates/monio)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/HuakunShen/monio)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A pure Rust cross-platform input hook library with **proper drag detection**.

## Features

- **Cross-platform**: macOS, Windows, Linux (X11/evdev), and a compile-checked
  HarmonyOS PC/2in1 backend
- **Proper drag detection**: Distinguishes `MouseDragged` from `MouseMoved` events
- **Event grabbing**: Block events from reaching other applications (global hotkeys)
- **Async/Channel support**: Non-blocking event receiving with std or tokio channels
- **Event recording & playback**: Record and replay macros (requires `recorder` feature)
- **Input statistics**: Analyze typing speed, mouse distance, etc. (requires `statistics` feature)
- **Display queries**: Get monitor info, DPI scale, system settings (multi-monitor support)
- **Rust API**: Platform backends use native Rust bindings
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
monio = "0.2"
```

### Feature Flags

```toml
# Default (X11 on Linux)
monio = "0.2"

# Async channel support with Tokio
monio = { version = "0.2", features = ["tokio"] }

# Event recording and playback (macro scripts)
monio = { version = "0.2", features = ["recorder"] }

# Input statistics collection
monio = { version = "0.2", features = ["statistics"] }

# All features
monio = { version = "0.2", features = ["tokio", "recorder", "statistics"] }

# Linux: evdev support (works on X11 AND Wayland)
monio = { version = "0.2", features = ["evdev"], default-features = false }
```

### AI agent skill

Install the monio skill so agents can provide usage guidance from this repo:

```bash
npx skills add https://github.com/HuakunShen/monio/skills --skill monio
```

## Using monio in practice

- [kunkunsh/tauri-plugin-user-input](https://github.com/kunkunsh/tauri-plugin-user-input.git): A Tauri plugin for exposing global keyboard and mouse input hooks to desktop apps.
- [HuakunShen/monio-napi](https://github.com/HuakunShen/monio-napi.git): Node.js N-API bindings that wrap monio for JavaScript/TypeScript usage. Mainly for Electron apps.

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

| Platform      | Grab Support     | Notes                                               |
| ------------- | ---------------- | --------------------------------------------------- |
| macOS         | ✅ Full          | Via CGEventTap                                      |
| Windows       | ✅ Full          | Via low-level hooks                                 |
| Linux/X11     | ✅ Active        | XGrabKeyboard/XGrabPointer with XTest pass-through  |
| Linux/Wayland | ⚠️ Limited       | See [Wayland Limitation](#wayland-limitation) below |
| HarmonyOS PC  | ⚠️ Keyboard only | API 26+; compile-checked, pointer observe-only       |

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

| Event Type      | Description                                 |
| --------------- | ------------------------------------------- |
| `HookEnabled`   | Hook started successfully                   |
| `HookDisabled`  | Hook stopped                                |
| `KeyPressed`    | Key pressed down                            |
| `KeyReleased`   | Key released                                |
| `KeyTyped`      | Character typed (after dead key processing) |
| `MousePressed`  | Mouse button pressed                        |
| `MouseReleased` | Mouse button released                       |
| `MouseClicked`  | Button press + release without movement     |
| `MouseMoved`    | Mouse moved (no buttons held)               |
| `MouseDragged`  | Mouse moved while button held               |
| `MouseWheel`    | Scroll wheel rotated                        |

### Input provenance

Every `Event` includes an `origin: InputOrigin`. Treat
`InputOrigin::Unknown` as an honest lack of evidence, not as proof that the
event came from physical hardware.

On macOS and Windows, every event injected through Monio's simulation API
carries a random process-session tag. macOS stores it in
`CGEventField::EventSourceUserData` and also validates
`CGEventField::EventSourceUnixProcessID`. Windows stores a 32-bit tag in
`KEYBDINPUT::dwExtraInfo` or `MOUSEINPUT::dwExtraInfo` and accepts it only when
the low-level hook also reports `LLKHF_INJECTED` or `LLMHF_INJECTED`.

The Linux evdev backend creates one process-scoped uinput device before
capture enumeration and retains the exact character-device number of its
`/dev/input/event*` node. Listen mode classifies events from that live device
as `ThisMonioSession`; grab mode excludes the device to prevent pass-through
events from feeding back into the grab loop. Device names are not used as
identity:

```rust
use monio::Event;

fn handle(event: &Event) {
    if event.is_from_this_monio_session() {
        // Monio injected this event in the current process session.
        // Do not re-transmit it as local input.
        return;
    }

    // The event is untagged or its origin is otherwise unknown.
}
```

This is a non-authenticating feedback-loop marker for Monio's own injected
input, not a security or authorization boundary. It does not prove that an
untagged event is physical: another program may synthesize an untagged event,
and privileged, Accessibility-authorized, or suitably positioned software may
be able to imitate source metadata. Other Linux devices remain `Unknown`. The
X11 backend instead keeps one process-scoped XTest client and has XRecord
correlate that client's `XTestFakeInput` requests with the resulting device
events in server order. It can therefore classify Monio's own XTest key,
button, wheel, and pointer events without access to `/dev/input` or
`/dev/uinput`. Requests from other X11 clients and unmatched events remain
`Unknown`.

HarmonyOS Input Kit monitor/hook callbacks do not expose evidence sufficient
to identify this Monio process as the injector. All captured HarmonyOS events
therefore remain `InputOrigin::Unknown`; simultaneous capture and
retransmission must not assume that self-injected events can be filtered.

Run the native macOS, Windows, or Linux/X11 diagnostic. macOS requires
Accessibility permission:

```bash
cargo run --example synthetic_input_detection
```

The command exits unsuccessfully unless the synthesized keyboard and mouse
events are all observed as `Injected { injector: ThisMonioSession }`.

For the platform mechanisms, current verification status, unresolved
hypotheses, and native experiment requirements, see
[`docs/input-provenance-cross-platform-handoff.md`](docs/input-provenance-cross-platform-handoff.md).

## Platform Notes

### macOS

Requires **Accessibility permissions**. The app will prompt for permission on first run, or you can grant it manually in System Preferences → Security & Privacy → Privacy → Accessibility.

### Windows

No special permissions required for hooking. Simulation may require the app to be running as Administrator in some contexts.

### HarmonyOS PC/2in1

The backend targets HarmonyOS PC/2in1 API 26.0.0 or newer and uses Input Kit:

- `ohos.permission.INPUT_MONITORING` for keyboard, mouse, and axis monitoring;
- `ohos.permission.HOOK_KEY_EVENT` for keyboard suppression/pass-through;
- `ohos.permission.CONTROL_DEVICE` for direct key and global mouse injection.

It does not use the legacy `OH_Input_RequestInjection` authorization flow and
does not request `ohos.permission.INTERCEPT_INPUT_EVENT`.

`grab()` can consume keyboard events. Returning `Some(_)` dispatches the
original native keyboard event; it cannot rewrite it. Mouse and wheel events
are delivered to the handler for observation, but `None` cannot consume them.
Pointer position is supported; display enumeration, primary-display lookup,
display-at-point, and system input settings return `Error::NotSupported`.

The backend has passed Rust checks for `aarch64-unknown-linux-ohos` on Linux,
but has not been linked, packaged, signed, or run on a HarmonyOS PC. See
[`docs/harmonyos-pc-input-backend.md`](docs/harmonyos-pc-input-backend.md) for
the SDK split, evidence, exact limitations, and native acceptance matrix.

### Linux

Two backends are available:

**X11 (default)**: Uses XRecord for listen-only capture, active
`XGrabKeyboard`/`XGrabPointer` sessions for `grab()`, and XTest for simulation
and grab pass-through. It works only on X11 and requires no `input` group or
`/dev/uinput` access.

When an X11 grab handler passes a pointer press, Monio yields that complete
pointer gesture to the receiving application and reacquires the pointer after
the application's implicit X11 grab ends. The handler may therefore not receive
intermediate motion or release events for a gesture it chose to pass. Returning
`None` consistently suppresses keyboard, button, wheel, and pointer-motion
events, which is the recommended mode while a CrossFlow source is controlling
another computer.

**evdev**: Reads directly from `/dev/input/event*` devices. Works on both X11 and Wayland!

```bash
# Use evdev backend (for Wayland support)
cargo build --features evdev --no-default-features
```

**evdev permissions**: Requires membership in the `input` group and access to
`/dev/uinput`. The injector is created before capture so its exact identity is
available for provenance classification:

```bash
sudo usermod -aG input $USER
echo 'KERNEL=="uinput", GROUP="input", MODE="0660"' | sudo tee /etc/udev/rules.d/99-uinput.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --name-match=uinput
# Log out and back in for changes to take effect
```

#### Wayland Limitation

On **Wayland**, `grab()` pass-through behavior depends on the compositor and
libinput environment:

- ✅ **Blocking events works**: Events you choose to consume (return `None`) are properly blocked
- ⚠️ **Pass-through may fail**: Events you return as `Some(event)` may not reach other applications

**Why this happens:**
When Monio grabs an evdev device, it intercepts events before the compositor's
input stack sees them. Pass-through requires re-injection through a uinput
virtual device, and compositor policy for those events varies.

**Workarounds:**

- Use **X11** for unprivileged active-grab support
- Use grab only for **consuming/blocking** events, not for selective pass-through
- For global hotkeys on Wayland, consider using your compositor's native hotkey system

Validate pass-through on the target compositor before relying on it.

## Examples

```bash
# Basic event logging
cargo run --example basic

# Drag detection demo
cargo run --example drag_detection

# Event simulation
cargo run --example simulate

# macOS/Windows/Linux X11 self-injection provenance
cargo run --example synthetic_input_detection

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
