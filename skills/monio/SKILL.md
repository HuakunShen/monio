---
name: monio
description: Use the Rust monio crate for global input hooks, drag-aware mouse tracking, event simulation, grabbing, channel-based consumption, and optional macros/statistics features.
---

# monio

Use this skill when working with the [`monio`](https://crates.io/crates/monio) Rust crate for cross-platform input hooks and simulation.

Always start by checking `examples/*.rs` in the repository for the complete pattern and platform notes.

## Core entry points

- `listen(callback)` / `grab(callback)` for direct callback-based hooks
- `channel::listen_channel` (sync) and `channel::listen_async_channel` (tokio) for non-blocking processing
- `Hook::new()` + `run_async` + `stop` for explicit lifecycle control
- `mouse_*` and `key_*` helpers for event simulation

## Example-guided workflows

### 1) Basic event logging
- File: `examples/basic.rs`
- Use `listen(|event| match event.event_type { ... })` to inspect `KeyPressed`, `MouseDragged`, `MouseMoved`, wheel, clicked, and other events.

### 2) Drag detection
- File: `examples/drag_detection.rs`
- Confirm `MouseDragged` is emitted only while a button is held by tracking `MousePressed` → movement transitions.

### 3) Channel-based hooks
- File: `examples/channel_sync.rs`: background worker via `listen_channel` and timed/non-blocking reads
- File: `examples/channel_async.rs`: async stream via `listen_async_channel`, `tokio::select!`, and heartbeats

### 4) Input simulation
- File: `examples/simulate.rs`
- Use `mouse_move`, `mouse_click`, `key_press`, `key_release`, and `key_tap` for automation scripts.

### 5) Global grab / blocking events
- File: `examples/grab.rs`
- Return `None` from `grab` callback to consume an event; return `Some(event)` to pass through.

### 6) Display and position queries
- File: `examples/display.rs`: `displays`, `primary_display`, `system_settings`
- File: `examples/mouse_position.rs`: `mouse_position`, `display_at_point`

### 7) Recorder + statistics
- File: `examples/recorder.rs` (requires `recorder` feature)
- File: `examples/statistics.rs` (requires `statistics` feature)

### 8) Terminal TUI / richer UI handling
- File: `examples/tui_key_displayer.rs`
- Useful reference for threaded event forwarding and rendering live event state.

## Quick snippets

```rust
use monio::{Event, EventType, listen};

listen(|event: &Event| {
    if event.event_type == EventType::MouseDragged {
        if let Some(mouse) = &event.mouse {
            println!("drag at ({}, {})", mouse.x, mouse.y);
        }
    }
})
```

```rust
use monio::{key_tap, Key};

key_tap(Key::KeyA)?; // emits a full tap
```

