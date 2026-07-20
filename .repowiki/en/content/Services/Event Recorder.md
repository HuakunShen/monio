# Event Recorder

`src/recorder.rs` — behind the `recorder` feature flag (pulls in `serde` + `serde_json`). Records timestamped input events and replays them via simulation. Intended for macro scripts, automated testing, and operation tutorials.

## Types

- **`RecordedEvent`**: `{ elapsed: Duration, event: Event }` — time since recording start.
- **`Recording`**: `{ events: Vec<RecordedEvent>, created_at: SystemTime, description: Option<String> }`. Serializable to/from JSON via `save(path)`/`load(path)`.
- **`EventRecorder`**: owns an internal `Hook`, a `recording: Arc<Mutex<Option<Recording>>>`, and a `start_time: Arc<Mutex<Option<Instant>>>`.

## Recording

```rust
use monio::recorder::{EventRecorder, Recording};

let mut recorder = EventRecorder::new();
recorder.start_recording().unwrap();
// ... user performs actions ...
let recording = recorder.stop_recording().unwrap();
recording.save("macro.json").unwrap();
```

`start_recording()` starts a `Hook::run_async` in the background; its callback pushes a `RecordedEvent` (with `elapsed` computed from `start_time`) into the shared `recording`, skipping `HookEnabled`/`HookDisabled` lifecycle events. `EventRecorder::record_for(duration)` is a convenience wrapper that starts, sleeps, then stops.

## Playback

```rust
let recording = Recording::load("macro.json").unwrap();
recording.playback().unwrap();              // original timing
recording.playback_with_speed(2.0).unwrap(); // 2x speed
recording.playback_fast().unwrap();          // as fast as possible, no waiting
```

Playback iterates recorded events, sleeps to preserve (speed-adjusted) inter-event timing, then calls `crate::platform::simulate(&recorded.event)` for each — reusing the same per-OS `simulate()` used by the standalone simulation functions. `HookEnabled`/`HookDisabled` are skipped during playback.

## Concurrency & Error Handling

- `start_recording()` guards against double-start (`Error::AlreadyRunning`) and **only sets `self.running = true` after `hook.run_async()` succeeds** — if starting the underlying hook fails, the recorder never claims to be running.
- Mutex handling is intentionally asymmetric:
  - **Public API** (`stop_recording`): propagates poisoning as `Error::ThreadError("recording mutex poisoned")`.
  - **Background event callback**: uses `if let Ok(...)` and silently skips the event on a poisoned lock — dropping one event is preferable to crashing the recording thread.
- This split (propagate in public API, degrade gracefully in callbacks) is a deliberate pattern also used in [[Statistics Collector]]; it was the outcome of a 2026-02-05 Codex code-review pass (see `.journal/2026-02-05.md`).

## Testing

`cargo test` covers: empty-recording invariants, `with_description`, `duration()` computed from the last event's `elapsed`, and a save→load JSON round-trip via a temp file.
