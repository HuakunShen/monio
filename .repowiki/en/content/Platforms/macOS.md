# macOS

`src/platform/macos/` — implemented with `objc2`/`objc2-core-graphics`/`objc2-core-foundation`/`objc2-foundation` (pure Rust ObjC bindings, no C shim).

```
macos/
├── mod.rs        # re-exports display/listen/simulate fns
├── display.rs    # CoreGraphics display + CFPreferences system settings
├── keycodes.rs   # macOS virtual keycode <-> Key mapping
├── listen.rs     # CGEventTap-based listen/grab loop
└── simulate.rs   # CGEvent-based synthetic input
```

## Listening & Grabbing

Uses `CGEventTap` for both listen and grab modes — **full grab support** (unlike Linux/X11). The mouse-move handler here is one of the three files to check when debugging drag detection (see [[Architecture]]): it consults `state::is_button_held()` to decide `MouseDragged` vs `MouseMoved`, and `set_mask()`/`unset_mask()` are called on button press/release.

**Requires Accessibility permissions** — examples must be granted Accessibility access in System Settings before `cargo run --example basic` etc. will receive events.

## Rust 2024 Edition Note

`#![allow(unsafe_op_in_unsafe_fn)]` is required in this module for compatibility with the `objc2` API surface under the 2024 edition's stricter unsafe-block rules.

## Display Queries

- `CGGetActiveDisplayList` + `CGDisplayBounds` for enumeration (multi-display supported).
- `scale_factor = CGDisplayPixelsWide(display) / bounds.width`.
- Refresh rate via `CGDisplayMode::refresh_rate` (the deprecated `CGDisplayModeGetRefreshRate` free function was replaced during the 2026-02-05 display feature work).
- System settings (keyboard repeat, etc.) read via `CFPreferencesCopyValue`; values are in Apple-specific units (not yet converted/documented — see [[Display and System Settings]]).

## Simulation

`simulate.rs` builds and posts `CGEvent`s for `key_press/release/tap`, `mouse_press/release/click/move`, and `mouse_position()`.
