# Windows

`src/platform/windows/` — implemented with the `windows` crate (`Win32_UI_WindowsAndMessaging`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_UI_HiDpi`, `Win32_Graphics_Gdi`, `Win32_System_Threading`, `Win32_UI_Shell`, `Win32_Foundation` feature flags in `Cargo.toml`).

```
windows/
├── mod.rs        # re-exports display/listen/simulate fns
├── display.rs    # EnumDisplayMonitors + DPI + SystemParametersInfoW
├── keycodes.rs   # Windows virtual-key <-> Key mapping
├── listen.rs     # Low-level hook (WH_KEYBOARD_LL / WH_MOUSE_LL) listen/grab loop
└── simulate.rs   # SendInput-based synthetic input
```

## Listening & Grabbing

Uses `SetWindowsHookEx` with `WH_KEYBOARD_LL` and `WH_MOUSE_LL` — **full grab support**, same tier as macOS. As on every platform, `listen.rs` is where `set_mask()`/`unset_mask()` update global state on button/modifier events and the mouse-move handler decides `MouseDragged` vs `MouseMoved` via `state::is_button_held()`.

## Display Queries

- `EnumDisplayMonitors` for multi-display enumeration.
- DPI scale from `GetDpiForMonitor`, falling back to `GetDpiForSystem` if per-monitor DPI is unavailable.
- Refresh rate from `EnumDisplaySettingsW`.
- System input settings (keyboard repeat rate/delay, mouse sensitivity/acceleration, double-click time) via `SystemParametersInfoW` — Windows has the most complete `SystemSettings` coverage of the three platforms (see [[Display and System Settings]]), including `keyboard_layout`, which is currently Windows-only.

## Simulation

`simulate.rs` builds `INPUT` structs and calls `SendInput` for `key_press/release/tap`, `mouse_press/release/click/move`, and `mouse_position()`.
