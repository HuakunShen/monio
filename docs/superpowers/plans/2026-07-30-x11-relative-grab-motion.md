# X11 Relative Grab Motion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make X11 `grab()` report unclipped XI2 relative pointer deltas and make captured relative motion directly replayable on another computer.

**Architecture:** Extend existing mouse events with optional relative data while preserving absolute coordinates and event types. Keep XRecord-based `listen()` unchanged; add a focused XI2 helper to the active-grab path, suppress duplicate core motion callbacks, and use XTest for relative injection. Temporarily disable raw selection during local pass-through so Monio does not capture its own replay.

**Tech Stack:** Rust 2024, x11-rs 2.21 (`xlib`, `xrecord`, `xinput`, `xtest`), XInput2 2.1+, libXi, XTest, serde-gated recorder support, Xvfb/native X11 diagnostics.

## Global Constraints

- X11 `listen()` remains XRecord-based and continues to report `relative: None`.
- A successfully started X11 `grab()` requires XI2 2.1+ and reports relative data for pointer motion.
- `MouseData::x` and `MouseData::y` always remain absolute screen coordinates.
- Relative motion must continue while the core pointer is clipped at a screen edge.
- A physical motion produces one grab-handler callback, not separate core and XI2 callbacks.
- Returning `None` consumes motion without XTest replay.
- Returning `Some(event)` preserves current local pass-through without replay recursion or double movement.
- Legacy recorder JSON without `relative` must deserialize with `relative: None`.
- Existing uncommitted evdev provenance/startup work is committed separately before relative-motion implementation.
- Do not change macOS or Windows capture behavior.
- Every commit stages exact paths so unrelated worktree changes cannot be included accidentally.

---

### Task 1: Land the Existing evdev Provenance Fix Separately

**Files:**
- Modify: `docs/input-provenance-cross-platform-handoff.md`
- Modify: `examples/synthetic_input_detection.rs`
- Modify: `src/platform/linux/evdev/provenance.rs`
- Modify: `src/platform/linux/evdev/simulate.rs`

**Interfaces:**
- Consumes: Existing worktree changes that retry udev node discovery and validate evdev relative loopback.
- Produces: A clean, independently reviewable evdev fix commit before X11 relative-motion files are edited.

- [ ] **Step 1: Review the existing patch and confirm its scope**

Run:

```bash
git diff -- docs/input-provenance-cross-platform-handoff.md examples/synthetic_input_detection.rs src/platform/linux/evdev/provenance.rs src/platform/linux/evdev/simulate.rs
```

Expected: only the already-verified uinput event-node startup retry, evdev provenance handling, relative synthetic diagnostic, and handoff notes.

- [ ] **Step 2: Re-run the evdev-only regression suite**

Run:

```bash
cargo test --no-default-features --features evdev --all-targets
cargo clippy --no-default-features --features evdev --all-targets -- -D warnings
```

Expected: both commands pass.

- [ ] **Step 3: Commit only the evdev patch**

```bash
git add docs/input-provenance-cross-platform-handoff.md examples/synthetic_input_detection.rs src/platform/linux/evdev/provenance.rs src/platform/linux/evdev/simulate.rs
git commit -m "fix(linux): stabilize evdev synthetic provenance"
```

- [ ] **Step 4: Confirm the implementation worktree is clean**

Run:

```bash
git status --short
```

Expected: only this implementation-plan document is untracked or modified.

---

### Task 2: Add Relative Motion to the Public Event Model

**Files:**
- Modify: `src/event.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: Existing `Event`, `MouseData`, `Event::mouse_moved()`, and `Event::mouse_dragged()`.
- Produces: `RelativeMotion { delta_x: f64, delta_y: f64 }`, `MouseData::relative: Option<RelativeMotion>`, `Event::mouse_moved_relative(...)`, and `Event::mouse_dragged_relative(...)`.

- [ ] **Step 1: Write failing event-model tests**

Add tests to `src/event.rs`:

```rust
#[test]
fn absolute_mouse_motion_has_no_relative_delta() {
    let event = Event::mouse_moved(100.0, 200.0);
    assert_eq!(event.mouse.unwrap().relative, None);
}

#[test]
fn relative_mouse_motion_keeps_absolute_position_and_delta() {
    let event = Event::mouse_moved_relative(100.0, 200.0, -3.5, 4.25);
    let mouse = event.mouse.unwrap();
    assert_eq!((mouse.x, mouse.y), (100.0, 200.0));
    assert_eq!(
        mouse.relative,
        Some(RelativeMotion {
            delta_x: -3.5,
            delta_y: 4.25,
        })
    );
}

#[test]
fn relative_drag_uses_drag_event_type() {
    let event = Event::mouse_dragged_relative(10.0, 20.0, 1.0, -2.0);
    assert_eq!(event.event_type, EventType::MouseDragged);
    assert_eq!(
        event.mouse.unwrap().relative,
        Some(RelativeMotion {
            delta_x: 1.0,
            delta_y: -2.0,
        })
    );
}
```

Under `#[cfg(feature = "recorder")]`, add:

```rust
#[test]
fn legacy_serialized_mouse_event_defaults_to_no_relative_motion() {
    let event = Event::mouse_moved(10.0, 20.0);
    let mut value = serde_json::to_value(event).expect("event should serialize");
    value["mouse"]
        .as_object_mut()
        .expect("mouse should be an object")
        .remove("relative");

    let decoded: Event =
        serde_json::from_value(value).expect("legacy event should deserialize");
    assert_eq!(decoded.mouse.unwrap().relative, None);
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run:

```bash
cargo test --features recorder event::tests
```

Expected: compilation fails because `RelativeMotion`, `MouseData::relative`, and the relative constructors do not exist.

- [ ] **Step 3: Implement the event model**

In `src/event.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "recorder", derive(Serialize, Deserialize))]
pub struct RelativeMotion {
    pub delta_x: f64,
    pub delta_y: f64,
}
```

Add this field to `MouseData`:

```rust
#[cfg_attr(feature = "recorder", serde(default))]
pub relative: Option<RelativeMotion>,
```

Set `relative: None` in every existing `MouseData` literal. Implement:

```rust
pub fn mouse_moved_relative(
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
) -> Self {
    let mut event = Self::mouse_moved(x, y);
    event.mouse.as_mut().expect("mouse event").relative =
        Some(RelativeMotion { delta_x, delta_y });
    event
}

pub fn mouse_dragged_relative(
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
) -> Self {
    let mut event = Self::mouse_dragged(x, y);
    event.mouse.as_mut().expect("mouse event").relative =
        Some(RelativeMotion { delta_x, delta_y });
    event
}
```

Re-export `RelativeMotion` from `src/lib.rs`.

- [ ] **Step 4: Run event and recorder tests**

Run:

```bash
cargo test --features recorder event::tests
```

Expected: all event tests pass, including legacy deserialization.

- [ ] **Step 5: Commit the event model**

```bash
git add src/event.rs src/lib.rs
git commit -m "feat: represent relative pointer motion"
```

---

### Task 3: Add a Cross-Platform Relative Injection Contract

**Files:**
- Create: `src/platform/motion.rs`
- Modify: `src/platform/mod.rs`
- Modify: `src/platform/linux/mod.rs`
- Modify: `src/platform/linux/x11/mod.rs`
- Modify: `src/platform/linux/x11/simulate.rs`
- Modify: `src/platform/linux/evdev/mod.rs`
- Modify: `src/platform/linux/evdev/simulate.rs`
- Modify: `src/platform/macos/mod.rs`
- Modify: `src/platform/macos/simulate.rs`
- Modify: `src/platform/windows/mod.rs`
- Modify: `src/platform/windows/simulate.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `MouseData::relative`, existing `mouse_position()`, `mouse_move()`, XTest injector, evdev `REL_X`/`REL_Y`.
- Produces: Public `mouse_move_relative(delta_x, delta_y) -> Result<()>` and internal `motion_from_event(&Event) -> Option<Motion>`.

- [ ] **Step 1: Write failing dispatch tests**

Create `src/platform/motion.rs` with tests that define the required behavior:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Motion {
    Absolute { x: f64, y: f64 },
    Relative { delta_x: f64, delta_y: f64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Event;

    #[test]
    fn absolute_event_dispatches_absolute_motion() {
        assert_eq!(
            motion_from_event(&Event::mouse_moved(10.0, 20.0)),
            Some(Motion::Absolute { x: 10.0, y: 20.0 })
        );
    }

    #[test]
    fn relative_event_dispatches_relative_motion() {
        assert_eq!(
            motion_from_event(&Event::mouse_moved_relative(
                100.0, 200.0, -4.0, 6.0,
            )),
            Some(Motion::Relative {
                delta_x: -4.0,
                delta_y: 6.0,
            })
        );
    }
}
```

- [ ] **Step 2: Run the tests and verify the helper is incomplete**

Run:

```bash
cargo test platform::motion::tests
```

Expected: compilation fails because `motion_from_event` is not defined or the module is not wired into `platform`.

- [ ] **Step 3: Implement shared motion dispatch**

Implement:

```rust
pub(crate) fn motion_from_event(event: &Event) -> Option<Motion> {
    let mouse = event.mouse.as_ref()?;
    match mouse.relative {
        Some(relative) => Some(Motion::Relative {
            delta_x: relative.delta_x,
            delta_y: relative.delta_y,
        }),
        None => Some(Motion::Absolute {
            x: mouse.x,
            y: mouse.y,
        }),
    }
}
```

Declare `mod motion;` in `src/platform/mod.rs`. Update every platform
`simulate()` motion arm to match `motion_from_event(event)` and call either
`mouse_move()` or `mouse_move_relative()`.

- [ ] **Step 4: Implement relative injection on every backend**

Use native relative injection on X11:

```rust
pub fn mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    let x = finite_rounded_c_int(delta_x);
    let y = finite_rounded_c_int(delta_y);
    with_injector(|display| {
        let result =
            unsafe { xtest::XTestFakeRelativeMotionEvent(display, x, y, 0) };
        unsafe { xlib::XSync(display, FALSE) };
        if result == 0 {
            Err(Error::SimulateFailed(
                "XTestFakeRelativeMotionEvent failed".into(),
            ))
        } else {
            Ok(())
        }
    })
}
```

Extract `finite_rounded_c_int()` so absolute and relative X11 motion share the
same finite/clamping rules.

On evdev, move the existing `REL_X`/`REL_Y` implementation into
`mouse_move_relative()` and retain `mouse_move()` as its current compatibility
alias.

On macOS and Windows, add:

```rust
pub fn mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    let (x, y) = mouse_position()?;
    mouse_move(x + delta_x, y + delta_y)
}
```

Add a `NotSupported` implementation to the Linux no-backend stub. Re-export
the function from every platform `mod.rs` and from `src/lib.rs`.

- [ ] **Step 5: Run dispatch and backend compile tests**

Run:

```bash
cargo test platform::motion::tests
cargo check --all-features --all-targets
cargo check --no-default-features --features evdev --all-targets
```

Expected: all commands pass.

- [ ] **Step 6: Commit relative injection**

```bash
git add src/platform/motion.rs src/platform/mod.rs src/platform/linux/mod.rs src/platform/linux/x11/mod.rs src/platform/linux/x11/simulate.rs src/platform/linux/evdev/mod.rs src/platform/linux/evdev/simulate.rs src/platform/macos/mod.rs src/platform/macos/simulate.rs src/platform/windows/mod.rs src/platform/windows/simulate.rs src/lib.rs
git commit -m "feat: inject relative pointer motion"
```

---

### Task 4: Build the XI2 Raw-Motion Adapter

**Files:**
- Create: `src/platform/linux/x11/xinput.rs`
- Modify: `src/platform/linux/x11/mod.rs`
- Modify: `Cargo.toml`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: x11-rs `xinput2`, `XGenericEventCookie`, and `RelativeMotion`.
- Produces: `RawMotionInput::initialize(display, root)`, `select()`, `deselect()`, and `decode(event) -> Result<Option<RelativeMotion>>`.

- [ ] **Step 1: Write failing sparse-valuator tests**

In `src/platform/linux/x11/xinput.rs`, add a pure helper contract and tests:

```rust
fn decode_axes(mask: &[u8], values: &[f64]) -> Option<RelativeMotion> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_both_relative_axes() {
        assert_eq!(
            decode_axes(&[0b0000_0011], &[3.5, -4.25]),
            Some(RelativeMotion {
                delta_x: 3.5,
                delta_y: -4.25,
            })
        );
    }

    #[test]
    fn decodes_sparse_y_axis() {
        assert_eq!(
            decode_axes(&[0b0000_0010], &[7.0]),
            Some(RelativeMotion {
                delta_x: 0.0,
                delta_y: 7.0,
            })
        );
    }

    #[test]
    fn ignores_events_without_xy_axes() {
        assert_eq!(decode_axes(&[0b0000_0100], &[9.0]), None);
    }
}
```

- [ ] **Step 2: Enable xinput and verify the tests fail behaviorally**

Change the Linux x11 dependency features to:

```toml
x11 = { version = "2.21", features = ["xlib", "xrecord", "xinput", "xtst"], optional = true }
```

Add `mod xinput;` in `src/platform/linux/x11/mod.rs`, then run:

```bash
cargo test xinput::tests
```

Expected: the first two tests fail because `decode_axes()` returns `None`.

- [ ] **Step 3: Implement sparse packed-value decoding**

Implement `decode_axes()` by walking every bit in the mask, advancing the
packed value index only for set bits, and retaining valuators 0 and 1:

```rust
fn decode_axes(mask: &[u8], values: &[f64]) -> Option<RelativeMotion> {
    let mut value_index = 0;
    let mut delta_x = None;
    let mut delta_y = None;

    for axis in 0..mask.len() * 8 {
        if mask[axis / 8] & (1 << (axis % 8)) == 0 {
            continue;
        }
        let value = *values.get(value_index)?;
        value_index += 1;
        match axis {
            0 => delta_x = Some(value),
            1 => delta_y = Some(value),
            _ => {}
        }
    }

    (delta_x.is_some() || delta_y.is_some()).then(|| RelativeMotion {
        delta_x: delta_x.unwrap_or(0.0),
        delta_y: delta_y.unwrap_or(0.0),
    })
}
```

- [ ] **Step 4: Implement XI2 initialization, selection, and cookie lifetime**

Add:

```rust
pub(super) struct RawMotionInput {
    opcode: c_int,
    root: xlib::Window,
    selected: bool,
}
```

`initialize()` must:

- call `XQueryExtension(display, c"XInputExtension", ...)`;
- call `XIQueryVersion(display, &mut 2, &mut 0)` and require `Success`;
- save the extension opcode and root;
- call `select(display)`.

`select()` creates a three-byte XI2 event mask, sets `XI_RawMotion` with
`XISetMask`, calls `XISelectEvents` for `XIAllMasterDevices`, synchronizes, and
sets `selected = true`. `deselect()` submits an all-zero mask and synchronizes.
`Drop` does not call Xlib because it cannot return an error; the owning
`ActiveGrabs` explicitly calls `deselect()` before closing the display.

`decode()` must reject non-`GenericEvent`, wrong extension opcodes, and
non-`XI_RawMotion` cookies. For accepted cookies, call `XGetEventData`, copy the
valuator mask and the packed `raw_values` slice, run `decode_axes()`, and call
`XFreeEventData` on every successful cookie acquisition before returning.

- [ ] **Step 5: Add libXi to CI and run adapter tests**

Add `libxi-dev` to all three Ubuntu dependency-install commands in
`.github/workflows/ci.yml`.

Run:

```bash
cargo test xinput::tests
cargo check --features x11 --all-targets
```

Expected: all tests pass and the X11 build links successfully.

- [ ] **Step 6: Commit the XI2 adapter**

```bash
git add Cargo.toml Cargo.lock .github/workflows/ci.yml src/platform/linux/x11/mod.rs src/platform/linux/x11/xinput.rs
git commit -m "feat(linux): add XI2 raw motion adapter"
```

---

### Task 5: Integrate Raw Motion with Active X11 Grabs

**Files:**
- Modify: `src/platform/linux/x11/listen.rs`
- Modify: `src/platform/linux/x11/xinput.rs`

**Interfaces:**
- Consumes: `RawMotionInput`, `Event::mouse_moved_relative()`, `Event::mouse_dragged_relative()`, and existing `ActiveGrabs`.
- Produces: One relative grab callback per XI2 raw motion and synchronized raw-selection/pass-through transitions.

- [ ] **Step 1: Write failing motion-classification tests**

Extract a helper in `listen.rs`:

```rust
fn relative_motion_event(
    x: f64,
    y: f64,
    relative: RelativeMotion,
    dragging: bool,
) -> Event
```

Add:

```rust
#[test]
fn raw_motion_is_moved_without_held_button() {
    let event = relative_motion_event(
        100.0,
        200.0,
        RelativeMotion {
            delta_x: 4.0,
            delta_y: -2.0,
        },
        false,
    );
    assert_eq!(event.event_type, EventType::MouseMoved);
    assert_eq!(event.mouse.unwrap().relative.unwrap().delta_x, 4.0);
}

#[test]
fn raw_motion_is_dragged_with_held_button() {
    let event = relative_motion_event(
        100.0,
        200.0,
        RelativeMotion {
            delta_x: 4.0,
            delta_y: -2.0,
        },
        true,
    );
    assert_eq!(event.event_type, EventType::MouseDragged);
}
```

- [ ] **Step 2: Run the tests and verify the helper is missing**

Run:

```bash
cargo test relative_motion_event
```

Expected: compilation fails until the helper and imports are implemented.

- [ ] **Step 3: Make `ActiveGrabs` own XI2 selection**

Add `raw_motion: RawMotionInput` to `ActiveGrabs`. In `acquire()` initialize it
before sending `HookEnabled`. Add methods:

```rust
fn suspend_raw_motion(&mut self) -> Result<()>;
fn resume_raw_motion(&mut self) -> Result<()>;
fn pointer_position(&self) -> Result<(f64, f64)>;
```

`pointer_position()` uses `XQueryPointer` against the root window and returns
`HookStartFailed` or `SimulateFailed` with a precise XQueryPointer message when
the query fails.

- [ ] **Step 4: Route XI2 cookies and suppress core duplicates**

In the active-grab loop:

- pass mutable `XEvent` values to `RawMotionInput::decode()`;
- for decoded relative motion, query absolute pointer position;
- construct `MouseMoved` or `MouseDragged` from `state::is_button_held()`;
- call the grab handler once;
- if it returns `Some`, execute the synchronized motion pass-through method;
- ignore core `MotionNotify` while raw selection is active;
- continue handling core keyboard, buttons, and wheel events unchanged.

The relative event origin remains `InputOrigin::Unknown`, matching the current
active-grab safety boundary.

- [ ] **Step 5: Suspend raw selection around pointer pass-through**

For passed motion:

```rust
self.suspend_raw_motion()?;
// ungrab pointer, replay current absolute position, reacquire pointer
self.resume_raw_motion()?;
```

For a passed pointer press, suspend before `begin_pointer_passthrough()`. In
`try_reacquire_pointer()`, resume only after the pointer grab succeeds and the
button mask has been synchronized. If replay, regrab, or resume fails, return
the error and allow `Drop` to release the remaining resources.

The final implementation uses synchronous `XGrabPointer` plus `SyncPointer` to
freeze on each button event. A passed button event uses `ReplayPointer` with
`CurrentTime`, so the target application receives the original event and owns
the complete implicit-grab gesture. The Xvfb regression test includes
press → button-held motion → release, not merely an immediate button pair.

Update `ActiveGrabs::drop()` to deselect raw motion before releasing the
pointer and keyboard grabs. Cleanup remains best-effort in `Drop`; operational
selection errors have already been propagated by the event loop.

- [ ] **Step 6: Run unit and X11 regression tests**

Run:

```bash
cargo test --features x11
cargo clippy --features x11 --all-targets -- -D warnings
```

Expected: tests and clippy pass; existing X11 grab conversion tests remain
green.

- [ ] **Step 7: Commit active-grab integration**

```bash
git add src/platform/linux/x11/listen.rs src/platform/linux/x11/xinput.rs
git commit -m "feat(linux): report relative X11 grab motion"
```

---

### Task 6: Add an X11 Relative-Motion Diagnostic

**Files:**
- Create: `examples/x11_relative_grab_detection.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `Hook::grab_async`, `MouseData::relative`, `mouse_move_relative()`.
- Produces: A finite diagnostic for signs, drag classification, edge continuation, consume behavior, pass-through behavior, and clean release.

- [ ] **Step 1: Add the diagnostic target and verify it does not build**

Add:

```toml
[[example]]
name = "x11_relative_grab_detection"
path = "examples/x11_relative_grab_detection.rs"
required-features = ["x11"]
```

Run:

```bash
cargo check --features x11 --example x11_relative_grab_detection
```

Expected: failure because the example file does not exist.

- [ ] **Step 2: Implement a finite, emergency-safe diagnostic**

The example must:

- print a warning before acquiring input;
- start the grab on a background thread;
- consume pointer motion while printing absolute `x/y` and relative `dx/dy`;
- count motion events with `relative: None` as failures;
- classify held-button motion as `MouseDragged`;
- stop on `Escape` inside the handler and also install a `ctrlc` fallback;
- automatically stop after ten seconds;
- print explicit instructions to move into and continue against a screen edge;
- print totals for relative events, missing-relative events, and drag events;
- call `Hook::stop()` and join before exiting;
- provide `--self-test`, which injects known right/down and left/up relative
  movements after `HookEnabled`, verifies the pointer moved and returned, and
  requires XI2 RawMotion in both directions while keeping physical capture as
  a separate native check.

Use an atomic stop request rather than calling `Hook::stop()` from inside the
grab callback.

- [ ] **Step 3: Compile and run the diagnostic on the native X11 desktop**

Run the automated mode under a disposable X server first:

```bash
xvfb-run -a cargo run --features x11 --example x11_relative_grab_detection -- --self-test
```

Expected:

- relative injection moves the pointer right/down and restores its origin;
- `missing relative events` is zero;
- the process exits successfully without leaving a grab behind.

XI 2.0 negotiation did not deliver XTest-generated XI2 RawMotion on the tested
server. With XI 2.1 negotiation, the self-test observes both directions of
XTest-generated RawMotion. Edge continuation, physical drag classification,
and replay-loop behavior must still be verified with the native runs.

Then run on the native desktop:

```bash
cargo run --features x11 --example x11_relative_grab_detection
```

Expected:

- `relative events` is greater than zero;
- `missing relative events` is zero;
- delta signs match physical direction;
- deltas continue at the screen edge;
- the process exits after Escape or ten seconds;
- keyboard and pointer control return immediately.

- [ ] **Step 4: Exercise pass-through without recursion**

Run the diagnostic's `--pass-through` mode:

```bash
cargo run --features x11 --example x11_relative_grab_detection -- --pass-through
```

Expected: local cursor/application motion occurs once, event count remains
bounded, and there is no feedback loop or doubled movement.

Final native result: drag-to-highlight worked, edge motion retained nonzero raw
deltas, and the passed gesture produced zero Monio `MouseDragged` callbacks
because the receiving application owned the implicit grab until release.

Also exercise finite pass-through under Xvfb:

```bash
xvfb-run -a cargo run --features x11 --example x11_relative_grab_detection -- --self-test --pass-through
```

Expected: the self-test exits successfully and the callback count remains
bounded. This confirms startup/injection/cleanup only; it does not replace the
physical pass-through run.

- [ ] **Step 5: Commit the diagnostic**

```bash
git add Cargo.toml examples/x11_relative_grab_detection.rs
git commit -m "test(linux): diagnose X11 relative grab motion"
```

---

### Task 7: Document Packaging, Behavior, and Verified Limits

**Files:**
- Modify: `README.md`
- Modify: `docs/input-provenance-cross-platform-handoff.md`
- Modify: `AGENTS.md`
- Modify: `docs/superpowers/specs/2026-07-30-x11-relative-grab-motion-design.md`

**Interfaces:**
- Consumes: Native and automated results from Tasks 1–6.
- Produces: User-facing feature semantics, libXi packaging guidance, and a verified handoff record for later Wayland work.

- [ ] **Step 1: Update Linux dependency and API documentation**

Document:

- X11 builds require `libxi-dev` in addition to current development packages.
- Dynamically linked deployed applications require `libXi.so.6`; distro
  packages should depend on `libxi6`, while AppImage/other bundles may carry
  the shared library.
- Users do not normally install a separate "XI2 application"; XI2 is an X
  server extension checked at runtime.
- `listen()` reports absolute-only motion.
- X11 `grab()` reports absolute position plus raw relative deltas.
- `mouse_move_relative()` is the correct remote replay primitive.
- X11 relative capture still does not provide full Wayland desktop control.

- [ ] **Step 2: Record native evidence in the handoff**

Add the exact date, commands, desktop session type, XI2 version result, edge
test result, consume result, pass-through result, drag result, and any observed
limitations to `docs/input-provenance-cross-platform-handoff.md`. Do not label
an unrun check as verified.

- [ ] **Step 3: Run the complete verification matrix**

Run:

```bash
cargo fmt --all -- --check
cargo test --all-features --all-targets
cargo test --no-default-features --features evdev --all-targets
cargo clippy --all-features --all-targets -- -D warnings
cargo clippy --no-default-features --features evdev --all-targets -- -D warnings
cargo doc --all-features --no-deps
```

Expected: every command exits successfully with no warnings.

- [ ] **Step 4: Inspect the release binary's Linux linkage**

Run:

```bash
cargo build --release --features x11 --example x11_relative_grab_detection
ldd target/release/examples/x11_relative_grab_detection
```

Expected: the binary reports dynamic dependencies including `libXi.so.6`,
`libX11.so.6`, and `libXtst.so.6`.

- [ ] **Step 5: Review the final diff and commit documentation**

Run:

```bash
git diff --check
git status --short
```

Then commit only the documentation files:

```bash
git add README.md AGENTS.md docs/input-provenance-cross-platform-handoff.md docs/superpowers
git commit -m "docs: document X11 relative grab motion"
```

- [ ] **Step 6: Confirm final repository state**

Run:

```bash
git status --short
git log --oneline -8
```

Expected: the implementation files are committed, the worktree is clean except
for this plan if it has intentionally not been committed, and the recent log
contains separate evdev, event-model, injection, XI2, grab, diagnostic, and
documentation commits.
