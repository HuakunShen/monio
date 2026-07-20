# System Overview

**monio** (crate name `monio`, repo `monio-rs`) is a pure-Rust, cross-platform input hook library for macOS, Windows, and Linux (X11 and evdev). Its defining feature is **proper drag detection** — it distinguishes `MouseDragged` from `MouseMoved` by tracking global button state, which most competing libraries (e.g. rdev-style hooks) get wrong.

Current version: `0.1.1` (see `Cargo.toml`).

## What It Provides

- **Listening**: `listen()` / `Hook::run()` / `Hook::run_async()` — receive all keyboard/mouse events, pass-through only (cannot block events).
- **Grabbing**: `grab()` / `Hook::grab()` / `Hook::grab_async()` — receive events and optionally consume them (block from reaching other apps). Full support on macOS and Windows; falls back to listen-only on Linux/X11 (XRecord cannot grab).
- **Channels**: `monio::channel` — non-blocking alternatives to callbacks, with sync (`std::sync::mpsc`) and optional async (`tokio`) variants, for both listen and grab modes.
- **Simulation**: `simulate()`, `key_press/release/tap()`, `mouse_press/release/click/move()`, `mouse_position()` — inject synthetic input events.
- **Display/system queries**: `displays()`, `primary_display()`, `display_at_point()`, `system_settings()` — monitor geometry, DPI scale, refresh rate, keyboard/mouse system settings.
- **Recording & playback** (`recorder` feature): `EventRecorder`, `Recording` — record timestamped input and replay it later (macros, automated testing).
- **Statistics** (`statistics` feature): `StatisticsCollector`, `EventStatistics` — typing speed, mouse distance, click intervals, "needs a break" heuristics.

## Downstream Consumers

- [kunkunsh/tauri-plugin-user-input](https://github.com/kunkunsh/tauri-plugin-user-input.git) — Tauri plugin exposing monio's hooks to desktop apps.
- [HuakunShen/monio-napi](https://github.com/HuakunShen/monio-napi.git) — Node.js N-API bindings, mainly for Electron apps.

## Platform Support Matrix

| Capability | macOS | Windows | Linux/X11 | Linux/evdev |
|---|---|---|---|---|
| Listen | ✅ CGEventTap | ✅ `WH_KEYBOARD_LL`/`WH_MOUSE_LL` | ✅ XRecord | ✅ `/dev/input` |
| Grab (consume events) | ✅ | ✅ | ⚠️ falls back to listen | ⚠️ pass-through unreliable on Wayland (libinput bypasses re-injection) |
| Simulate | ✅ | ✅ | ✅ XTest | ✅ uinput |
| Display query | ✅ CoreGraphics | ✅ Win32 (`EnumDisplayMonitors`) | ✅ Xlib (single display only, no RandR) | ❌ `NotSupported` |
| System settings | ✅ `CFPreferencesCopyValue` | ✅ `SystemParametersInfoW` | ⚠️ pointer control only | ❌ `NotSupported` |

For implementation detail see [[Architecture]] and the per-platform pages under [[macOS]], [[Windows]], [[Linux]].

## Module Map

See [[Architecture]] for the full breakdown; at a glance:

- `src/hook.rs` — `Hook`, `EventHandler`, `GrabHandler`, `listen()`, `grab()` → [[Hook and Events]]
- `src/channel.rs` — channel-based variants → [[Channel-Based Events]]
- `src/recorder.rs` — record/playback (feature-gated) → [[Event Recorder]]
- `src/statistics.rs` — stats collection (feature-gated) → [[Statistics Collector]]
- `src/display.rs` — monitor/system settings queries → [[Display and System Settings]]
- `src/state.rs` — the global atomic button/modifier mask (the core drag-detection fix)
- `src/event.rs`, `src/keycode.rs`, `src/error.rs` — shared types
- `src/platform/{macos,windows,linux}/` — per-OS backends → [[macOS]], [[Windows]], [[Linux]]

## Build & Test

```bash
cargo check                # fast compile check
cargo check --examples     # check all examples compile
cargo test                 # run unit tests
cargo build --release      # release build
```

Feature flags: `x11` (default, Linux), `evdev` (Linux, works under Wayland too), `tokio` (async channels), `recorder`, `statistics`. See `Cargo.toml` `[features]`.
