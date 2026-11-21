# Changelog

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
