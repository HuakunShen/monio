# Cross-platform input provenance handoff

Status: macOS and Windows self-injection detection are implemented and natively
verified. Linux evdev/uinput exact-device classification has passed native X11
and Wayland self-loopback tests; exclusive grab consumption and selective
uinput pass-through are verified on GNOME 46 Wayland, while the unrelated-device,
restart, hotplug, and broader compositor matrix is still pending. Linux X11
request correlation is implemented and natively
verified on GNOME X11. X11 active keyboard/pointer grab support is implemented
and natively verified on GNOME X11 and an isolated Xvfb server. XI2 relative
grab motion and relative XTest injection are implemented; automated
initialization/injection/cleanup checks and native physical RawMotion/drag
capture, screen-edge continuation, and gesture pass-through all pass. Wayland
InputCapture receive and RemoteDesktop/EIS target injection now have native
GNOME proofs of concept. Target injection can activate an armed source barrier,
and injection into a physically activated capture session is echoed back to the
receiver. CrossFlow must therefore disable source capture while a machine holds
an inbound target lease. A production portal backend is not yet implemented.

Last updated: 2026-07-30

Audience: a human or AI continuing Monio input-provenance work on a native
Windows or Linux machine.

## Executive summary

Monio can observe input that it injected itself. Without provenance, a remote
input application such as CrossFlow can accidentally capture a received event
and send it back across the network, creating a feedback loop.

The required V1 guarantee is deliberately narrow:

> Identify input injected by this Monio platform-host session so the caller can
> drop it.

It is not necessary for feedback-loop prevention to prove that every remaining
event came from physical hardware. `InputOrigin::Unknown` means only that the
active backend cannot make a stronger claim.

Current macOS behavior demonstrates the intended contract:

```rust
if event.is_from_this_monio_session() {
    // Never retransmit it and never use it to claim local input ownership.
    return;
}
```

The equivalent implementation mechanism is platform-specific:

| Platform/backend | Intended self-injection evidence | Status |
| --- | --- | --- |
| macOS Core Graphics | Random process-session `EventSourceUserData` plus current source PID | Implemented and natively verified |
| Windows low-level hooks | Random process-session `dwExtraInfo`, plus injected hook flags | Implemented and natively verified |
| Linux evdev/uinput | Exact live character-device number owned by Monio's uinput handle | Implemented; native X11/Wayland self-loopback and GNOME Wayland grab/pass-through verified, broader matrix pending |
| Linux X11 XRecord/XTest | Persistent XTest client ID plus ordered request/device-event correlation | Implemented and natively verified on GNOME X11 |
| Wayland portal/libei | No stable per-event self-injection identity was observed on GNOME; use mutually exclusive source/target leases | Native InputCapture receiver and RemoteDesktop/EIS sender proofs pass; feedback/barrier hazards verified; production backend pending |

Provenance and suppression are separate responsibilities. X11 `listen()` uses
XRecord request correlation for self-injection classification. X11 `grab()`
now uses active keyboard/pointer grabs for suppression. A CrossFlow source
capture session needs reliable suppression while control is remote, but it
does not need to prove that every other event is physical.

## Why this work exists

The original diagnostic performed these actions:

1. enable Monio's listener;
2. synthesize a `ControlLeft` press and release;
3. move the pointer to a target and restore it;
4. report whether matching listener events were observed.

On macOS, the listener observed every synthesized action. Timing, key values,
and coordinates proved only that recapture happened. They could not prove who
created the events.

This matters to CrossFlow because a target machine receives an operation,
injects it into its OS, and may have its local capture hook active at the same
time:

```text
source A
  -> network operation
  -> target B injects it
  -> B captures the injected event again
  -> without provenance, B may retransmit it
```

The fix belongs in Monio/native platform code. CrossFlow's portable domain
logic should not contain Core Graphics, Win32, X11, evdev, uinput, portal, or
libei details.

## Terms and safety boundary

### `ThisMonioSession`

Positive evidence that the active native backend recognizes the event as one
injected by its own Monio platform-host session.

This is a feedback-loop marker, not a credential or authorization proof.

### `Unknown`

The backend does not have evidence strong enough for another classification.

`Unknown` is not equivalent to:

- physical;
- human-generated;
- safe;
- non-injected;
- trusted.

Another application, accessibility tool, virtual HID device, kernel driver, or
compositor path may create an event that remains `Unknown`.

### Device-backed is not cryptographic proof

Even when a backend identifies a physical-looking input device, privileged
software or a virtual driver may imitate device metadata. Device identity can
support product policy and local-takeover UX, but must not become a remote
authorization primitive.

## Current public Monio contract

Implemented in `src/event.rs`:

```rust
#[non_exhaustive]
pub enum InputOrigin {
    Unknown,
    Injected {
        injector: InjectorIdentity,
    },
}

#[non_exhaustive]
pub enum InjectorIdentity {
    ThisMonioSession,
}

impl Event {
    pub fn is_from_this_monio_session(&self) -> bool;
}
```

All ordinary constructors default to `Unknown`. Recorder JSON created before
the `origin` field existed still deserializes with `Unknown`.

Do not add `Physical` merely because one platform has a weak heuristic.
Add new variants only when their evidence and semantics can be stated and
tested precisely.

## macOS: implemented reference behavior

Relevant files:

- `src/platform/macos/provenance.rs`
- `src/platform/macos/listen.rs`
- `src/platform/macos/simulate.rs`
- `examples/synthetic_input_detection.rs`

### Mechanism

The process initializes one random, non-zero, positive 63-bit session tag. All
events injected by Monio in that process reuse the tag; it is not a unique ID
per event.

Before `CGEvent::post`, Monio writes the tag to:

```text
CGEventField::EventSourceUserData
```

The event-tap listener reports `ThisMonioSession` only when:

```text
observed user-data tag == current Monio session tag
and
EventSourceUnixProcessID == current process PID
```

Every other event remains `Unknown`.

### Verified native command

From the standalone Monio repository:

```bash
cargo run --example synthetic_input_detection
```

The command requires macOS Accessibility permission. It exits unsuccessfully
unless the tagged key press, key release, pointer target, and pointer
restoration are all recaptured as `ThisMonioSession`.

Verified implementation commit:

```text
baa2bc7d2ea98678367391da549a2cef714c1e05
```

### Design implication

Capture and injection should stay in the same Native Platform Host process.
If one process captures while an unrelated helper process injects, the current
tag-plus-PID rule intentionally does not classify that helper's events as
`ThisMonioSession`.

If CrossFlow later requires multiple injector processes, design an explicit,
bounded process-session registration mechanism. Do not silently weaken the PID
check.

## Windows: implemented reference behavior

Relevant files:

- `src/platform/windows/provenance.rs`;
- `src/platform/windows/listen.rs`;
- `src/platform/windows/raw_input.rs`;
- `src/platform/windows/simulate.rs`;
- `examples/synthetic_input_detection.rs`;
- `examples/windows_relative_grab_detection.rs`.

Windows initializes two distinct random, nonzero 32-bit process-session tags:
one for public Monio injection and one private grab-replay tag. Every keyboard
and mouse `SendInput` record stores the appropriate tag in `dwExtraInfo`.
Low-level keyboard and mouse hooks preserve `dwExtraInfo` plus the injected
flags and report `ThisMonioSession` only when an exact active tag matches and
`LLKHF_INJECTED` or `LLMHF_INJECTED` is present. Zero, mismatched, untagged,
and other-injector events remain `Unknown`. Only the private replay tag
bypasses grab-handler dispatch; ordinary Monio simulation remains observable
to the handler.

Primary references:

- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-kbdllhookstruct>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-msllhookstruct>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-keybdinput>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-mouseinput>
- <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput>

Native Windows verification observed the tagged key press, key release, mouse
target, and mouse restoration as `ThisMonioSession`. Implementation commit:

```text
4f7ce57d7d4bae60fc711bf16cb1c01355a66844
```

### Windows relative grab implementation

Ordinary Windows `listen()` still reports absolute pointer motion with
`MouseData::relative == None`. During `grab()`, Monio combines two Windows
input paths:

1. `WH_MOUSE_LL` suppresses physical legacy movement, records its projected
   absolute point, handles buttons and wheels, and retains injected-event
   provenance;
2. a message-only window receives `WM_INPUT` and decodes relative
   `RAWMOUSE::lLastX/lLastY`;
3. one physical relative sample constructs exactly one `MouseMoved` or
   `MouseDragged` handler event containing both absolute `x/y` and raw
   `delta_x/delta_y`;
4. `None` consumes that motion; `Some` replays the original raw delta through
   privately tagged relative `SendInput`;
5. the low-level hook recognizes that private replay and passes it without
   recursively invoking the grab handler; and
6. `GetCurrentInputMessageSource` filters injected Raw Input because injected
   movement remains on the provenance-aware low-level-hook path.

`mouse_move_relative()` now sends signed relative `MOUSEEVENTF_MOVE` directly
instead of reading the cursor and converting the result into an absolute
move. Finite deltas are rounded and clamped to the Win32 signed integer range;
non-finite values become zero.

Raw Input registration is singleton per device class within a process, not
across the desktop. Windows grab snapshots this process's Generic Desktop
mouse registration, temporarily points it at Monio's message-only window, and
restores it only if Monio still owns the registration during cleanup. A second
Monio Windows hook/grab session is rejected with `AlreadyRunning`, preventing
the process-global handlers and registration from being overwritten.

The focused implementation checks passed on Windows:

```powershell
cargo fmt --all -- --check
cargo test --all-features
cargo test platform::windows
cargo test --example windows_relative_grab_detection
cargo check --example windows_relative_grab_detection
cargo clippy --all-features --all-targets -- -D warnings
cargo check --examples
cargo doc --all-features --no-deps
cargo run --example synthetic_input_detection
```

The provenance diagnostic had to run as the active interactive user. The
restricted `codexsandboxoffline` account reproduced `GetCursorPos:
Access is denied (0x80070005)` even in the standalone `mouse_position`
example, before any hook was installed. Running the same executable outside
that sandbox read the cursor successfully. In the interactive context,
`synthetic_input_detection` classified ControlLeft press/release and both
pointer moves as `ThisMonioSession`.

Native consume and pass-through invocations:

```powershell
cargo run --example windows_relative_grab_detection
cargo run --example windows_relative_grab_detection -- --pass-through
```

both successfully registered the hook and Raw Input receiver in the
interactive session, emitted `HookEnabled`, timed out normally, restored local
control, and reported `Grab released: true`. No physical mouse motion occurred
during either ten-second window, so both diagnostics correctly exited
unsuccessfully with `no physical relative motion was observed`.

The following physical observations therefore remain pending and must not be
inferred from `SendInput`:

- nonzero positive and negative X/Y raw deltas from a conventional mouse;
- `MouseDragged` while a physical button is held;
- continued deltas while repeatedly pushing against every screen edge;
- consume mode keeping the local pointer stationary;
- pass-through feeling one-to-one without doubling, feedback, or jumps.

### Windows feasibility for CrossFlow

A standalone Rust agent is feasible for the ordinary Windows interactive
desktop. The `windows` crate exposes all APIs used here, so this capture path
does not require a C#, C++, or service helper. CrossFlow should run one native
agent in each logged-in interactive user session and keep low-level hook
callbacks limited to enqueueing/local decisions; network I/O must happen
outside those timeout-sensitive callbacks.

Hard Windows boundaries remain:

- an interactive service in session 0 cannot capture the logged-in user's
  desktop; a service may supervise, but the input agent must run in the user
  session;
- low-level hooks apply to the installing thread's desktop/session, not every
  concurrent login or secure desktop;
- `SendInput` is subject to User Interface Privilege Isolation (UIPI), so a
  normal-integrity agent cannot control a higher-integrity application;
- UAC secure desktop, lock screen, sign-in desktop, and secure-attention
  sequence are outside the normal capture/injection path.

The CrossFlow executable must also declare `PerMonitorV2` DPI awareness in its
application manifest. `MSLLHOOKSTRUCT::pt` uses per-monitor-aware screen
coordinates, while cursor/display APIs can be virtualized for a DPI-unaware
process. This host demonstrated the mismatch: a requested logical target near
`(1420, 768)` appeared in the low-level hook near `(2131, 1153)`, a roughly
1.5x difference. DPI awareness is process-global application policy, and
Microsoft recommends setting it in the executable manifest, so Monio should
not silently change it from an embeddable library.

Primary DPI references:

- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-msllhookstruct>
- <https://learn.microsoft.com/windows/win32/sbscs/application-manifests>
- <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setprocessdpiawarenesscontext>

Separate repository/product work is still required before claiming a complete
Windows CrossFlow implementation:

- preserve keyboard scan code and extended-key identity and validate layout,
  dead-key, IME, AltGr, autorepeat, and layout-mismatch behavior;
- preserve vertical versus horizontal wheel direction during generic
  `simulate()` replay;
- add an asynchronous startup handshake so `Hook::run_async` and
  `Hook::grab_async` can return backend startup errors directly;
- have the CrossFlow state machine release remotely held keys/buttons on
  disconnect, timeout, target switch, lock, and process exit.

After adding that manifest, the native product acceptance matrix must cover
mixed-DPI multi-monitor layouts, negative virtual-screen coordinates, 1000 Hz
or faster mice, elevated target applications, RDP, fast user switching,
lock/unlock, sleep/resume, network disconnect, and process crash. These items
do not block the Rust architecture, but they prevent describing the entire
Windows product as finished.

## Linux X11 handoff

### Implemented mechanism

Relevant files:

- `src/platform/linux/x11/provenance.rs`;
- `src/platform/linux/x11/listen.rs`;
- `src/platform/linux/x11/simulate.rs`;
- `src/platform/linux/x11/xinput.rs`.

XTest does not provide a Monio-controlled field on the resulting device event,
so the X11 backend does not pretend that the event carries a tag. Instead,
Monio owns one persistent XTest display connection for the process session.
The connection's X11 resource-ID base is its injector identity.

The listener asks XRecord for both:

- core device events from all clients;
- `XTestFakeInput` requests.

XRecord identifies requests by the originating client's resource-ID base.
Only requests from Monio's persistent injector enter the correlation queue.
The XRecord specification says requests are recorded immediately before
execution and that XTest-generated device events are recorded in request
order. The correlator validates the expected event type and key/button detail
against the next device event. A match becomes `ThisMonioSession`; a mismatch
clears pending state and remains `Unknown`.

This is request provenance plus server ordering, not timing, coordinate, or
suppression-window guessing. Requests from unrelated XTest clients have a
different resource-ID base and do not enter the queue.

Primary references:

- <https://www.x.org/releases/X11R7.6/doc/recordproto/record.html>
- <https://www.x.org/releases/current/doc/xextproto/xtest.pdf>

### Native verification recorded on 2026-07-30

The host ran GNOME in a native X11 session. A protocol probe first observed
each request from the known injector ID immediately followed by its matching
device event with the same server time. The retained Monio diagnostic then
passed:

```bash
cargo run --example synthetic_input_detection
```

Observed classifications:

```text
Tagged ControlLeft press:          YES
Tagged ControlLeft release:        YES
Tagged mouse target:               YES
Tagged mouse restoration:          YES
```

The build checks also passed:

```bash
cargo check --features x11
cargo check --all-features
cargo clippy --features x11 --lib -- -D warnings
cargo fmt --all -- --check
```

The two pre-existing `unnecessary_map_or` warnings in `examples/grab.rs` were
mechanically fixed during the X11 grab work so all-target clippy can run with
warnings denied.

### Remaining X11 acceptance work

Before claiming a broad X-server compatibility matrix:

1. run an unrelated XTest client concurrently and confirm its identical
   events remain `Unknown`;
2. stress high-rate pointer, button, and wheel injection;
3. restart the listener while the persistent injector connection remains
   alive;
4. test generated key autorepeat, which has no one-to-one XTest request and
   should remain `Unknown` rather than be guessed as self;
5. repeat on another Xorg version and on Xwayland, without describing
   Xwayland as a complete Wayland backend.

### X11 capture and suppression are implementable

The old limitation was specific to Monio's XRecord listener, not to X11.
Before 2026-07-30, `run_grab_hook` adapted the grab handler to a normal
listener and ignored its return value. That fallback has now been replaced by
an active-grab event loop; `listen()` remains on XRecord.

X11 provides active grabs:

- `XGrabKeyboard` routes subsequent keyboard events to the grabbing client;
- `XGrabPointer` routes selected pointer events to the grabbing client and may
  confine the pointer or replace its cursor;
- ungrabbing or destroying/unmapping the grab window returns control to the
  normal desktop.

Primary references:

- <https://xorg.freedesktop.org/archive/current/doc/man/man3/XGrabKeyboard.3.xhtml>
- <https://xorg.freedesktop.org/archive/current/doc/man/man3/XGrabPointer.3.xhtml>

Deskflow, the open-source upstream used by Synergy, is a native precedent. Its
X11 source/server path creates a full-screen override-redirect `InputOnly`
window with an invisible cursor. When control leaves the local screen it maps
the window, calls `XGrabKeyboard` and `XGrabPointer`, retries both grabs with
rollback, warps the pointer to the screen center, and reads the resulting
motion/key/button events. Returning to the local screen unmaps the window,
which releases the grabs. Its X11 target/client path injects with XTest.

Primary source:

- <https://github.com/deskflow/deskflow/blob/master/src/lib/platform/XWindowsScreen.cpp>

The recommended Monio split is:

```text
ordinary listen/provenance:
  XRecord + XTest request correlation

target injection:
  persistent XTest connection

CrossFlow source capture while remote-active:
  active X11 keyboard/pointer grab on a dedicated InputOnly window
```

### Implemented X11 grab mechanism

Relevant files:

- `src/platform/linux/x11/listen.rs`;
- `src/platform/linux/x11/simulate.rs`;
- `examples/x11_grab_detection.rs`.

`run_grab_hook` now:

1. opens a dedicated X connection and maps a 1x1 off-screen,
   override-redirect `InputOnly` window;
2. requires XI2 2.1+ and selects `XI_RawMotion` for the master pointer;
3. acquires `XGrabKeyboard` first and a synchronous `XGrabPointer` second,
   then uses `SyncPointer` to run until the next button event;
4. rolls the keyboard grab back if pointer acquisition fails;
5. reports `AlreadyGrabbed`, `GrabInvalidTime`, `GrabNotViewable`, and
   `GrabFrozen` with device-specific startup errors;
6. dispatches keyboard, button, and wheel events from the core stream, while
   XI2 raw events are the single handler source for pointer motion;
7. consumes events whose handler returns `None`;
8. temporarily deselects raw motion, releases, XTest-replays, reacquires, and
   reselects around passed standalone motion;
9. uses `ReplayPointer` with `CurrentTime` for passed pointer-button events,
   yielding the original event to the receiving X11 client and its implicit
   grab until release, then reacquires the pointer and raw selection;
10. deselects raw motion, ungrabs both devices, destroys the window, and closes
    the connection on normal stop or error. X11 also releases the grabs
    automatically if the process connection dies.

A passed pointer press is therefore a gesture-level decision on X11. The
handler can see the press that starts the pass-through, but may not see its
intermediate motion or release before Monio reacquires the pointer. CrossFlow's
remote-active path should return `None` for every source event, so it does not
enter this compatibility path.

Active grabs are exclusive. They require neither root nor the `input` group,
but they fail if another X11 client already owns the keyboard or pointer. No
reboot is needed; the conflicting client must release its grab or disconnect.

### X11 grab verification on 2026-07-30

The deterministic isolated acceptance command is:

```bash
xvfb-run -a cargo run --features x11 --example x11_grab_detection
```

The same diagnostic also passed directly in the unlocked GNOME X11 session:

```bash
cargo run --features x11 --example x11_grab_detection
```

Both runs passed with these independently observed behaviors:

- consumed Q press/release: handler 2, observer 0;
- passed W press/release: handler 2, observer 2;
- consumed left-button press/release: handler 2, observer 0;
- passed right-button gesture: handler observed its start, observer received
  press, button-held motion, and release;
- consumed standalone pointer motion: handler 1, observer 0;
- an existing keyboard grab produced the expected conflict error;
- an existing pointer grab caused pointer acquisition to fail and the
  diagnostic then acquired the keyboard, proving keyboard rollback.
- after `Hook::stop()`, the diagnostic independently acquired both devices,
  proving normal-stop cleanup released the grabs.

Fresh repository verification after the implementation passed:

```bash
cargo fmt --all -- --check
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo check --examples
cargo doc --all-features --no-deps
```

The post-change native `cargo run --example synthetic_input_detection` also
classified the ControlLeft press/release and both pointer moves as
`Injected { injector: ThisMonioSession }`, confirming the unchanged XRecord
listen/provenance path still works on the GNOME X11 display.

The earlier `AlreadyGrabbed` result on `DISPLAY=:0` was caused by the GNOME
screen shield while the desktop was locked: GNOME ScreenSaver reported active,
the logind session reported `LockedHint=yes`, and GNOME Shell owned the
compositor input path. Unlocking changed those states to inactive/unlocked and
the native diagnostic acquired both devices without a reboot.

The diagnostic observer is deliberately placed at root coordinates `(256,
256)` and verifies that both keyboard focus and the pointer route to its own
window before generating events. Its original `(32, 32)` location was under
GNOME's top-bar/compositor overlay, so a passed right-button gesture went to
GNOME Shell rather than the observer and produced a false failure.

The first diagnostic used an immediate passed button pair, which did not catch
the later native drag failure. It now waits between the passed press, motion,
and release and requires the observer's `MotionNotify` to carry the expected
held-button mask.

### X11 relative motion for CrossFlow

Relevant files:

- `src/event.rs`;
- `src/platform/motion.rs`;
- `src/platform/linux/x11/xinput.rs`;
- `src/platform/linux/x11/listen.rs`;
- `src/platform/linux/x11/simulate.rs`;
- `examples/x11_relative_grab_detection.rs`.

`MouseData` now has `relative: Option<RelativeMotion>`. Absolute `x`/`y`
coordinates retain their old meaning:

- ordinary XRecord `listen()` motion has `relative: None`;
- active X11 `grab()` motion has both current root coordinates and XI2 raw
  `delta_x`/`delta_y`;
- `MouseMoved` versus `MouseDragged` still comes from Monio's held-button
  state;
- `simulate()` prefers relative replay when an event carries relative data;
- `mouse_move_relative(delta_x, delta_y)` is available as a direct public
  injection API.

The grab connection selects `XI_RawMotion` for `XIAllMasterDevices` on the
root window. It decodes XI2's sparse valuator mask and packed `raw_values`;
valuator 0 is horizontal motion and valuator 1 is vertical motion. Core
`MotionNotify` is ignored while raw selection is active so one physical
movement cannot create both an absolute and a relative handler callback.
Using raw rather than clipped root-coordinate differences is what should keep
CrossFlow movement available at a local screen edge.

If XI2 2.1 negotiation or event selection fails, `grab()` fails before
`HookEnabled`; it does not silently fall back to absolute-only motion. Relative
grab events remain `InputOrigin::Unknown`, consistent with the active-grab
safety boundary.

The client must request XI 2.1, not merely test whether the server supports a
2.x release. XI 2.1 introduced RawEvents delivery regardless of grab state.
The first native diagnostic negotiated only XI 2.0 and consequently received
zero raw events even though the X.Org server supported XI 2.4. Requiring and
requesting XI 2.1 fixed the native capture path.

The public X11 relative injector uses `XTestFakeRelativeMotionEvent`. The
system XTest header exposes the correct four-argument ABI
`(display, delta_x, delta_y, delay)`, but x11-rs 2.21 declares an incorrect
five-argument signature. Monio therefore uses a narrow local FFI declaration
with the header's four-argument ABI rather than calling the incorrect binding.

Enabling x11-rs `xinput` adds a dynamic libXi dependency. Ubuntu/Debian build
hosts need `libxi-dev`; deployed dynamically linked applications need
`libXi.so.6` (normally supplied by `libxi6`). XI2 is an X-server extension,
not a separate user application. Static X11 desktop linking is not the default
or recommended packaging path.

Automated checks completed on 2026-07-30:

```bash
cargo test --features x11
cargo clippy --features x11 --all-targets -- -D warnings
xvfb-run -a cargo run --features x11 \
  --example x11_relative_grab_detection -- --self-test
xvfb-run -a cargo run --features x11 \
  --example x11_relative_grab_detection -- --self-test --pass-through
```

Both Xvfb modes negotiated XI2, acquired and released the grabs, moved the
pointer right/down with relative injection, observed the corresponding
positive XI2 RawMotion, restored its origin with inverse injection, and
observed negative RawMotion. The self-test now requires both directions rather
than merely checking the final pointer positions. The unit suite covers sparse
XI2 X/Y valuators, movement versus drag construction, serialization
compatibility, relative replay dispatch, and XTest integer normalization. A
release build inspected with `ldd` resolved `libX11.so.6`, `libXi.so.6`, and
`libXtst.so.6` dynamically.

The earlier XTest probes negotiated only XI 2.0 and received zero RawMotion.
After requesting XI 2.1, the same relative XTest injection produced RawMotion,
which is consistent with XI 2.1 adding delivery regardless of grab state. This
automated path verifies the event pipeline but is not a substitute for physical
hardware capture. A delayed Monio uinput probe was also not attached as an Xorg
slave pointer (`xinput list` never showed it), so it could not provide a
separate hardware-shaped source.

Native hardware acceptance on the GNOME X11 session recorded:

```bash
cargo run --features x11 --example x11_relative_grab_detection
cargo run --features x11 --example x11_relative_grab_detection -- --pass-through
```

The consume run received 992 relative motion events, including 176
`MouseDragged` events. It observed both signs on both axes, reported zero
motion events without relative data, and continued reporting nonzero deltas
while the pointer was held against a screen edge.

The original pass-through implementation used asynchronous `XGrabPointer`,
released it, and synthesized another press with XTest. A delayed drag
regression test proved that the target received neither the complete button
pair nor button-held motion: Monio reacquired the pointer while the physical
button was still down. X11 cannot `ReplayPointer` an event frozen directly by
`XGrabPointer`; the corrected path uses a synchronous pointer grab, arms it
with `SyncPointer`, and replays the original frozen button event with
`ReplayPointer`. `CurrentTime` is required here; using the event timestamp left
the request ineffective on the tested server.

After the fix, the isolated Xvfb observer received the passed press, a
`MotionNotify` carrying the held-button mask, and the release. The final native
pass-through run received 1,298 ordinary relative events, zero missing-relative
events, all four direction signs, and zero `MouseDragged` callbacks. The user
confirmed that drag-to-highlight worked. Zero drag callbacks are expected in
this mode because Monio sees the passed press, then the receiving application
owns the gesture until release. Both native runs released the grab at timeout.
This verifies physical RawMotion delivery, direction signs, edge continuation,
consume-mode drag classification, gesture pass-through, and normal-stop cleanup
on this GNOME X11 system.

Do not make the first CrossFlow implementation depend on selective local
pass-through. While control is on B or C, all captured keyboard and pointer
events should be routed remotely. When control returns to A, release the grabs
and let X11 resume normal local delivery.

The deterministic diagnostic now covers failed/partial grabs and another
client already holding a grab. A product acceptance matrix must still cover
pointer confinement/centering, multi-monitor edges, keys and buttons held
during transition, emergency release, process crash, network disconnect,
wheel gestures, autorepeat, and additional Xorg/window-manager combinations.

## Linux evdev/uinput handoff

### Implemented mechanism

Relevant files:

- `src/platform/linux/evdev/provenance.rs`;
- `src/platform/linux/evdev/listen.rs`;
- `src/platform/linux/evdev/simulate.rs`.

The process owns one persistent `VirtualDevice`. Hook and grab startup create
it before enumerating capture devices. The pinned `evdev` 0.12.2 API resolves
the corresponding `/dev/input/event*` node, and Monio records its live
character-device number (`st_rdev`). Names and advertised input IDs are not
used as provenance.

Each opened capture device retains an `InputOrigin` alongside its `Device`.
Classification uses `fstat` on the opened device fd, avoiding a pathname/open
race. Listen mode includes the Monio node and marks converted events from it as
`ThisMonioSession`. Grab mode fails closed on fd inspection errors and excludes
that exact node so pass-through re-injection cannot feed back into the same
grab loop. Every other device remains `Unknown`.

The virtual device stays alive for the process session, so hook restart reuses
the same kernel identity. Startup fails explicitly if Monio can create the
uinput device but cannot open its event node for listen classification.

The listener still enumerates devices once per hook start. General physical
device hotplug/removal is not newly handled by this provenance slice.

### Native self-loopback verification on 2026-07-30

The Linux host was Ubuntu with kernel `7.0.0-28-generic`; its local GNOME
desktop used X11. The user joined the `input` group, physical event nodes were
`root:input 0660`, and a udev rule made `/dev/uinput` `root:input 0660`.

The first native run exposed a startup race. The evdev crate's
`enumerate_dev_nodes_blocking()` waits for the uinput sysfs child and returns
its `/dev/input/event*` path, but it does not wait for udev to finish changing
the new node from its initial root-only permissions. A syscall trace observed:

```text
openat(..., "/dev/input/event256", O_RDWR|O_CLOEXEC) = -1 EACCES
openat(..., "/dev/input/event256", O_RDONLY|O_CLOEXEC) = -1 EACCES
```

Delaying only that open by 500 ms made the listener start, confirming the race.
Monio now opens the injector event node before recording its device number and
retries transient `NotFound`/`PermissionDenied` results for up to one second.
Permanent permission failures still return a permission-specific error.

The provenance diagnostic was also made backend-aware: evdev sends a relative
`(32, 24)` motion followed by `(-32, -24)` and requires observing the tracked
positions `(32, 24)` and `(0, 0)`. This avoids both the unsupported evdev
`mouse_position()` API and a false positive where the X and Y axis events from
one move were mistaken for outward and return movements.

This native command then passed:

```bash
cargo run --no-default-features --features evdev \
  --example synthetic_input_detection
```

It classified the ControlLeft press/release, relative pointer move, and actual
inverse relative move as `Injected { ThisMonioSession }`. This verifies
creation and permission readiness of the Monio uinput node, listen-mode capture
of that exact node, keyboard/pointer injection, and exact self-device
classification on this host.

It does not yet verify an unrelated uinput device, exclusive grab
consume/pass-through behavior, listener restart, hot-unplug/recreation, or a
Wayland compositor. The broader matrix below remains required.

### Required privileged/container experiments

Use a Linux host or container with explicit `/dev/uinput` and `/dev/input`
authority:

1. Create two virtual devices:
   - Monio's injector;
   - an unrelated test injector.
2. Start capture before and after creating the Monio device.
3. Restart capture while both devices exist.
4. Emit identical key and pointer sequences from both devices.
5. Assert only the exact Monio device is self-classified or excluded.
6. Assert the unrelated virtual device does not become
   `ThisMonioSession`.
7. Test hot-unplug and recreation.
8. Verify no event is retransmitted through a model CrossFlow loop.

Suggested checks:

```bash
cargo test --no-default-features --features evdev,tokio,recorder,statistics --all-targets
cargo clippy --no-default-features --features evdev,tokio,recorder,statistics --all-targets -- -D warnings
cargo fmt --check
```

Host permissions and udev rules are part of the test fixture. Do not hide
permission failures by skipping the native test.

### Native GNOME Wayland verification on 2026-07-30

The same host was restarted into a native GNOME Wayland session:

```text
XDG_SESSION_TYPE=wayland
XDG_CURRENT_DESKTOP=ubuntu:GNOME
GNOME Shell 46.0
kernel 7.0.0-28-generic
libinput 1.25.0
```

The user was an active member of the `input` group. `/dev/input/event*` and
`/dev/uinput` were `root:input 0660`. Codex's default sandbox hid these device
nodes and supplementary groups, so native commands had to run outside that
sandbox; this was a tooling boundary rather than a host permission failure.

The evdev provenance command passed unchanged:

```bash
cargo run --no-default-features --features evdev \
  --example synthetic_input_detection
```

The listener observed the ControlLeft press/release and both relative pointer
moves as `Injected { injector: ThisMonioSession }`.

The selective-grab command was then exercised against ordinary Wayland
applications:

```bash
cargo run --no-default-features --features evdev --example grab
```

The evdev grab received a physical left-button press and Q/W/E key presses.
Q/W/E were consumed and did not reach the focused application. The allowed
A/B/C keys reached it, and passed pointer click, motion, and drag operations
also remained effective. The ten-second safety timeout released the devices.

This disproves the previous claim that uinput pass-through failure is a
fundamental Wayland limitation. Pass-through remains compositor-dependent:
GNOME 46 is positively verified, while KDE and other compositors still require
native testing.

### evdev grab suitability for CrossFlow

The evdev backend uses the kernel's exclusive device grab through
`Device::grab()`/`EVIOCGRAB`. While held, other evdev clients, including the
desktop input stack, do not receive events from that device. This provides the
all-or-nothing source suppression CrossFlow needs on either X11 or Wayland.
`uinput` separately creates virtual devices for target injection or local
re-injection.

Primary references:

- <https://www.freedesktop.org/software/libevdev/doc/latest/ioctls.html>
- <https://www.kernel.org/doc/html/latest/input/uinput.html>

The current Monio evdev grab implementation is evidence that kernel-level
blocking is possible, but it is not yet production-ready as a CrossFlow
capture backend:

- it accepts partial success if at least one enumerated device was grabbed,
  which can leak input from an ungrabbed keyboard or pointer;
- it enumerates devices once and does not yet handle hotplug/removal;
- pass-through re-emits each raw event with a separate `SYN_REPORT`, changing
  the original frame boundaries;
- the shared uinput device advertises a bounded set of common keyboard,
  button, and relative-axis capabilities, not an exact clone of every source
  device;
- direct `/dev/input/event*` and `/dev/uinput` access requires privileged
  installation policy.

For CrossFlow, avoid the pass-through path while remote-active: grab every
required source device atomically, forward captured operations to the remote
target, then ungrab all devices when returning local. If any required device
cannot be grabbed, roll back all grabs and refuse activation.

For a desktop product, evdev should be a deliberate fallback implemented
behind a narrowly scoped privileged helper or explicit device policy. Do not
grant the whole GUI process permanent membership in a broadly privileged
`input` group by default.

## Wayland portal/libei handoff

### Confirmed protocol facts

Ordinary Wayland clients do not receive a general-purpose global input hook.
Compositor-mediated APIs are the intended path.

The XDG InputCapture portal:

- obtains user/compositor permission;
- exposes session-specific `Zones`;
- supports pointer barriers on the outside boundary of available zones;
- activates capture when the compositor decides a barrier was crossed;
- transports captured events through an EIS connection;
- supports release with a suggested cursor position;
- may persist permission through a restore token, subject to portal policy.

InputCapture version 2 defines `persist_mode` and single-use restore tokens.
It does not allow an application to demand immediate capture: the application
enables a session and configures triggers, while the compositor decides when a
pointer barrier activates it. That restriction matches CrossFlow's normal
edge-crossing transition.

The portal documentation is unusually close to CrossFlow's product model:
screen zones, edge barriers, activation, capture, and release are first-class
concepts.

Primary reference:

- <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.InputCapture.html>

libei provides:

- sender-side emulated input;
- receiver-side captured input;
- explicit emulation start/stop/frame lifecycle;
- device objects;
- device types distinguishing virtual and physical devices for receiver
  clients.

Primary references:

- <https://github.com/libinput/libei>
- <https://libinput.pages.freedesktop.org/libei/>
- <https://libinput.pages.freedesktop.org/libei/api/group__libei-device.html>

### Native GNOME portal capability probe on 2026-07-30

The current GNOME Wayland session exposes both required portal interfaces:

```text
xdg-desktop-portal 1.18.4
xdg-desktop-portal-gnome 46.2
org.freedesktop.portal.InputCapture version 1
  SupportedCapabilities = 15
  ConnectToEIS available
org.freedesktop.portal.RemoteDesktop version 2
  AvailableDeviceTypes = 7
  ConnectToEIS available
libei runtime 1.2.1
```

InputCapture version 1 means this host can exercise zones, barriers, capture,
release, and EIS transport, but not the version-2 persistence/restore-token
flow. The libei runtime is installed; its development headers and pkg-config
metadata are not currently installed. The proof of concept uses `ashpd 0.13.13`
and the pure-Rust `reis 0.7.1`, so it does not link the system libei runtime.

`ashpd` and `reis` solve different parts of the path. `ashpd` handles the
portal/D-Bus session and returns an EIS file descriptor. It does not encode or
decode the EIS event stream. That stream still needs the system `libei`, a
protocol client such as `reis`, or a new implementation. The legacy
RemoteDesktop `Notify*` D-Bus methods cannot replace the InputCapture receiver,
and GNOME rejected mixing `NotifyPointerMotion` with `ConnectToEIS` on the same
RemoteDesktop session.

The `reis 0.7.1` source used here was statically reviewed against upstream
commit `02bc1b52aca10231d380c0cf236d69ec3c56cae4`; Cargo locks crates.io checksum
`4b967ec6489a42067a20724f11987bb96178ea796873cbe1afd25fb93ee6f85f`. The crate
forbids unsafe Rust and has no build script, proc macro, external-command
execution, or network client. It is nevertheless a small project without a
published security policy or release tags, and it has not received a
production security audit. Static review found denial-of-service hardening
gaps for a malicious local EIS peer: unbounded pre-parse byte/file-descriptor
queues, an object-ID overflow edge, and an `unwrap` on file-descriptor cloning.
`REIS_DEBUG` also logs protocol/input content. The portal supplies a local
compositor-owned socket, which narrows but does not erase that risk.

Use `reis` as a replaceable diagnostic dependency. For production, prefer
system `libei`, or pin and harden a reviewed `reis` fork behind Monio's own
adapter. `reis` is not architecturally mandatory.

The portal reports `SupportedCapabilities = 15`, although the current public
specification defines only bits 1 (keyboard), 2 (pointer), and 4
(touchscreen), and requires clients to ignore unknown capabilities. The strict
`ashpd 0.13.13` property decoder rejects the unknown bit 8. The proof of
concept therefore requests the required keyboard/pointer mask directly and
uses the granted session mask instead of calling
`InputCapture::supported_capabilities()`.

### Native GNOME InputCapture/EIS verification on 2026-07-30

The proof of concept is feature-gated and intentionally does not replace the
existing `listen()`/`grab()` API:

```bash
cargo run --no-default-features --features wayland-portal,x11 \
  --example wayland_input_capture_detection
```

It creates a small XWayland window only to give the GNOME portal dialog a
stable parent. The source capture path remains native Wayland
InputCapture/EIS; a production desktop application should supply its own
native Wayland window identifier. Parentless requests were unreliable on this
GNOME version and logged:

```text
Failed to associate portal window with parent window
```

The portal returned one logical zone:

```text
Region(3840, 2160, 0, 0)
```

For that zone, GNOME rejected a vertical right barrier at `x=3839` because it
overlapped the monitor region. The accepted outside barrier is
`x = x_offset + width = 3840`, with inclusive Y coordinates `0..=2159`.
This agrees with Deskflow's native logs, where a `1920x1080@2752,0` zone uses
a right barrier at `x=4672`.

Crossing the accepted barrier produced:

```text
activation_id=Some(1)
cursor_position=Some((3840.0, ...))
barrier_id=Some(Barrier(1))
```

Three capture runs all released control successfully. The final run observed:

```text
Relative motion events: 1197
Keyboard events:        12
Button events:          6
Scroll events:          80
```

The keyboard events included A/B/C press and release with evdev keycodes
30/48/46. Pointer buttons used evdev codes 272/273. A conventional wheel
arrived as `ScrollDiscrete` with increments of positive or negative 120, not
as `ScrollDelta`.

EIS delivered the logical devices as:

```text
name=Some("captured relative pointer"), type=Virtual
name=Some("captured keyboard"), type=Virtual
```

This invalidates the earlier proposed policy of treating only EIS `Physical`
devices as eligible source input and dropping every `Virtual` device. On this
GNOME compositor, that policy would drop all real captured keyboard and
pointer input. Portal source events must remain `InputOrigin::Unknown`; the
target-side tests below confirm that device type alone cannot provide exact
self-injection provenance.

An EIS pointer event was observed before the corresponding D-Bus `Activated`
signal was selected by the client. A production implementation must correlate
the EIS `DeviceStartEmulating.sequence` with the portal `activation_id`; it
must not assume cross-transport delivery order.

### Native GNOME RemoteDesktop/EIS and feedback verification on 2026-07-30

The same diagnostic can create a simultaneous RemoteDesktop v2 sender:

```bash
cargo run --no-default-features --features wayland-portal,x11 \
  --example wayland_input_capture_detection -- --inject-self-test
```

GNOME granted keyboard and pointer control and advertised three EIS sender
devices: an absolute pointer, a relative pointer, and a keyboard. Pointer
buttons and scrolling were available on the pointer devices. A visible
relative-motion preflight and `XQueryPointer` coordinate changes verified that
target-side EIS injection reaches the desktop.

After a physical pointer crossed the InputCapture barrier, the sender injected
a distinctive sequence. Every injected event was received by the active
InputCapture session:

```text
PointerMotion: relative=(37.000, -23.000)
KeyboardKey: key=30 state=Press
KeyboardKey: key=30 state=Released
Button: button=272 state=Press
Button: button=272 state=Released
Scroll: discrete=(0, 120)
```

The diagnostic then performed a same-process disabled/enabled control. While
InputCapture was disabled, the RemoteDesktop sender placed the absolute
pointer at `(2520, 719)` and sent relative motion `(120, 0)`. No `Activated`
signal arrived within two seconds.

After InputCapture was enabled with the same barrier, the identical sequence
activated the right barrier:

```text
triggered_by_target_injection=true
cursor_position=Some((2560.0, 719.0))
barrier_id=Some(Barrier(1))
```

After that injected activation, relative motion `(37, -23)` and its inverse,
evdev key 30 press/release, button 272 press/release, and discrete scroll 120
all echoed into InputCapture with `DeviceType::Virtual`. An earlier attempt
observed no post-activation echo, but the controlled repeat disproved that as
a stable isolation property.

The control directly validates the CrossFlow gate: disabled capture resisted
target injection; enabled capture self-activated and echoed it. Leaving
InputCapture armed on every machine is unsafe.

### Proposed Wayland architecture

Source/capture side:

```text
XDG InputCapture portal
  -> ConnectToEIS
  -> passive libei receiver
  -> CrossFlow Platform Host capture adapter
```

Target/injection side:

```text
XDG RemoteDesktop portal
  -> ConnectToEIS
  -> libei sender
  -> compositor input stack
```

The RemoteDesktop portal grants keyboard/pointer injection after user approval
and returns an EIS file descriptor suitable for a libei sender context:

- <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html>

For feedback-loop prevention, do not classify captured EIS devices solely from
`DeviceType`. GNOME 46 represents captured local keyboard and pointer streams
as `Virtual`, and a physically activated InputCapture session echoed events
from a simultaneous RemoteDesktop sender.

Use a target-authoritative role/lease invariant instead:

1. Only the current physical source machine may have InputCapture enabled.
2. Before a machine accepts an inbound target lease, call
   `InputCapture.Disable` and confirm source routing is stopped.
3. While the inbound lease is active, do not publish any local capture stream
   from that machine, regardless of `DeviceType`.
4. On target-to-source transition, stop/release inbound emulation first, then
   set barriers and enable InputCapture under a new epoch.
5. Drop operations carrying a stale source or target epoch.

The original source remains responsible for the logical cursor while it moves
across B and C; B does not need to become a source merely because B is the
current injection target. Local physical takeover on B is a separate, later
feature and needs an explicit takeover gesture or stronger device evidence.

### Synergy/Deskflow Wayland evidence

Synergy supports Linux. As of 2026-07-30, its documentation describes Wayland
keyboard/pointer sharing as experimental and lists GNOME 46+ and KDE Plasma
6.1+ as supported desktop environments. Availability is not universal across
Wayland compositors because the active portal backend must implement
InputCapture.

References:

- <https://help.symless.com/hc/en-us/articles/35748398109841-Wayland-support-on-Linux>
- <https://symless.com/synergy/open-source>
- <https://github.com/deskflow/deskflow>

Deskflow's source confirms the architecture rather than merely advertising
Wayland support:

- a primary/source `EiScreen` creates a libei receiver and
  `PortalInputCapture`;
- a secondary/target `EiScreen` creates a libei sender and
  `PortalRemoteDesktop`;
- source screen edges become portal pointer barriers;
- activation delivers relative pointer, keyboard, button, and scroll events
  through libei;
- returning local calls portal `Release` with a suggested cursor position;
- target events are injected through the RemoteDesktop EIS connection.

Primary source:

- <https://github.com/deskflow/deskflow/blob/master/src/lib/platform/EiScreen.cpp>
- <https://github.com/deskflow/deskflow/blob/master/src/lib/platform/PortalInputCapture.cpp>
- <https://github.com/deskflow/deskflow/blob/master/src/lib/platform/PortalRemoteDesktop.cpp>

### Unknowns requiring a native compositor matrix

Do not assume these without testing:

- whether the installed portal backend implements InputCapture version 2;
- whether GNOME, KDE, wlroots-based compositors, and nested compositors expose
  identical zone/barrier behavior;
- whether other compositors reproduce GNOME's physical-activation echo and
  target-injection barrier activation behavior;
- whether the receiver exposes enough stable identity to distinguish
  CrossFlow's virtual sender from every other virtual sender;
- permission persistence and restore-token behavior across login/reboot;
- keyboard layout, keymap, relative motion, high-resolution wheel, and
  multi-monitor coordinate fidelity;
- behavior when zones change while a remote session is active.

The compositor knows emulated input separately from physical input, but ordinary
Wayland application events are not a reliable provenance surface. Keep the
portal/libei adapter inside Monio's platform layer.

### Native Wayland acceptance tests

At minimum:

1. Detect portal availability, version, and capabilities without hanging.
2. Create and close a session after user denial.
3. Restore a previously permitted session where supported.
4. Retrieve zones and reconfigure after `ZonesChanged`.
5. Activate at each configured pointer barrier.
6. Capture keyboard, relative pointer, button, and wheel events.
7. Inject the same event kinds on a target session.
8. Verify an inbound target lease disables InputCapture before injection.
9. Verify injected target motion cannot activate a disabled source barrier.
10. Verify no target event is retransmitted even when the compositor echoes it.
11. Release capture and restore/suggest cursor placement.
12. Repeat on at least one GNOME and one KDE native host before claiming broad
    Wayland support.

Nested compositor tests may automate protocol behavior, but they do not replace
native permission and desktop-environment tests.

## Recommended Linux backend order

For a desktop application:

```text
Wayland session
  ordinary source capture and CrossFlow edge activation:
    1. InputCapture portal + passive libei receiver
    2. explicit capability error if unavailable
    3. evdev only for an intentionally privileged deployment

  target injection:
    1. RemoteDesktop portal + libei sender
    2. uinput only for an intentionally privileged deployment

X11 session
  ordinary listen/provenance:
    1. XRecord/XTest

  CrossFlow source capture while remote-active:
    1. active X11 keyboard/pointer grab
    2. evdev exclusive grab as a privileged fallback

  target injection:
    1. XTest
    2. uinput as a privileged fallback
```

For a headless agent, appliance, test container, or explicitly privileged
installation:

```text
evdev/uinput
```

Do not silently select XRecord/XTest on Wayland through XWayland. It cannot see
or control native Wayland clients as a complete global input backend.

## TODO: CrossFlow cross-platform relative capture parity

The operating systems expose different mechanisms for cursor-independent
relative pointer motion. Do not treat native capability as evidence that the
current Monio backend already publishes `MouseData::relative` or provides a
fail-safe CrossFlow capture lifecycle.

| Platform/backend | Native capability | Current Monio state | Remaining work |
| --- | --- | --- | --- |
| macOS Core Graphics | `CGEventField::MouseEventDeltaX/Y` reports movement since the previous mouse event. `CGAssociateMouseAndMouseCursorPosition(false)` can keep the cursor stationary while events continue to carry deltas. | `listen()` and `grab()` retain absolute `x/y` and publish `MouseData::relative`; relative injection sets the event delta fields while retaining its absolute target. `RelativePointerCapture` now owns save/hide/disassociate and associate/warp/show restoration. Focused conversion, real lease, exclusivity, drop, hook-cleanup, and restoration-retry tests pass on macOS. | Run native physical and posted-event edge tests and exercise disconnect/crash recovery from a CrossFlow host process. |
| Windows | Raw Input delivers `WM_INPUT`/`RAWMOUSE`; `MOUSE_MOVE_RELATIVE` exposes `lLastX/lLastY` independently of the low-level hook's absolute cursor point. `SendInput` can inject relative `MOUSEEVENTF_MOVE`, and `ClipCursor` can constrain the visible cursor during an owned capture session. | The `WH_MOUSE_LL` backend reports only absolute `MSLLHOOKSTRUCT.pt`. `mouse_move_relative()` exists but is implemented as current position plus an absolute move. `RelativePointerCapture::acquire()` explicitly returns `Error::NotSupported`. | Design a Raw Input motion stream plus low-level-hook suppression/provenance path, including event ordering, deduplication, registration ownership, injection semantics, and fail-safe cursor release. |
| Linux X11 | XI2 `XI_RawMotion` remains available at the screen edge while active X11 grabs provide suppression. | Implemented for `grab()` and natively verified at the screen edge; relative XTest injection is also implemented. `RelativePointerCapture` is a no-op because the backend capture already supplies cursor-independent deltas. | Retain the existing path and complete the remaining window-manager, multi-monitor, transition, and failure-lifecycle matrix. |
| Linux evdev/uinput | Kernel `REL_X/REL_Y` events are already cursor-independent hardware deltas; `EVIOCGRAB` provides exclusive source ownership. | Relative injection, capture, exclusive grab, and GNOME Wayland pass-through have been natively verified. The listener coalesces `REL_X/REL_Y` per device through `SYN_REPORT`, retains tracked absolute `x/y`, publishes `MouseData::relative`, and replays an allowed grab motion as one uinput frame. `RelativePointerCapture` is a no-op. | Re-run the native injection, grab, provenance, and GNOME Wayland regression matrix with the public relative field asserted. |
| Linux Wayland portal/libei | InputCapture barriers plus an EIS receiver provide compositor-owned activation and relative capture; RemoteDesktop plus an EIS sender provides target injection. | Native GNOME proofs pass, including relative motion and capture/target role gating, but the production backend is not implemented. | Implement the production adapter, capability reporting, release/suggested-cursor restoration, and GNOME/KDE acceptance matrix. |

### Deskflow reference implementation

Deskflow master commit
[`0bc325a`](https://github.com/deskflow/deskflow/commit/0bc325addc2720de4fdad637b34b20d780f681a4)
was inspected on 2026-07-30. Its production choices provide a useful
CrossFlow reference, but they are not identical across platforms:

- macOS uses a `CGEventTap`; while the pointer is routed to another machine,
  [`OSXScreen::leave()`](https://github.com/deskflow/deskflow/blob/0bc325addc2720de4fdad637b34b20d780f681a4/src/lib/platform/OSXScreen.mm#L817-L835)
  calls `CGAssociateMouseAndMouseCursorPosition(false)`, and
  [`onMouseMove()`](https://github.com/deskflow/deskflow/blob/0bc325addc2720de4fdad637b34b20d780f681a4/src/lib/platform/OSXScreen.mm#L991-L1026)
  reads `kCGMouseEventDeltaX/Y`. The event tap consumes off-screen input and
  re-associates the cursor on return. It does not use `IOHIDManager`.
- Windows uses
  [`WH_MOUSE_LL`](https://github.com/deskflow/deskflow/blob/0bc325addc2720de4fdad637b34b20d780f681a4/src/lib/platform/MSWindowsHook.cpp#L474-L620),
  derives deltas from consecutive absolute hook positions, recenters with
  `SetCursorPos`, and filters the synthetic warp interval with
  `DESKFLOW_MSG_PRE_WARP`/`POST_WARP`. It does not use Raw Input.
- X11 registers `XI_RawMotion`, but the
  [current handler](https://github.com/deskflow/deskflow/blob/0bc325addc2720de4fdad637b34b20d780f681a4/src/lib/platform/XWindowsScreen.cpp#L1166-L1190)
  queries the current absolute pointer position instead of reading XI2 raw
  valuators. It derives a delta and periodically recenters the pointer.
  Monio's existing XI2 raw-valuator path and evdev `REL_X/Y` path are stronger
  relative-motion sources.
- Wayland uses Portal InputCapture plus libei and forwards
  [`ei_event_pointer_get_dx/dy`](https://github.com/deskflow/deskflow/blob/0bc325addc2720de4fdad637b34b20d780f681a4/src/lib/platform/EiScreen.cpp#L829-L856)
  directly, with libei also providing target-side relative injection.

Deskflow therefore validates a split design: native delta capture is distinct
from target-side injection, and cursor ownership is an explicit route/session
transition rather than an incidental property of a generic hook callback.

Cross-platform TODOs:

- [x] Align macOS `listen()` and `grab()` motion events with
  `MouseData::relative`, while retaining absolute `x/y` and existing drag
  classification.
- [x] Make macOS relative injection construct an event with both the absolute
  target position and `MouseEventDeltaX/Y`.
- [ ] Run a native posted-event round trip and verify the macOS event tap
  retains the requested target and delta fields.
- [ ] Run a physical macOS screen-edge diagnostic. Record whether nonzero
  deltas continue while the visible cursor is clamped.
- [x] Implement an explicit cursor-disassociated capture lease for the
  CrossFlow-remote-active state, following the source-verified Deskflow
  pattern. `RelativePointerCapture` saves and restores cursor position and
  visibility, re-associates on explicit release, drop, and hook/channel
  shutdown, and never activates implicitly from generic `grab()`.
- [ ] Add Windows Raw Input relative capture behind an owned message-window
  lifecycle. Account for the Win32 rule that only one window per raw-input
  device class is registered per process, so a library must not silently
  replace its host application's registration.
- [ ] Correlate Windows Raw Input motion with `WH_MOUSE_LL` suppression and
  Monio provenance without double-emitting movement or weakening
  `ThisMonioSession`.
- [ ] Decide and document whether Windows CrossFlow injection should use true
  relative `SendInput(MOUSEEVENTF_MOVE)` and target-side pointer acceleration,
  or preserve the current absolute-target calculation; verify the selected
  semantics natively in both ordinary desktop and screen-edge cases.
- [x] Expose the already-working Linux evdev relative capture through
  `MouseData::relative`, coalescing `REL_X/REL_Y` from one `SYN_REPORT` frame
  into one movement event without regressing the verified injection, grab,
  provenance, or Wayland pass-through paths.
- [ ] Re-run the native evdev diagnostic and assert the coalesced public
  relative field as well as the previously verified cursor displacement,
  suppression, provenance, and pass-through behavior.
- [ ] Expose backend capabilities and guarantees explicitly: relative capture,
  edge continuation, suppression, cursor restoration, and whether deltas are
  raw, accelerated, or compositor-defined.
- [x] Put macOS cursor disassociation and emergency restoration in the
  dedicated `RelativePointerCapture` lifecycle, not in the generic callback
  API. Windows cursor-independent capture remains a separate Raw Input task.

### `RelativePointerCapture` contract

The public lease deliberately does not start or stop an event hook:

```rust
let capture = monio::RelativePointerCapture::acquire()?;
// Keep an existing grab hook active and route MouseData::relative remotely.
capture.release()?;
# Ok::<(), monio::Error>(())
```

- The lease is process-wide and exclusive. A second acquisition returns
  `Error::RelativePointerCaptureAlreadyActive`.
- macOS acquisition records the global cursor point and display, hides the
  cursor, then calls `CGAssociateMouseAndMouseCursorPosition(false)`.
- macOS restoration always attempts associate, warp to the saved point, then
  show. Completed steps are recorded so a retry cannot over-increment cursor
  visibility or repeat a successful warp.
- Explicit release reports restoration errors. `Drop` performs a best-effort
  retry and never panics.
- Every blocking/async Hook exit and `ChannelHookHandle` stop/drop calls the
  same process-wide cleanup helper. This is a safety net for route teardown;
  normal CrossFlow code should still release the guard explicitly when routing
  returns local.
- Linux X11 and evdev implement the lease as a no-op because their selected
  capture paths already provide cursor-independent deltas.
- Windows rejects acquisition until a cursor-independent capture backend is
  implemented.

Primary Windows references:

- <https://learn.microsoft.com/windows/win32/inputdev/about-raw-input>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-rawmouse>
- <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-registerrawinputdevices>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-mouseinput>

## CrossFlow Linux capture model

CrossFlow is not the same problem as a generic per-event filtering hook.
Generic `grab()` promises that each event can independently be consumed or
passed through. CrossFlow instead needs an explicit ownership transition:

```text
A local
  -> pointer crosses an armed edge
  -> capture activates on A
  -> virtual pointer and keyboard are routed to B
  -> virtual pointer crosses B's remote edge
  -> routing target changes from B to C
  -> virtual pointer returns to A's boundary
  -> capture releases and A resumes normal local input
```

A remains the physical keyboard/mouse owner while the virtual pointer is on B
or C. Moving from B to C changes only CrossFlow's routing target; it does not
require B or C to grab A's devices. If every computer may later become an
owner, each computer independently implements the same optional source
capture role.

Recommended platform mapping:

| Session on source A | Edge activation and suppression | Target-side injection |
| --- | --- | --- |
| X11 | Active `XGrabKeyboard` + `XGrabPointer` session | XTest when target is X11 |
| Wayland with supported portal | InputCapture barriers + libei receiver | RemoteDesktop portal + libei sender |
| Portal unavailable/headless/managed appliance | Privileged evdev `EVIOCGRAB` helper | Privileged uinput helper |

macOS and Windows continue using their native Monio capture and injection
backends. The source and target backend choices are independent; for example,
an X11 source may control a Wayland target through that target's
RemoteDesktop/libei backend.

### Capture-session API direction

Do not force CrossFlow activation into the existing
`grab(callback) -> Option<Event>` contract. A dedicated capability and
lifecycle API can model the platform facts more accurately:

```text
arm(pointer barriers)
  -> activated(edge, cursor_position, activation_id)
  -> captured input stream
  -> release(suggested_cursor_position)
  -> deactivated
```

The exact Rust API remains a proposal. It should expose backend-independent
states and capabilities, not X11 windows, evdev fds, portal objects, or libei
devices.

### Required fail-safe behavior

A production capture session must:

- acquire all required keyboard and pointer control atomically or not activate;
- never leave a partial grab after an error;
- release immediately on process shutdown, portal revocation, network loss,
  lease expiry, or target disconnect;
- release all keys/buttons on the previous target before switching targets or
  ending the session;
- define what happens when an edge is crossed while a button is held, so drag
  state cannot become stuck on two machines;
- retain an emergency local release path that does not depend on the network;
- handle source-device and display-topology hotplug;
- keep self-injection provenance filtering enabled on every target listener.

## CrossFlow integration rules

Monio reports platform facts. CrossFlow owns routing and input-control policy.

### Required V1 behavior

| Captured origin | CrossFlow action |
| --- | --- |
| `ThisMonioSession` | Drop locally; never send; never claim input ownership |
| `Unknown` | Treat as local-input candidate, not physical proof |
| Future virtual-other evidence | Default to no retransmission or no automatic takeover |
| Future device-backed evidence | May support local takeover policy, but not remote authorization |

Capture and injection should be owned by one Native Platform Host process per
active local user session. The Node Agent/Fabric may carry typed input
operations, but it should not own OS hooks or platform permissions.

High-frequency input is not CRDT state and should not be broadcast through
Gossip. A target-authoritative lease/epoch controls which source may send
operations. Provenance filtering happens before an event becomes a CrossFlow
operation.

On Wayland, lease state is also a native capture gate. A machine entering the
target role must disable its InputCapture session before RemoteDesktop/EIS
injection starts. This is required even if no captured events are currently
being forwarded: GNOME target injection can activate an armed barrier. Only
the active source epoch may enable InputCapture.

### Minimal data flow

```text
native capture
  -> classify provenance
  -> drop self-injected
  -> check local source lease/policy
  -> normalize typed input operation
  -> CrossFlow transport
  -> target native injection
```

If local physical takeover is later required, use the strongest platform
device-backed signal available. Do not implement it by changing
`Unknown == physical`.

## Suggested Monio architecture

Keep the public event model small while moving backend facts through typed
internal context:

```rust
struct CapturedPlatformEvent {
    event: Event,
    origin: InputOrigin,
    // Private backend evidence may live here until a stable public contract is
    // justified.
}
```

Potential future capability reporting:

```rust
struct InputCapabilities {
    self_injection_detection: bool,
    physical_device_evidence: bool,
    global_capture: bool,
    suppression: bool,
    relative_pointer: bool,
}
```

This is a proposal, not an implemented API. Do not add it without tests and a
clear compatibility decision.

Prefer backend modules with the same conceptual ports:

```text
CaptureBackend
InjectionBackend
DisplayTopologyBackend
InputCapabilityReport
```

The backend may compose those responsibilities internally, but portable callers
must not depend on X11 handles, evdev devices, portal session objects, or Win32
hook structures.

## Provenance implementation order

Recommended sequence:

1. Linux evdev/uinput privileged E2E for the implemented exact-device identity.
2. Linux X11 unrelated-client, stress, restart, and autorepeat acceptance.
3. A capability-reporting design for backend-specific guarantees.
4. **Done on GNOME 46:** Wayland portal/libei source and target proofs of
   concept, including echo and injected-barrier activation diagnostics.
5. Wayland production zone/barrier, role-gating, and capture/release
   integration.
6. Native GNOME/KDE compatibility matrix.

Each platform slice should be separately reviewable and must preserve the
macOS behavior.

## CrossFlow capture implementation order

The current development host is X11. Recommended sequence:

1. **Done:** replace the generic X11 `grab()` fallback with active keyboard and
   pointer grabs without changing XRecord provenance.
2. **Done on Xvfb and native GNOME X11:** verify keyboard/button/motion
   consume, key and pointer gesture pass-through, conflicting owners,
   partial-grab rollback, and normal-stop cleanup.
3. **Done:** rerun the success path on an unlocked native GNOME X11 session.
4. Model arm/activate/remote-active/release as a separate CrossFlow capture
   lifecycle; do not make CrossFlow depend on generic per-event pass-through.
5. Verify wheel, autorepeat, held-state transitions, multi-monitor edges,
   emergency release, and disconnect cleanup on native X11.
6. **Done on native GNOME 46:** run the portal/libei capability and event probe.
7. **Proof complete:** InputCapture barriers plus a passive EIS receiver on
   the source side; production adapter pending.
8. **Proof complete:** RemoteDesktop plus an EIS sender on the target side;
   production adapter pending.
9. Validate GNOME and KDE independently and report unsupported portal backends
   as capabilities, not generic Linux failure.
10. Harden the evdev/helper fallback only if unsupported compositors or managed
   deployments require it.

## Definition of done for a provenance backend

A platform backend is not complete until:

- every Monio injection path carries or owns the intended self identity;
- capture retains enough backend context to classify it;
- exact self input is positively recognized;
- zero, mismatched, unrelated, and missing evidence remain non-self;
- the native diagnostic exits non-zero if any expected self event is missing;
- restart and lifecycle behavior are tested;
- public documentation says what `Unknown` does and does not mean;
- all default and feature-specific builds remain compatible;
- the result was exercised on the native OS, not only cross-compiled.

## Instructions for another AI

Before changing code:

1. Read `AGENTS.md`.
2. Read this document completely.
3. Inspect the current platform source; this document may be stale after the
   commit listed above.
4. Run `git status` and preserve unrelated work.
5. Write a failing focused test before changing the backend.
6. Keep the task to one native platform/backend.
7. Do not implement CrossFlow ownership policy inside Monio.
8. Do not call `Unknown` physical.
9. Run the native diagnostic and record its exact output.
10. Update this document's status, confirmed facts, experiments, and remaining
    unknowns.

Suggested Linux evdev/uinput prompt:

```text
Read AGENTS.md and docs/input-provenance-cross-platform-handoff.md completely.
Verify the implemented exact self-device provenance for the evdev/uinput
backend with privileged native/container E2E. Cover listener restart, a second
unrelated virtual device, permission failures, and grab feedback. Update the
handoff with exact commands and results. Do not use device name alone as
identity, and do not claim completion from compile checks.
```

Suggested Wayland research prompt:

```text
Read AGENTS.md and docs/input-provenance-cross-platform-handoff.md completely.
Do a read-only native Wayland portal/libei feasibility probe first. Record
desktop environment, compositor, portal versions/capabilities, permission
flow, zones, barriers, receiver device types, and whether injected virtual
events are echoed into capture. Do not implement a production backend until
the evidence and a bounded design are reviewed.
```
