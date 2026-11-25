# Changelog

## 0.3.0

- Complete the HarmonyOS PC/2-in-1 backend (`target_env = "ohos"`): add display enumeration (`displays`, `primary_display`, `display_at_point`), global `display_id` offset resolution in `mouse_position`, phased `scroll`, `media_key`, `MouseWheel` / `MouseDragged` / relative motion simulation, and explicit `Error::NotSupported` boundaries.
- Add `monio::capabilities()`, `InputCapabilities`, `CaptureScope`, and `InjectionAuthorization` for compile-time backend capability and injection-authorization introspection.
- Add a cross-platform GPUI example app under `examples/gpui-app/` (isolated Cargo workspace, `gpui-component` 0.9.0, Desktop + HarmonyOS).
- Scope `futures-util` to Linux `target_os` dependencies to eliminate unused-dependency warnings on non-Linux targets.

## 0.2.0

- Fix macOS synthetic dragging: pointer moves now emit the appropriate drag event while a simulated mouse button is held. Track click counts for repeated clicks.
- Add relative pointer motion/capture and input-origin metadata, with platform-specific behavior and limitations documented in the input provenance handoff.
- Add macOS text input, phased scrolling, trackpad gestures, and media-key simulation.
- Add an experimental HarmonyOS PC/2-in-1 backend. It is compile-checked, not device-tested.

### Migrating from 0.1.x

- `Event` has an `origin` field. Prefer constructors; update struct literals with `InputOrigin::Unknown` when provenance is unavailable.
- `MouseData` has an optional `relative` field. Update struct literals with `None` when relative motion is unavailable.
- `Error` adds `RelativePointerCaptureAlreadyActive`; exhaustive matches must handle the new variant.
- Backend capabilities are not identical. Consult the platform documentation before relying on provenance, pointer capture, or experimental platform support.

### Packaging

- Include the full Apache-2.0 license and restrict the crate archive to library source, examples, assets, and relevant documentation.
