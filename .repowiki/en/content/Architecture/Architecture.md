# Architecture

## Core Design: Global Atomic State Tracking

The central architectural decision lives in `src/state.rs`: a single `static MODIFIER_MASK: AtomicU32` tracks which mouse buttons and keyboard modifiers are currently held, persisting across individual events.

```
MouseMove event → is_button_held()? → MouseDragged : MouseMoved
```

Each platform's low-level listener calls `set_mask()` / `unset_mask()` on every button-press/release and modifier-change event, and `is_button_held()` (checked against `MASK_ALL_BUTTONS`) on every mouse-move event to decide whether to emit `EventType::MouseDragged` or `EventType::MouseMoved`. This is the fix for a common bug in other input-hook libraries where drag events are misreported as plain moves.

Bit layout (`src/state.rs`):
- Modifier bits 0–6: Shift, Ctrl, Alt, Meta, CapsLock, NumLock, ScrollLock
- Button bits 8–12: Button1 (Left) .. Button5

`state::reset_mask()` is called at the start of every hook/grab/channel entry point (`Hook::run`, `Hook::run_async`, `Hook::grab`, `Hook::grab_async`, `channel::listen_channel`, etc.) so stale state from a previous run never leaks in.

## Module Structure

```
src/
├── lib.rs          # Public API re-exports, crate-level docs
├── event.rs        # Event, EventType, Button, ScrollDirection, KeyboardData/MouseData/WheelData
├── error.rs        # Error enum (thiserror), Result<T> alias
├── state.rs         # Global atomic button/modifier mask — THE KEY FIX
├── keycode.rs       # Key enum for all keyboard keys
├── hook.rs           # Hook struct, EventHandler/GrabHandler traits, listen()/grab()
├── channel.rs        # Channel-based (std mpsc + optional tokio) alternatives to callbacks
├── display.rs         # DisplayInfo, SystemSettings, displays()/primary_display()/...
├── recorder.rs        # EventRecorder, Recording, RecordedEvent (feature = "recorder")
├── statistics.rs      # StatisticsCollector, EventStatistics (feature = "statistics")
└── platform/
    ├── mod.rs      # Conditional compilation dispatch by target_os
    ├── macos/      # CGEventTap (objc2 bindings)
    ├── windows/    # SetWindowsHookEx (windows crate)
    └── linux/
        ├── x11/    # XRecord (listen) + XTest (simulate) — default backend
        └── evdev/  # /dev/input + uinput — works under Wayland, feature = "evdev"
```

`src/platform/mod.rs` is a straight `#[cfg(target_os = "...")]` dispatcher; it `pub use`s exactly one of `macos::*`, `windows::*`, `linux::*` depending on the compile target, and fails the build via `compile_error!` on any other OS.

## Platform Contract

Every platform backend module exports the **same function signatures**, so the rest of the crate (hook.rs, channel.rs, recorder.rs, statistics.rs, display.rs) is platform-agnostic:

```rust
fn run_hook<H: EventHandler>(running: &Arc<AtomicBool>, handler: H) -> Result<()>;
fn run_grab_hook<H: GrabHandler>(running: &Arc<AtomicBool>, handler: H) -> Result<()>;
fn stop_hook() -> Result<()>;
fn simulate(event: &Event) -> Result<()>;
fn key_press/release/tap(key: Key) -> Result<()>;
fn mouse_press/release/click/move(...) -> Result<()>;
fn mouse_position() -> Result<(f64, f64)>;
fn displays() -> Result<Vec<DisplayInfo>>;
fn primary_display() -> Result<DisplayInfo>;
fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>>;
fn system_settings() -> Result<SystemSettings>;
```

This contract is what makes `Hook`, `channel::listen_channel`, `EventRecorder`, and `StatisticsCollector` all reusable across OSes without any `#[cfg]` in their own code — they only call into `crate::platform::*`.

## Layering

```
lib.rs (public API)
  ├─ hook.rs ──────────────┐
  ├─ channel.rs ───────────┤  all call into platform::{run_hook, run_grab_hook, stop_hook, simulate}
  ├─ recorder.rs ──────────┤  (recorder/statistics build on top of Hook, not platform directly)
  ├─ statistics.rs ────────┘
  ├─ display.rs ───────────►  platform::{displays, primary_display, display_at_point, system_settings}
  └─ event.rs / state.rs / keycode.rs / error.rs — shared, OS-independent types
platform/mod.rs → macos | windows | linux (cfg-selected)
```

`recorder::EventRecorder` and `statistics::StatisticsCollector` are built **on top of** `Hook::run_async`, not directly on `platform::*` — they own a `Hook` instance internally and feed its callback into their own event-processing logic (see [[Event Recorder]] and [[Statistics Collector]]).

## Key Files When Debugging Drag Detection

1. `src/state.rs` — the atomic mask and `is_button_held()` check.
2. `src/platform/*/listen.rs` — where `set_mask()`/`unset_mask()` are called on button events.
3. The mouse-move handler in each platform's listener that decides between `MouseDragged` and `MouseMoved`.

## Known Design Constraints

- **Linux/X11 grab**: XRecord is listen-only at the X11 protocol level; `Hook::grab()` on X11 falls back to listen mode rather than failing (see `hook.rs` docs on `GrabHandler`).
- **Linux/Wayland pass-through**: evdev grab can *block* events (return `None`) but cannot reliably *pass through* re-injected events — libinput ignores uinput-injected devices for security reasons. Full detail in `src/platform/linux/mod.rs` module docs and [[Linux]].
- **Mutex poisoning** (`recorder.rs`, `statistics.rs`): public API methods (`stop_recording`, `stop`) propagate poisoning via `Error::ThreadError`; background event callbacks silently drop the event on a poisoned lock (`if let Ok(...)`) rather than panic or propagate, since dropping one event is preferable to killing the hook thread.
- **`running` flag ordering**: in `start_recording()`/`start()` (recorder.rs, statistics.rs) the hook is started via `run_async` *before* `self.running` is set to `true`, so a failed `run_async` never leaves the collector in a "running" state it can't recover from.
