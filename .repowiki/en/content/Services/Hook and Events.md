# Hook and Events

`src/hook.rs` — the primary entry point most consumers use.

## Types

- **`Hook`**: holds `running: Arc<AtomicBool>` and `thread_handle: RwLock<Option<JoinHandle<()>>>`. `Drop` calls `stop()` automatically if still running.
- **`EventHandler`** trait: `fn handle_event(&self, event: &Event)` — listen-only, cannot consume events. Blanket-implemented for any `Fn(&Event) + Send + Sync`, so closures work directly.
- **`GrabHandler`** trait: `fn handle_event(&self, event: &Event) -> Option<Event>` — return `None` to consume (block) the event, `Some(event)` to pass it through. Also blanket-implemented for closures.

## API Surface

| Function | Blocking? | Can consume events? |
|---|---|---|
| `listen(callback)` | Yes | No |
| `grab(callback)` | Yes | Yes (platform-dependent) |
| `Hook::run(handler)` | Yes | No |
| `Hook::run_async(handler)` | No (background thread) | No |
| `Hook::grab(handler)` | Yes | Yes |
| `Hook::grab_async(handler)` | No (background thread) | Yes |
| `Hook::stop()` | joins the background thread if async | — |
| `Hook::is_running()` | — | — |

All of `run`/`run_async`/`grab`/`grab_async` guard against double-start with `self.running.swap(true, ...)` → `Error::AlreadyRunning`, and call `crate::state::reset_mask()` before starting so no stale button/modifier state leaks from a prior run.

## Grab Platform Support

- **macOS**: full support via CGEventTap.
- **Windows**: full support via low-level hooks (`WH_KEYBOARD_LL`/`WH_MOUSE_LL`).
- **Linux/X11**: not supported at the protocol level (XRecord is listen-only) — `run_grab_hook` falls back to listen mode rather than erroring.
- **Linux/evdev**: can block (consume) events, but pass-through is unreliable under Wayland compositors — see [[Linux]].

## Usage

```rust
use monio::{listen, grab, Event, EventType, Key};

// Listen (pass-through only)
listen(|event: &Event| {
    if event.event_type == EventType::MouseDragged {
        if let Some(mouse) = &event.mouse {
            println!("Dragging at ({}, {})", mouse.x, mouse.y);
        }
    }
}).expect("Failed to start hook");

// Grab (can block events)
grab(|event: &Event| {
    if event.event_type == EventType::KeyPressed {
        if let Some(kb) = &event.keyboard {
            if kb.key == Key::Escape {
                return None; // consume — Escape never reaches other apps
            }
        }
    }
    Some(event.clone()) // pass through everything else
}).expect("Failed to start grab");
```

## Event Model (`src/event.rs`)

- `EventType`: `HookEnabled`/`HookDisabled`, `KeyPressed`/`KeyReleased`/`KeyTyped`, `MousePressed`/`MouseReleased`/`MouseClicked`/`MouseMoved`/`MouseDragged`, `MouseWheel`.
- `Event`: `event_type`, `time: SystemTime`, `mask: u32` (snapshot of the global state mask at event time), plus optional `keyboard: Option<KeyboardData>`, `mouse: Option<MouseData>`, `wheel: Option<WheelData>`.
- `Button`: `Left`/`Right`/`Middle`/`Button4`/`Button5`/`Unknown(u8)`, with `number()`/`from_number()` (1-indexed) conversions.
- `ScrollDirection`: `Up`/`Down`/`Left`/`Right`. Convention documented in `statistics.rs`: positive delta = up/right, negative = down/left.
- All event data types derive `Serialize`/`Deserialize` when the `recorder` feature is enabled (needed for `Recording` JSON round-trips — see [[Event Recorder]]).

`Event::new(event_type)` auto-stamps `time` and `mask` (via `crate::state::get_mask()`); constructors like `Event::key_pressed`, `Event::mouse_dragged`, `Event::mouse_wheel` etc. build fully-populated events for platform code to emit.
