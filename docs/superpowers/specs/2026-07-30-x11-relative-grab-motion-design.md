# X11 Relative Grab Motion Design

Date: 2026-07-30

Status: Approved for planning

## Context

Monio's default Linux backend currently has two different pointer-motion
semantics:

- X11 listening and grabbing report only absolute root-window coordinates.
- evdev capture and injection operate on relative device deltas.

Absolute X11 motion is sufficient for ordinary input monitoring, but it is not
sufficient for a CrossFlow-style keyboard-and-mouse handoff. Once the local
pointer reaches a screen edge, its absolute coordinate stops changing even
though the physical mouse continues to move. A remote-active CrossFlow source
must continue receiving motion after that point.

The X11 backend already has the other required building blocks:

- XRecord for ordinary, non-blocking listening;
- active `XGrabKeyboard` and `XGrabPointer` sessions for suppression;
- XTest for local pass-through and input injection.

XInput2 (XI2) adds the missing primitive: `XI_RawMotion` events contain
relative device motion and are delivered independently of the clipped core
pointer position.

## Scope

This change adds relative motion to the X11 active-grab path used by
CrossFlow. It does not change ordinary X11 `listen()` behavior.

In scope:

- attach XI2 relative deltas to motion events delivered by X11 `grab()`;
- continue reporting deltas when the local pointer is at a screen edge;
- avoid duplicate core and XI2 motion callbacks;
- retain current grab consume/pass-through semantics;
- add a public relative-motion injection function;
- make `simulate()` replay captured relative motion as relative motion;
- preserve recorder compatibility with older serialized events;
- document and verify the native X11 behavior.

Out of scope:

- a dedicated CrossFlow arm/activate/release session API;
- relative motion from ordinary X11 `listen()`;
- changing macOS or Windows capture behavior;
- cursor hiding, confinement, or center warping;
- network transport, topology, edge switching, or remote held-state cleanup;
- Wayland compositor portals.

## Alternatives Considered

### Add optional relative data to existing mouse motion events

Add a `RelativeMotion` value to `MouseData` while retaining the existing
`MouseMoved` and `MouseDragged` event types and absolute `x`/`y` fields.

This is the selected approach. Existing consumers can continue using absolute
coordinates, while CrossFlow can use the relative data without maintaining a
parallel event stream.

### Add new relative event types

New `MouseMovedRelative` and `MouseDraggedRelative` variants would make the
coordinate space explicit, but every consumer would need to handle two sets of
motion variants. It would also separate absolute and relative observations of
the same physical action and make duplicate delivery easier to introduce.

### Add a separate relative-motion callback API

A second callback or a dedicated CrossFlow capture session would avoid changing
`MouseData`, but it would require synchronizing keyboard, button, wheel, and
relative-motion streams. A dedicated capture session remains the preferred
long-term CrossFlow lifecycle, but it is larger than this X11 backend slice.

## Public API

Add a relative-motion value:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelativeMotion {
    pub delta_x: f64,
    pub delta_y: f64,
}
```

Extend `MouseData`:

```rust
pub struct MouseData {
    pub button: Option<Button>,
    pub x: f64,
    pub y: f64,
    pub clicks: u8,
    pub relative: Option<RelativeMotion>,
}
```

The invariants are:

- `x` and `y` always remain absolute screen coordinates.
- `relative: None` means the backend did not provide a relative observation.
- `relative: Some(...)` contains the delta associated with this motion event.
- Button and click events do not carry relative motion.
- Existing `Event::mouse_moved()` and `Event::mouse_dragged()` constructors set
  `relative` to `None`.
- New relative-motion constructors create motion events containing both the
  current absolute position and the relative delta.

`relative` uses `serde(default)` under the `recorder` feature so recordings
created before this field existed deserialize with `None`.

Add the public injection function:

```rust
pub fn mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()>;
```

The function is part of the common platform contract so downstream
cross-platform code can compile uniformly:

- X11 uses `XTestFakeRelativeMotionEvent`.
- evdev emits `REL_X` and `REL_Y`, reusing its existing relative path.
- macOS and Windows retain their current capture behavior and implement the
  injection API by adding the delta to the current position before using their
  existing absolute injection path.
- unsupported Linux builds return `Error::NotSupported`.

For a `MouseMoved` or `MouseDragged` event, `simulate()` uses
`mouse_move_relative()` when `MouseData::relative` is present and preserves the
existing absolute `mouse_move()` behavior otherwise.

## X11 Initialization

The X11 dependency enables the `xinput` module and the Linux build installs and
links libXi.

`ActiveGrabs::acquire()` performs the following steps before announcing
`HookEnabled`:

1. Query the `XInputExtension` opcode.
2. Negotiate XI2 version 2.0 or newer with `XIQueryVersion`.
3. Select `XI_RawMotion` for `XIAllMasterDevices` on the root window.
4. Acquire the existing keyboard and pointer grabs.

If XI2 2.0 or raw-motion selection is unavailable, X11 `grab()` fails with an
actionable `HookStartFailed` error. It does not silently start an
absolute-only grab, because a successfully started grab must satisfy the new
relative-motion contract. Ordinary X11 `listen()` remains available because it
does not initialize XI2.

## Event Flow

The grab event loop continues to receive core keyboard, button, wheel, and
motion events. It additionally handles X11 `GenericEvent` cookies:

1. Call `XGetEventData` for each generic event.
2. Accept only the negotiated XI2 opcode and `XI_RawMotion`.
3. Decode the sparse valuator mask and its packed values.
4. Read horizontal and vertical relative axes and ignore an event with neither
   axis.
5. Query the current root pointer position for the compatible absolute `x` and
   `y` fields.
6. Construct `MouseMoved` or `MouseDragged` according to Monio's current button
   mask, attaching `RelativeMotion`.
7. Call the grab handler exactly once.
8. Always release cookie data with `XFreeEventData`.

XI2 raw device values are used for `delta_x` and `delta_y`. They remain
available while the core pointer is clipped at a screen edge. CrossFlow may
apply product-level sensitivity or acceleration later; Monio does not silently
derive a delta from clipped absolute coordinates.

Core `MotionNotify` events are no longer sent to the handler while the XI2
relative selection is active. They may update cached absolute position, but
they cannot create a second callback for the same physical movement.

Keyboard, button, and wheel conversion remains unchanged.

## Consume and Pass-Through Semantics

When the handler returns `None`, Monio consumes the motion event. The XI2 raw
selection remains active and no XTest motion is generated. This is the main
CrossFlow remote-active path.

When the handler returns `Some(event)`, Monio preserves the existing local
pass-through behavior:

1. Temporarily deselect XI2 raw motion for this client.
2. Release the pointer grab.
3. Replay the current absolute pointer position with XTest.
4. Reacquire the pointer grab.
5. Reselect XI2 raw motion.

Temporarily deselecting raw motion prevents Monio's own pass-through replay
from re-entering the handler or producing double movement.

The same selection lifecycle applies to an accepted pointer press. Raw motion
is deselected while the receiving application's implicit pointer gesture owns
the pointer and is reselected only after Monio reacquires the pointer. This
retains the documented behavior that Monio may not receive intermediate motion
or release events for a passed local pointer gesture.

All selection, ungrab, replay, and regrab transitions are synchronized with the
X server. Failure to restore either the pointer grab or XI2 raw selection stops
the grab loop with an error rather than leaving the process in a partially
active state.

## Error Handling and Cleanup

- XI2 extension/version/selection failures are reported before `HookEnabled`.
- Non-finite relative injection values are normalized consistently with the
  existing motion API before conversion to XTest integer offsets.
- Every successful `XGetEventData` call has a matching `XFreeEventData`.
- `ActiveGrabs::drop()` releases pointer and keyboard grabs, deselects raw
  motion when possible, destroys the grab window, synchronizes, and closes the
  display.
- `HookDisabled` remains the final callback after a started grab loop exits.
- Existing `stop_hook()` behavior continues to stop both XRecord listen loops
  and active-grab loops.

## Testing and Native Acceptance

Unit tests cover:

- sparse XI2 valuator masks and packed value decoding;
- X-only, Y-only, and combined relative motion;
- motion constructor invariants;
- legacy recorder data defaulting `relative` to `None`;
- `simulate()` choosing relative injection only when relative data exists.

Build validation covers:

- default X11 targets;
- evdev-only targets;
- all features and all targets;
- formatting, clippy with warnings denied, tests, and rustdoc.

X11 integration validation covers:

- XI2 initialization under Xvfb;
- relative XTest injection moving and restoring the Xvfb pointer;
- grab acquisition and cleanup in consume and pass-through configurations;
- existing keyboard, pointer-button, and wheel grab behavior.

XTest-generated motion does not traverse the XI2 raw-device path on the tested
X server, so it cannot verify raw callbacks. One callback per physical
movement, positive/negative delta signs, edge continuation, and pass-through
feedback behavior require the native hardware diagnostic below.

Native X11 validation uses a diagnostic example and the current desktop
session:

- move horizontally and vertically and inspect deltas;
- move against every screen edge and confirm deltas continue;
- consume motion and confirm no application receives it;
- pass motion through and confirm there is no double movement;
- press and drag to confirm relative `MouseDragged`;
- stop the diagnostic and confirm all grabs are released immediately.

The verified results and any remaining limitations are recorded in
`docs/input-provenance-cross-platform-handoff.md`.

## Compatibility and Downstream Impact

Adding a public field to `MouseData` is source-breaking for consumers that
construct it with a struct literal. Constructors and event callbacks remain
behaviorally compatible, and serialized recordings remain backward
compatible. The downstream N-API and Tauri bindings will need to expose
`relative` before JavaScript consumers can use it, but those repository changes
are not part of this implementation.

The existing uncommitted evdev provenance/startup fixes in this worktree are
independent and must not be folded into the design-document commit.
