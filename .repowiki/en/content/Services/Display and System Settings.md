# Display and System Settings

`src/display.rs` — cross-platform monitor geometry and system input-setting queries. Public API delegates straight to `crate::platform::{displays, primary_display, display_at_point, system_settings}` — no OS-specific logic lives in this file.

## Types

- **`Rect`**: `{ x, y, width, height }` in screen coordinates, with `contains(x, y) -> bool` (half-open: `x < self.x + self.width`).
- **`DisplayInfo`**: `{ id: u32, bounds: Rect, scale_factor: f64, refresh_rate: Option<u32>, is_primary: bool }`.
- **`SystemSettings`**: `{ keyboard_repeat_rate, keyboard_repeat_delay, mouse_sensitivity, mouse_acceleration, mouse_acceleration_threshold, double_click_time, keyboard_layout }` — every field is `Option<T>` because availability varies per platform.

## Functions

```rust
pub fn displays() -> Result<Vec<DisplayInfo>>;
pub fn primary_display() -> Result<DisplayInfo>;
pub fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>>;
pub fn system_settings() -> Result<SystemSettings>;
```

## Design Decision: Unified API with Optional Fields

Per `.journal/2026-02-05.md`, three designs were considered when this feature was added (originally TODO item #4):

1. **Single unified struct with `Option<T>` fields** — chosen.
2. Fully platform-specific structs (max type safety, worse ergonomics).
3. Common base struct + platform-extension traits.

Option 1 won because it keeps the API surface simple, most consumers only need the common fields (`bounds`, `scale_factor`), and `Option<T>` makes platform gaps explicit at the type level rather than via panics or silent defaults.

## Per-Platform Behavior

| Platform | Displays | Notes |
|---|---|---|
| macOS | `CGGetActiveDisplayList` + `CGDisplayBounds`, multi-display | `scale_factor = CGDisplayPixelsWide / bounds.width`; settings via `CFPreferencesCopyValue` |
| Windows | `EnumDisplayMonitors`, multi-display | DPI via `GetDpiForMonitor` (falls back to `GetDpiForSystem`); refresh rate via `EnumDisplaySettingsW`; settings via `SystemParametersInfoW` |
| Linux/X11 | Xlib, **single display only** (no RandR extension support) | pointer settings via `XGetPointerControl` |
| Linux/evdev | — | returns `Error::NotSupported` for all display/settings queries — evdev is input-only, has no display API |

## Known Gaps (from journal, still open)

- Linux/X11: no multi-monitor support without adding RandR.
- Linux/Wayland: no standard display API exists; would need a different approach entirely.
- Keyboard layout (`SystemSettings::keyboard_layout`) is currently only populated on Windows.
- macOS keyboard-repeat values are in Apple-specific units and are not yet documented/converted to a common unit.
