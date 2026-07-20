# Linux

`src/platform/linux/` — two independent, feature-gated backends selected at compile time, plus a stub for when neither is enabled.

```
linux/
├── mod.rs        # feature-gated backend selection + module docs (grab/Wayland caveats)
├── keycodes.rs   # shared Linux keycode <-> Key mapping
├── x11/          # XRecord (listen) + XTest (simulate) — feature = "x11", default
│   ├── mod.rs
│   ├── display.rs  # Xlib display enumeration
│   ├── listen.rs   # XRecord event loop
│   └── simulate.rs # XTest synthetic input
└── evdev/        # /dev/input + uinput — feature = "evdev"
    ├── mod.rs
    ├── display.rs  # stub, returns NotSupported
    ├── listen.rs   # evdev event loop
    └── simulate.rs # uinput synthetic input
```

Backend selection in `mod.rs`:
- `x11` feature enabled → `pub use x11::*` (this is the **default**; `Cargo.toml` sets `default = ["x11"]`).
- `evdev` enabled and `x11` disabled → `pub use evdev::*`.
- Neither enabled → an inline `stub` module where every function returns `Error::NotSupported("No Linux backend enabled. Enable 'x11' or 'evdev' feature.")`.

```bash
# X11 (default)
cargo build --features x11

# evdev (works under both X11 and Wayland sessions)
cargo build --features evdev --no-default-features
```

## X11 Backend

- **Listen**: XRecord extension.
- **Grab**: not supported at the protocol level — XRecord is listen-only, so `run_grab_hook` falls back to listen mode (same contract as documented in [[Hook and Events]]).
- **Simulate**: XTest extension.
- **Display**: Xlib enumeration, but **single-display only** — no RandR extension support yet (tracked as a known gap in [[Display and System Settings]]).
- **System settings**: pointer control via `XGetPointerControl` only; most `SystemSettings` fields are `None`.

## evdev Backend

Reads directly from `/dev/input`, which is what makes it usable under Wayland (where X11-specific APIs don't apply). Requires the running user to be in the `input` group:

```bash
sudo usermod -aG input $USER
# log out and back in
```

- **Display/system settings**: not applicable — evdev is input-only. `displays()`, `primary_display()`, `display_at_point()`, `system_settings()` all return `Error::NotSupported`.
- **Simulate**: via `uinput` (a virtual input device).

### Wayland Grab Limitation (important)

Documented directly in `src/platform/linux/mod.rs`'s module docs — this is a fundamental constraint, not a bug to be fixed:

- ✅ **Consuming events works**: returning `None` from a `GrabHandler` successfully blocks the event.
- ❌ **Pass-through does not work reliably**: returning `Some(event)` re-injects via `uinput`, but Wayland compositors route input through **libinput**, which takes exclusive access to physical devices and typically **ignores uinput-injected virtual devices** for security reasons. A re-injected "passed through" event may simply not reach other applications.

Grab mode also needs, beyond the `input` group:

```bash
sudo usermod -aG input $USER
echo 'KERNEL=="uinput", GROUP="input", MODE="0660"' | sudo tee /etc/udev/rules.d/99-uinput.rules
sudo udevadm control --reload-rules
```

**Recommendation from the module docs**: prefer X11 over Wayland when full grab (block + pass-through) is required; on Wayland, restrict grab usage to pure event-consumption (blocking), not selective pass-through.

## Wayland (native protocol)

There is no `wayland` backend module in the current tree — Wayland support is provided *indirectly* via the evdev backend (which is protocol-agnostic since it reads `/dev/input` directly). A native Wayland input protocol integration (e.g. via `libei`/`reis`) has not been implemented.
