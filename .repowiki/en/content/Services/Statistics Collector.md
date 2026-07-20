# Statistics Collector

`src/statistics.rs` — behind the `statistics` feature flag (no extra dependencies beyond std). Aggregates input event counts and derived metrics in real time. Use cases called out in the module docs: productivity analysis (WakaTime-style), health reminders (detect prolonged continuous typing), and general user-behavior analysis.

## Types

- **`EventStatistics`**: the accumulated data — event counts per `EventType`, `key_frequency: HashMap<Key, u64>`, `button_clicks: HashMap<Button, u64>`, `total_mouse_distance` (Euclidean, accumulated per move/drag event), timing fields (`start_time`/`end_time`/`first_key_time`/`last_key_time`/`active_typing_duration`), `avg_click_interval`, and scroll totals (`total_vertical_scroll`/`total_horizontal_scroll`).
- **`StatisticsCollector`**: owns an internal `Hook` and `stats: Arc<Mutex<EventStatistics>>`; runs collection in the background.

## Usage

```rust
use monio::statistics::StatisticsCollector;
use std::time::Duration;

let stats = StatisticsCollector::collect_for(Duration::from_secs(60)).unwrap();
println!("Total events: {}", stats.total_events());
println!("Key presses: {}", stats.key_press_count);
println!("Most pressed key: {:?}", stats.most_frequent_key());
println!("Mouse moved: {:.1} pixels", stats.total_mouse_distance);
```

Or manually: `collector.start()` → `collector.snapshot()` (non-destructive read while still collecting) → `collector.stop()` (finalizes `end_time`, returns the final `EventStatistics`).

## Derived Metrics (`EventStatistics` methods)

| Method | Meaning |
|---|---|
| `most_frequent_key()` / `most_frequent_button()` | max by count over the frequency maps |
| `events_per_minute()` / `keys_per_minute()` | rate over `collection_duration()` |
| `mouse_activity_ratio()` | `(moves + presses) / (key presses + mouse presses + moves)` |
| `is_active_recently(duration)` | true if last key or mouse event was within `duration` |
| `needs_break(threshold)` | true if `active_typing_duration > threshold` **and** no >60s pause since the last key |
| `summary()` | human-readable multi-line report |
| `merge(other)` | additively combine two `EventStatistics` (used for combining windows/sessions) |

`active_typing_duration` only accumulates the interval between two key presses when that interval is **under 5 seconds** — this is what makes it "active typing time" rather than raw session duration.

Scroll sign convention (documented directly on the fields): positive `total_vertical_scroll` = scrolled up, positive `total_horizontal_scroll` = scrolled right; both negative for the opposite directions.

## Concurrency & Error Handling

Same pattern as [[Event Recorder]]: `start()` only flips `self.running` to `true` after `hook.run_async()` succeeds; the background callback uses `if let Ok(mut s) = stats.lock()` and silently skips on a poisoned mutex, while `stop()`/`snapshot()` propagate/degrade explicitly (`snapshot()` returns a fresh empty `EventStatistics` rather than erroring if the mutex is poisoned, since it's a best-effort read).

## Testing

`cargo test` covers: empty-stats invariants, single key-press frequency tracking, most-frequent-key tie-breaking, Pythagorean mouse-distance accumulation across two moves, and `merge()` combining two independently-recorded `EventStatistics`.
