# Windows Relative Grab Motion and CrossFlow Capture Design

Date: 2026-07-30

Status: Implemented; physical Windows edge/pass-through acceptance pending

## Product context

Monio is intended to provide the native input layer for CrossFlow, a
multi-computer keyboard and pointer sharing system similar to Synergy. Each
computer runs a native agent that:

1. observes local input while the computer is locally controlled;
2. takes ownership of keyboard and pointer input when control moves to another
   computer;
3. forwards typed input operations to the selected target;
4. injects received operations into the local operating system; and
5. releases capture safely when control returns, the network fails, or the
   agent exits.

The preferred architecture is one standalone Rust native-agent process per
active user session. Rust owns the cross-platform state machine, transport,
leases, event protocol, and platform adapters. A platform-specific helper
written in another language remains an option when an operating-system
framework or deployment constraint justifies it, but no current Windows,
macOS, or X11 input primitive requires a different implementation language.

High-frequency input should remain inside the native agent rather than crossing
a text or JSON bridge. If a future helper process is necessary, it must use a
bounded binary transport with ordering and backpressure rather than sending
each pointer sample through a general-purpose application IPC layer.

## Problem

The recent X11 work established a common relative-motion contract:

- `MouseData::x` and `MouseData::y` remain absolute screen coordinates;
- `MouseData::relative` optionally carries the delta associated with the same
  motion event;
- `grab()` on X11 reports XI2 raw deltas so motion continues at a screen edge;
- `mouse_move_relative()` and `simulate()` can replay relative motion.

The Windows public injection API already compiles, but Windows capture does not
yet satisfy this contract. `WH_MOUSE_LL` exposes `MSLLHOOKSTRUCT::pt`, which is
an absolute, screen-clipped position. Computing consecutive coordinate
differences would produce zero at an edge and would not represent device
motion.

Windows Raw Input exposes the required `RAWMOUSE::lLastX/lLastY` relative
values. It is asynchronous and does not itself suppress system delivery, so a
correct grab backend must combine Raw Input with the existing low-level hook.

Windows also permits only one Raw Input target window per device class within a
process. This is a compatibility concern when Monio is embedded in an
application that already registers mouse Raw Input. It is not a cross-process
singleton: a standalone CrossFlow agent does not take Raw Input away from
unrelated applications. During `grab()`, Monio will temporarily own this
process's mouse Raw Input registration and restore the previous registration
on cleanup.

## Cross-platform comparison

The Windows registration constraint is not universal:

- **Windows:** `WH_MOUSE_LL` supplies suppression and absolute/provenance
  information; Raw Input supplies unclipped relative device motion. The two
  streams must be coordinated. Mouse Raw Input registration is singleton per
  process and device class.
- **macOS:** a `CGEventTap` can be a passive listener or an active filter, and
  mouse events expose `MouseEventDeltaX/MouseEventDeltaY` in the same event.
  There is no equivalent single Raw Input receiver registration. Monio does
  not yet copy those fields into `MouseData::relative`; native screen-edge
  behavior must be verified before claiming parity.
- **Linux/X11:** XI2 raw-event selection is scoped to an X client. Active
  `XGrabKeyboard` and `XGrabPointer` ownership is globally exclusive, which is
  intentional while CrossFlow is remote-active. The current branch implements
  XI2 raw motion, active grabs, and XTest replay.
- **Linux/evdev:** ordinary reads may coexist, while `EVIOCGRAB` makes the
  grabbing handle the exclusive recipient for a device. This is also
  intentional during remote-active capture, but it requires device
  permissions and production work for atomic multi-device acquisition and
  hotplug.
- **Wayland:** ordinary applications do not have a portable global hook. A
  production adapter should use the InputCapture and RemoteDesktop portals
  with libei, accepting compositor permission and lifecycle control. This is a
  separate backend, not an XWayland fallback.

The portable CrossFlow layer must model lifecycle and capabilities rather than
assuming every platform implements capture with the same native primitive.

## Windows CrossFlow feasibility

The Windows architecture is feasible for an ordinary interactive desktop when
CrossFlow runs one native agent in the logged-in user's session. Rust can call
the required Win32 APIs directly; a C++, C#, or other language helper would not
remove the operating-system boundaries below.

The following boundaries are product requirements rather than defects in the
relative-motion implementation:

- The input agent must run in every interactive user session that CrossFlow
  controls. A Windows service in session 0 cannot own the logged-in user's
  interactive desktop; a service-based installation must launch a separate
  per-user agent and communicate with it over authenticated local IPC.
- Global low-level hooks cover the desktop associated with the calling thread,
  not every desktop or logged-in session on the machine.
- `SendInput` is subject to User Interface Privilege Isolation (UIPI). A
  normal-integrity agent cannot inject into a higher-integrity application.
  Supporting elevated targets requires an explicit signed/deployed UIAccess or
  elevation strategy; silent privilege escalation is not part of Monio.
- Lock, sign-in, UAC secure-desktop, and secure-attention-sequence control are
  outside the normal interactive desktop. CrossFlow must release capture when
  its desktop/session is no longer active and must not promise remote control
  of those protected surfaces.
- Low-level hook callbacks have a strict operating-system timeout. CrossFlow's
  callback must enqueue a bounded typed operation and return immediately; it
  must not perform network I/O or wait for remote acknowledgement on the hook
  thread.

The current repository also has integration gaps that must be tracked before
calling the full Windows CrossFlow product complete:

- this slice addresses the missing raw relative pointer capture and native
  relative replay;
- Windows keyboard capture currently retains virtual-key identity but discards
  `KBDLLHOOKSTRUCT::scanCode` and extended-key flags, so a layout, dead-key,
  IME, autorepeat, and physical-key fidelity matrix is still required;
- Windows `simulate()` does not yet preserve every wheel direction from
  `WheelData`, so horizontal and negative wheel replay require a focused
  follow-up;
- convenience asynchronous hook/channel entry points do not propagate native
  startup errors directly to their caller; a CrossFlow capture session needs
  an explicit ready/error handshake;
- CrossFlow, rather than Monio's event backend, must release remotely held
  keys/buttons on target switch, disconnect, lease expiry, and agent exit;
- multi-monitor DPI transitions, RDP/fast-user-switching, sleep/resume,
  lock/unlock, elevated targets, high-polling-rate mice, and process-crash
  cleanup require native acceptance runs.

None of these findings requires replacing Rust or abandons the
`MouseData::relative` contract. They do mean that successful relative-motion
tests are one platform slice, not evidence that the complete Windows
CrossFlow product has no remaining work.

## Scope

This implementation slice brings Windows `grab()` motion events into parity
with the existing X11 event contract.

In scope:

- report true Windows Raw Input relative mouse deltas from `grab()`;
- retain absolute screen coordinates on the same event;
- retain `MouseMoved` versus `MouseDragged` classification from Monio's held
  button state;
- ensure one physical motion produces one handler callback;
- preserve consume and local pass-through behavior;
- make Windows `mouse_move_relative()` use native relative `SendInput`;
- preserve Windows self-injection provenance;
- acquire and restore the process's prior mouse Raw Input registration;
- prevent a concurrent Windows Monio hook session from mutating the backend's
  process-global callback and registration state;
- add focused unit tests, a Windows native diagnostic, and documentation.

Out of scope:

- changing ordinary Windows `listen()`, which continues to report
  `relative: None`;
- implementing a complete CrossFlow `Armed -> Captured -> Released` public API;
- running `listen()` and `grab()` concurrently;
- changing macOS capture in this slice;
- supporting absolute tablet/touch digitizer motion as CrossFlow relative
  pointer input;
- changing downstream Tauri or N-API bindings;
- CrossFlow networking, leases, topology, edge routing, or held-state policy;
- Wayland portal/libei implementation.

## Alternatives considered

### Combine Raw Input with the low-level hook

Use a message-only window registered for mouse Raw Input during Windows
`grab()`. Continue using `WH_MOUSE_LL` for global suppression, injected-event
provenance, buttons, wheels, and compatible absolute coordinates. Dispatch
physical motion to the handler only from `WM_INPUT`.

This is the selected approach. It provides unclipped device deltas while
retaining the existing public API.

### Derive deltas from low-level-hook coordinates

Subtract consecutive `MSLLHOOKSTRUCT::pt` positions. This keeps the current
synchronous callback path and has little setup cost, but the result becomes
zero at screen edges and is affected by screen clipping. It does not satisfy
the CrossFlow requirement.

### Add a separate capture API or native helper first

A dedicated capture session is the correct long-term product abstraction and
could isolate Raw Input ownership in a helper process. Implementing it before
Windows event parity would expand the task into a cross-platform lifecycle and
IPC redesign. The current slice will keep the public event shape stable and
provide evidence for that later API.

## Windows components

### Raw mouse receiver

A focused internal `RawMouseInput` component will:

1. inspect and retain the process's current mouse Raw Input registration;
2. create a message-only target window owned by the hook thread;
3. register Generic Desktop mouse usage (`usage_page = 0x01`,
   `usage = 0x02`) with `RIDEV_INPUTSINK`;
4. decode `WM_INPUT` with `GetRawInputData`;
5. inspect `GetCurrentInputMessageSource` and ignore injected Raw Input because
   injected movement is handled by the provenance-aware low-level hook;
6. expose physical mouse samples to the grab loop;
7. remove Monio's registration during cleanup; and
8. restore the previous registration exactly when Monio still owns the mouse
   registration.

Initialization must finish before `HookEnabled`. A readiness gate remains
closed while the target window, registration, and hooks are installed.
Physical motion passes through normally while the gate is closed, and queued
pre-ready Raw Input is drained without invoking the handler. A registration or
window failure aborts grab startup and restores any state already changed.

Microsoft documents Raw Input registration as process-global and recommends
that general-purpose libraries avoid silently replacing an application's
registration. Monio will therefore document the temporary ownership and make
the restoration lifecycle explicit. CrossFlow's standalone agent should have
no pre-existing mouse registration in normal operation.

### Low-level mouse hook

Keyboard, button, wheel, and injected mouse events continue through the
existing low-level-hook conversion path.

In ready grab mode, a non-injected `WM_MOUSEMOVE` follows a different path:

1. retain its projected absolute `pt` as the latest compatible screen
   coordinate;
2. do not call the grab handler from the hook callback; and
3. return a nonzero result so the original local movement is suppressed.

The corresponding Raw Input sample later invokes the grab handler. The latest
projected point is compatibility metadata rather than a strict one-to-one
correlation: Windows may coalesce legacy movement independently of Raw Input.
This moves user callback execution for physical pointer motion out of the
time-sensitive low-level hook callback.

Injected mouse movement remains on the low-level-hook path because it may not
have a physical Raw Input sample and carries the provenance fields needed to
classify `ThisMonioSession`.

### Relative event construction

For a relative `RAWMOUSE` sample:

1. ignore samples with both axes equal to zero;
2. use `lLastX/lLastY` as `delta_x/delta_y`;
3. use the latest low-level-hook `pt`, or current cursor position when no
   projected position is available, for compatible absolute `x/y`;
4. choose `MouseDragged` when Monio's button mask contains a held button,
   otherwise choose `MouseMoved`;
5. leave `origin` as `Unknown`, because Raw Input does not carry Monio's
   `dwExtraInfo` provenance evidence; and
6. call the grab handler exactly once.

For an absolute `RAWMOUSE` sample, the receiver converts its normalized
position using the primary or virtual-desktop bounds selected by the raw flags
and constructs an absolute-only `MouseMoved` or `MouseDragged` event. Its
`relative` field remains `None`. Returning `Some` replays that absolute
position; returning `None` keeps the already-suppressed physical movement
consumed.

Raw absolute devices are not converted into CrossFlow relative pointer motion
in this slice. Pointer-sharing acceptance targets conventional relative mice
and touchpads, while absolute tablet/touch input remains observable without
being mislabeled as a relative delta.

## Consume and pass-through semantics

Returning `None` for a physical relative event generates no replacement
movement. The original low-level mouse event has already been suppressed, so
the local pointer remains under CrossFlow's control.

Returning `Some(event)` replays the original captured delta through native
relative `SendInput`. Windows applies the target machine's pointer-speed
settings to relative `SendInput`, matching normal local pointer processing
after the original physical movement was suppressed.

Grab pass-through replay uses a second private process-session tag distinct
from ordinary Monio simulation:

- the low-level hook recognizes the private replay tag and immediately passes
  that motion without invoking the grab handler;
- provenance classification still reports the replay as
  `ThisMonioSession`;
- ordinary calls to `mouse_move_relative()` retain the normal Monio session
  tag and therefore preserve existing hook observability.

This prevents replay recursion without treating every Monio injection as
internal grab pass-through.

The handler's returned event remains a pass/consume decision, matching current
Windows behavior; this slice does not add event-rewrite semantics.

## Relative injection

Windows `mouse_move_relative(delta_x, delta_y)` will send `MOUSEEVENTF_MOVE`
without `MOUSEEVENTF_ABSOLUTE`, using signed relative `dx/dy`. It will no
longer query the current cursor and convert the result to an absolute move.

Finite values are rounded and clamped to the Win32 signed integer range.
Non-finite values normalize to zero. A zero/zero movement is a successful
no-op.

Absolute `mouse_move(x, y)` remains unchanged by this slice except for tests
needed to prove that absolute and relative flag construction are distinct.

## Lifecycle and error handling

The Windows backend is process-singleton because its handlers, hook handles,
thread ID, and callback mode are process-global. A backend-wide active-session
guard will reject a second `listen()` or `grab()` with `Error::AlreadyRunning`
rather than corrupt global state.

Grab startup order:

1. claim the backend session guard;
2. initialize provenance and replay identity;
3. store the handler, running flag, and hook-thread ID;
4. close the grab-readiness gate;
5. create and register the Raw Input receiver;
6. install keyboard and mouse low-level hooks;
7. drain pre-ready Raw Input;
8. open the readiness gate;
9. publish `HookEnabled`; and
10. enter the message loop.

Cleanup runs for normal stop and every error after partial initialization:

1. stop accepting new Raw Input samples;
2. unhook mouse and keyboard hooks;
3. remove Monio's Raw Input registration and restore the previous one;
4. destroy the message-only window;
5. emit `HookDisabled` only if `HookEnabled` was emitted;
6. clear process-global handlers and thread state; and
7. release the backend session guard.

If the process crashes, Windows removes its hooks and window registration.
Normal errors must still perform explicit cleanup so an embedding process can
start another session.

## Testing

Focused unit tests will cover:

- construction of moved and dragged events from relative raw samples;
- zero-delta suppression;
- relative versus normalized absolute Raw Mouse classification;
- ordinary injected motion retaining its existing absolute/provenance path;
- private pass-through replay recognition and provenance;
- native relative `MOUSEINPUT` flags and signed delta normalization;
- process mouse-registration selection and restoration logic through isolated
  pure helpers;
- safe rejection and release of concurrent backend sessions.

A Windows-only diagnostic example will:

- start `Hook::grab_async`;
- report absolute and relative values for every physical motion;
- support consume and pass-through modes;
- count motion events missing relative data;
- count relative drag events;
- record positive and negative X/Y signs;
- allow Escape, Ctrl+C, and a timeout to release the grab; and
- print an explicit manual checklist for continued movement at every screen
  edge and one-to-one local pass-through.

Synthetic `SendInput` verifies provenance, replay bypass, and injection, but it
does not substitute for a physical Raw Input acceptance run. The implementation
will not claim screen-edge completion until a native physical-mouse run records
that deltas continue while the pointer is clipped.

Repository verification includes:

```text
cargo fmt --all -- --check
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo check --examples
cargo doc --all-features --no-deps
```

The existing `synthetic_input_detection` example must continue to recognize
ordinary Monio key and pointer injection as `ThisMonioSession`.

Implementation checks completed on Windows:

```text
cargo test platform::windows
cargo test --example windows_relative_grab_detection
cargo check --example windows_relative_grab_detection
cargo clippy --all-features --all-targets -- -D warnings
cargo check --examples
cargo doc --all-features --no-deps
cargo run --example synthetic_input_detection
```

The provenance diagnostic passed in the interactive user context. Native
consume and pass-through diagnostics both reached `HookEnabled` and
`Grab released: true`, proving startup and normal cleanup on this host. They
observed no physical movement during their ten-second windows and
intentionally failed with `no physical relative motion was observed`.
Physical direction, drag, screen-edge continuity, consume suppression, and
one-to-one pass-through therefore remain pending.

The native run also confirmed that the CrossFlow host executable needs a
`PerMonitorV2` DPI-aware manifest. In the DPI-unaware diagnostic process,
cursor/display coordinates were virtualized while `MSLLHOOKSTRUCT::pt`
remained per-monitor-aware, producing an observed roughly 1.5x coordinate
difference. Monio remains an embeddable library and does not mutate this
process-global application policy.

## Long-term CrossFlow lifecycle

The generic `grab(callback) -> Option<Event>` API is useful for backend
validation but should not become the complete CrossFlow ownership model.
CrossFlow should eventually expose a portable lifecycle such as:

```text
Local
  -> Armed(edges)
  -> Captured(edge, activation_id)
  -> Remote(target, lease)
  -> Released(cursor_position)
  -> Local
```

Windows and macOS may keep one native filter installed and switch between
pass-through and suppression internally. X11 should acquire active grabs only
when capture activates. Wayland should map the lifecycle to portal barriers,
activation, and release. evdev should acquire every required device atomically
or fail closed.

The portable layer owns routing, leases, target switching, emergency release,
and held-key/button cleanup. Platform adapters report input and enforce local
capture; they do not contain CrossFlow network policy.
