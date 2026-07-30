# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Test Commands

```bash
# Check compilation (fast)
cargo check
cargo check --all-features
cargo check --examples

# Run tests
cargo test
cargo test --all-features
cargo test <test_name>              # run a single test by name (e.g. `cargo test test_button_mask_operations`)

# Lint (CI runs both of these with -D warnings)
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo clippy --all-features -- -D warnings

# Build release
cargo build --release

# Docs (CI fails the build on doc warnings)
cargo doc --all-features --no-deps
```

Feature-gated code has unit tests inside `src/recorder.rs` and `src/statistics.rs` (`#[cfg(test)] mod tests`) — these only run under `cargo test --all-features` or `cargo test --features recorder,statistics`.

### Running examples

macOS requires Accessibility permissions (System Settings → Privacy & Security → Accessibility) before any listen/grab example will receive events.

```bash
cargo run --example basic              # event logging
cargo run --example drag_detection     # MouseDragged vs MouseMoved demo
cargo run --example grab               # block specific keys
cargo run --example simulate           # synthesize input
cargo run --example synthetic_input_detection # verify macOS/Windows self-injection provenance
cargo run --features x11 --example x11_grab_detection # native X11 grab diagnostic
cargo run --features x11 --example x11_relative_grab_detection # XI2 relative-motion diagnostic
cargo run --example mouse_position     # query cursor position
cargo run --example display            # monitor/DPI/system-settings query
cargo run --example channel_sync       # non-blocking std mpsc channel
cargo run --example channel_async --features tokio
cargo run --example recorder --features recorder -- record macro.json
cargo run --example recorder --features recorder -- playback macro.json
cargo run --example statistics --features statistics
cargo run --example tui_key_displayer  # ratatui TUI, keys/mouse live view
```

### Linux Feature Flags

Two independent, mutually-exclusive backends; `x11` is the default.

```bash
# X11 (default) — XRecord listen, XI2 active grab, XTest simulate/replay.
# Does not work on Wayland and needs no input-group/uinput permission.
cargo build --features x11

# evdev — reads /dev/input directly, works under both X11 and Wayland sessions.
# Requires membership in the `input` group: sudo usermod -aG input $USER (then re-login).
cargo build --features evdev --no-default-features
```

CI installs `libx11-dev libxi-dev libxtst-dev libevdev-dev` on Ubuntu for these
builds. X11 release binaries dynamically link `libX11.so.6`, `libXi.so.6`, and
`libXtst.so.6`.

### Input provenance work

Before changing input-source classification, synthetic-input detection, or any
Windows/Linux backend for remote-input use, read
[`docs/input-provenance-cross-platform-handoff.md`](docs/input-provenance-cross-platform-handoff.md)
completely. It separates verified behavior from hypotheses, defines the
`Unknown` safety boundary, and lists the native acceptance tests required on
each platform.

### Other feature flags

- `tokio` — adds async channel variants (`channel::listen_async_channel`, `channel::grab_async_channel`) alongside the always-available sync `std::mpsc` ones.
- `recorder` — enables `src/recorder.rs` (`EventRecorder`, `Recording`); pulls in `serde`/`serde_json` since recordings serialize to JSON.
- `statistics` — enables `src/statistics.rs` (`StatisticsCollector`, `EventStatistics`); no extra dependencies.

## Architecture

**monio** is a pure Rust cross-platform input hook library (macOS, Windows, Linux/X11, Linux/evdev). Its key feature is proper drag detection—distinguishing `MouseDragged` from `MouseMoved` events by tracking button state.

### Core Design: State Tracking

The critical architectural decision is in `src/state.rs`: a global `AtomicU32` mask tracks which buttons/modifiers are currently held. Each platform's listener updates this mask on button press/release events, and checks it on mouse move events:

```
MouseMove event → is_button_held()? → MouseDragged : MouseMoved
```

This fixes a common issue in other libraries where drag events are reported as regular moves. `state::reset_mask()` is called at the start of every hook/grab/channel entry point so stale state never leaks across runs.

### Module Structure

```
src/
├── lib.rs          # Public API re-exports
├── event.rs        # Event/InputOrigin plus keyboard, mouse, and wheel data
├── error.rs        # Error enum with thiserror
├── state.rs        # Global atomic button/modifier mask (THE KEY FIX)
├── keycode.rs      # Key enum for all keyboard keys
├── hook.rs         # Hook struct, EventHandler/GrabHandler traits, listen()/grab()
├── channel.rs       # Non-blocking channel-based alternative to Hook callbacks (std mpsc + optional tokio)
├── display.rs        # DisplayInfo/SystemSettings, displays()/primary_display()/display_at_point()/system_settings()
├── recorder.rs        # EventRecorder/Recording — record & playback input as macros (feature = "recorder")
├── statistics.rs       # StatisticsCollector/EventStatistics — typing speed, mouse distance, etc. (feature = "statistics")
└── platform/
    ├── mod.rs      # Conditional compilation for OS-specific modules (cfg(target_os))
    ├── macos/      # CGEventTap (objc2 bindings)
    ├── windows/    # SetWindowsHookEx (windows crate)
    └── linux/
        ├── x11/    # XRecord + XTest (default backend)
        └── evdev/  # /dev/input + uinput (works on Wayland; feature = "evdev")
```

`recorder.rs` and `statistics.rs` are built **on top of** `Hook::run_async` (they own an internal `Hook` and feed its callback), not directly on `platform::*`. Everything else (`hook.rs`, `channel.rs`, `display.rs`) calls straight into `platform::*`.

### Platform Implementations

Each platform module exports the same interface:
- `run_hook()` / `run_grab_hook()` - Blocking event loops (listen vs grab)
- `stop_hook()` - Signal loop to stop
- `simulate()` - Inject events
- `key_press/release/tap()`, `mouse_press/release/click/move()`, `mouse_position()` - Convenience functions
- `displays()`, `primary_display()`, `display_at_point()`, `system_settings()` - Display/system queries

This shared contract is what lets `Hook`, `channel::*`, `EventRecorder`, and `StatisticsCollector` stay platform-agnostic — they never contain `#[cfg(target_os = ...)]` themselves.

**macOS**: Uses `objc2-core-graphics` for CGEventTap. The `#![allow(unsafe_op_in_unsafe_fn)]` directive is needed for Rust 2024 edition compatibility with the objc2 APIs. Full grab support. Requires Accessibility permissions.

**Windows**: Uses the `windows` crate with low-level hooks (`WH_KEYBOARD_LL`, `WH_MOUSE_LL`). Full grab support. No special permissions for hooking; simulation may need Administrator in some contexts.

**Linux**: X11 uses XRecord for absolute-only listening, active
`XGrabKeyboard`/`XGrabPointer` sessions plus XI2 RawMotion for `grab()`, and
XTest for absolute/relative simulation and key/motion replay (default,
X11-only). Pointer buttons use synchronous `SyncPointer`/`ReplayPointer`
pass-through so the receiving application owns the complete local gesture.
Grab motion keeps absolute `MouseData::x/y` and adds raw deltas in
`MouseData::relative`; `listen()` leaves `relative` as `None`. A passed pointer
press yields the complete local pointer gesture until the receiving
application's implicit grab ends; the handler may not receive that gesture's
intermediate motion/release events.
evdev reads `/dev/input` directly and works under both X11 and Wayland, but
**Wayland grab pass-through is unreliable**: consuming events (`None`) works,
but re-injected pass-through events (`Some(event)`) are typically ignored by
libinput, which takes exclusive device access on Wayland. Prefer X11 when
unprivileged active grabbing is required.

### Key Files When Debugging Drag Detection

1. `src/state.rs` - The atomic mask and `is_button_held()` check
2. `src/platform/*/listen.rs` - Where `set_mask()`/`unset_mask()` are called on button events
3. The mouse move handler in each platform's listener that decides between `MouseDragged` and `MouseMoved`

### Error handling and mutex-poison convention

`recorder.rs` and `statistics.rs` follow a deliberate split: **public API methods** (`stop_recording`, `stop`) propagate a poisoned mutex as `Error::ThreadError`; **background event callbacks** silently skip the event on a poisoned lock (`if let Ok(...)`) rather than panic or propagate — dropping one event beats killing the hook thread. Also note the `running` flag is only set to `true` *after* the underlying `hook.run_async()` call succeeds, so a failed start never leaves a collector claiming to be running.

## Downstream consumers

- [kunkunsh/tauri-plugin-user-input](https://github.com/kunkunsh/tauri-plugin-user-input.git) — Tauri plugin exposing monio's hooks to desktop apps.
- [HuakunShen/monio-napi](https://github.com/HuakunShen/monio-napi.git) — Node.js N-API bindings, mainly for Electron apps.

An AI-agent skill for this repo lives in `skills/monio/` (installable via `npx skills add https://github.com/HuakunShen/monio/skills --skill monio`).
