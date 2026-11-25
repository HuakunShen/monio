# Monio GPUI Example (`monio-gpui`)

A cross-platform GPUI GUI example for [`monio`](../../README.md), isolated in its own Cargo workspace so the root `monio` crate remains pure Rust with zero GUI dependencies.

It targets both **Desktop** (macOS, Windows, Linux binary) and **HarmonyOS PC/2in1** (`cdylib` NAPI module + ArkTS Stage-model wrapper under [`ohos/`](ohos/)).

## Features

1. **Monitor & Grab**
   - Start/Stop **Listen** (`Hook::run`) and **Grab** (`Hook::grab`) with a live toggle to block `Key::Escape` while passing all other events through.
   - Live counters (`total_event_count`, `MouseMoved` vs `MouseDragged`, `KeyPressed` + blocked count, and active `monio::state::get_mask()` flags).
   - Event type filters (`KeyPressed`, `KeyReleased`, `MousePressed`, `MouseReleased`, `MouseMoved`, `MouseDragged`, `MouseWheel`) and a scrollable recent-events log showing `EventType`, key/button, coordinates, relative motion delta, wheel direction/delta, and `InputOrigin`.
   - Surfaces `Error::PermissionDenied`, `Error::NotSupported`, and hook errors directly in the status banner.
2. **Simulate**
   - Focusable target text box to observe simulated typing, key taps, and platform-aware Select-All (`Cmd+A` on macOS, `Ctrl+A` elsewhere) + Delete.
   - Absolute (`mouse_move`) and relative (`mouse_move_relative`) pointer movement, left/right click, phased `scroll()`, 10x back-and-forth scroll test, and `media_key()` controls (`VolumeUp`, `VolumeDown`, `Mute`, `PlayPause`).
   - All simulation calls run off the GPUI UI thread so the interface stays responsive.
3. **Displays & Capabilities**
   - Queries `displays()`, `primary_display()`, `mouse_position()`, `display_at_point()`, `system_settings()`, and `RelativePointerCapture` with a **Refresh Snapshot** action and platform capability matrix.

## Running on Desktop

From `examples/gpui-app/`:

```bash
cargo run
```

> **macOS note:** Grant Accessibility permission to your terminal or built binary under **System Settings -> Privacy & Security -> Accessibility** before starting Listen, Grab, or Simulate actions.

## Building for HarmonyOS

1. Check or build the Rust `cdylib` (`libmonio_gpui.so`) for `aarch64-unknown-linux-ohos`:

   ```bash
   cargo check --target aarch64-unknown-linux-ohos
   # Or build with ohrs into ohos/entry/libs:
   ohrs build --arch aarch --dist ohos/entry/libs -- --locked
   ```

2. Install ArkTS dependencies and build the HarmonyOS HAP:

   ```bash
   cd ohos
   ohpm install --all
   devecocli build
   ```

The ArkTS module manifest ([`ohos/entry/src/main/module.json5`](ohos/entry/src/main/module.json5)) targets `["2in1", "phone"]` and declares:
- `ohos.permission.INPUT_MONITORING`
- `ohos.permission.HOOK_KEY_EVENT`
- `ohos.permission.CONTROL_DEVICE`

Open [`ohos/`](ohos/) in DevEco Studio to configure signing and deploy to a HarmonyOS PC/2in1 device (API 26.0.0+ recommended for full `CONTROL_DEVICE` injection support; keyboard grab only, pointer events remain observe-only).
