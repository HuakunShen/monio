# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Test Commands

```bash
# Check compilation (fast)
cargo check

# Check compilation for all examples
cargo check --examples

# Run tests
cargo test

# Build release
cargo build --release

# Run a specific example (macOS requires Accessibility permissions)
cargo run --example basic
cargo run --example drag_detection
cargo run --example simulate
```

### Linux Feature Flags
```bash
# Build with X11 support (default)
cargo build --features x11

# Build with Wayland support (stub implementation)
cargo build --features wayland --no-default-features
```

## Architecture

**monio** is a pure Rust cross-platform input hook library. Its key feature is proper drag detection—distinguishing `MouseDragged` from `MouseMoved` events by tracking button state.

### Core Design: State Tracking

The critical architectural decision is in `src/state.rs`: a global `AtomicU32` mask tracks which buttons/modifiers are currently held. Each platform's listener updates this mask on button press/release events, and checks it on mouse move events:

```
MouseMove event → is_button_held()? → MouseDragged : MouseMoved
```

This fixes a common issue in other libraries where drag events are reported as regular moves.

### Module Structure

```
src/
├── lib.rs          # Public API re-exports
├── event.rs        # Event, EventType, Button, ScrollDirection types
├── error.rs        # Error enum with thiserror
├── state.rs        # Global atomic button/modifier mask (THE KEY FIX)
├── keycode.rs      # Key enum for all keyboard keys
├── hook.rs         # Hook struct, EventHandler trait, listen() function
└── platform/
    ├── mod.rs      # Conditional compilation for OS-specific modules
    ├── macos/      # CGEventTap (objc2 bindings)
    ├── windows/    # SetWindowsHookEx (windows crate)
    └── linux/
        ├── x11/    # XRecord + XTest
        └── wayland/ # Stub (libei not yet implemented)
```

### Platform Implementations

Each platform module exports the same interface:
- `run_hook()` - Blocking event loop
- `stop_hook()` - Signal loop to stop
- `simulate()` - Inject events
- `key_press/release/tap()`, `mouse_press/release/click/move()` - Convenience functions

**macOS**: Uses `objc2-core-graphics` for CGEventTap. The `#![allow(unsafe_op_in_unsafe_fn)]` directive is needed for Rust 2024 edition compatibility with the objc2 APIs.

**Windows**: Uses the `windows` crate with low-level hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`).

**Linux**: X11 uses XRecord for listening and XTest for simulation. Wayland support is stubbed out (would require libei/reis integration).

### Key Files When Debugging Drag Detection

1. `src/state.rs` - The atomic mask and `is_button_held()` check
2. `src/platform/*/listen.rs` - Where `set_mask()`/`unset_mask()` are called on button events
3. The mouse move handler in each platform's listener that decides between `MouseDragged` and `MouseMoved`
