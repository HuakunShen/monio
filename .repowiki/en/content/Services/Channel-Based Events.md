# Channel-Based Events

`src/channel.rs` — non-blocking alternatives to the callback-based `Hook` API, for consumers who want to pull events from a loop (e.g. a UI event loop, a `select!` in async code) instead of receiving them via a callback.

## Functions

| Function | Channel type | Grab? | Feature gate |
|---|---|---|---|
| `listen_channel(capacity)` | bounded `std::sync::mpsc::sync_channel` | No | — |
| `listen_unbounded_channel()` | unbounded `std::sync::mpsc::channel` | No | — |
| `grab_channel(capacity, filter)` | bounded, + sync filter closure | Yes | — |
| `listen_async_channel(capacity)` | `tokio::sync::mpsc` | No | `tokio` |
| `grab_async_channel(capacity, filter)` | `tokio::sync::mpsc`, + sync filter | Yes | `tokio` |

All return `(ChannelHookHandle, Receiver<Event>)`. The hook runs on a dedicated background thread; `ChannelHookHandle::stop()` signals it to stop and joins the thread. Dropping the handle also stops the hook (`impl Drop for ChannelHookHandle`).

## Backpressure Behavior

- Bounded channels use `try_send` — if the consumer is too slow and the buffer fills, **new events are silently dropped** rather than blocking the input hook thread. This is deliberate: a slow consumer must never stall physical keyboard/mouse input.
- The unbounded variant (`listen_unbounded_channel`) never drops events but has unbounded memory growth risk if the consumer falls behind — documented as a caveat in the function's doc comment.

## Grab + Filter Semantics

`grab_channel`/`grab_async_channel` take a `filter: Fn(&Event) -> bool` alongside the channel:

- **Every** event (whether ultimately consumed or passed through) is sent to the channel — the channel is for observation, not for the pass/block decision.
- The filter's return value is what determines the actual grab behavior: `true` → pass through (`Some(event)`), `false` → consume (`None`).
- The filter must decide synchronously and immediately — it runs on the hook's callback path, so it has the same latency constraints as any `GrabHandler`.

```rust
use monio::channel::grab_channel;
use monio::{EventType, Key};

let (handle, rx) = grab_channel(100, |event| {
    if event.event_type == EventType::KeyPressed {
        if let Some(kb) = &event.keyboard {
            if kb.key == Key::F1 {
                return false; // consume F1
            }
        }
    }
    true // pass everything else through
}).expect("Failed to start hook");

for event in rx.iter() {
    println!("{:?}", event.event_type);
}
```

## Implementation Note

Each channel constructor spawns its own thread and calls `crate::platform::run_hook`/`run_grab_hook` directly (not through `Hook`), wrapping an internal `ChannelHandler`/`UnboundedChannelHandler`/`GrabChannelHandler` that implements `EventHandler`/`GrabHandler` and forwards to the channel sender. Like `Hook`, it calls `crate::state::reset_mask()` before starting.
