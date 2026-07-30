# Cross-platform input provenance handoff

Status: macOS and Windows self-injection detection are implemented and natively
verified. Linux evdev/uinput exact-device classification is implemented and
compile-verified, but its privileged native acceptance matrix has not yet run.
Linux X11 request correlation is implemented and natively verified on GNOME
X11. Wayland portal/libei is not implemented.

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
| Linux evdev/uinput | Exact live character-device number owned by Monio's uinput handle | Implemented; compile-verified, privileged E2E pending |
| Linux X11 XRecord/XTest | Persistent XTest client ID plus ordered request/device-event correlation | Implemented and natively verified on GNOME X11 |
| Wayland portal/libei | Compositor-mediated device/session evidence; at minimum exclude virtual devices from local-source capture | Proposed; backend does not exist |

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
- `src/platform/windows/simulate.rs`;
- `examples/synthetic_input_detection.rs`.

Windows initializes one random, nonzero 32-bit process-session tag. Every
keyboard and mouse `SendInput` record stores it in `dwExtraInfo`. Low-level
keyboard and mouse hooks preserve `dwExtraInfo` plus the injected flags and
report `ThisMonioSession` only when the exact active tag matches and
`LLKHF_INJECTED` or `LLMHF_INJECTED` is present. Zero, mismatched, untagged, and
other-injector events remain `Unknown`.

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

## Linux X11 handoff

### Implemented mechanism

Relevant files:

- `src/platform/linux/x11/provenance.rs`;
- `src/platform/linux/x11/listen.rs`;
- `src/platform/linux/x11/simulate.rs`.

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

Full `cargo clippy --features x11 --all-targets -- -D warnings` was blocked by
pre-existing `unnecessary_map_or` warnings in `examples/grab.rs`; the X11
library target passed with warnings denied.

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

### Verification recorded on 2026-07-30

The Linux host was Ubuntu with kernel `7.0.0-28-generic`; its local GNOME
desktop used X11. These checks passed:

```bash
cargo check --no-default-features --features evdev
cargo fmt --all -- --check
```

The current user was not a member of the `input` group and `/dev/uinput` was
owned by root with mode `0600`, so the privileged native loopback experiment
was not run. This implementation must not be described as natively verified
until the matrix below passes.

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

### Existing Wayland-related comments need revalidation

Current source comments say a Wayland compositor may ignore re-injected uinput
events. That behavior is compositor/environment dependent and has not been
revalidated as part of the provenance work.

Record the compositor, desktop environment, libinput version, privileges, and
observed behavior before turning that comment into a product invariant.

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
XDG RemoteDesktop portal or compositor-provided EIS session
  -> libei sender
  -> compositor input stack
```

For feedback-loop prevention, the safe initial policy is:

```text
physical receiver device -> eligible local source input
virtual receiver device  -> do not retransmit
```

Dropping all virtual devices is stricter than identifying only CrossFlow's own
virtual device, but it meets the V1 loop-prevention requirement and avoids
pretending the generic protocol exposes an exact cross-session injector
credential.

### Unknowns requiring a native compositor matrix

Do not assume these without testing:

- whether the installed portal backend implements InputCapture version 2;
- whether GNOME, KDE, wlroots-based compositors, and nested compositors expose
  identical zone/barrier behavior;
- whether a RemoteDesktop/libei sender is ever echoed into a simultaneous
  InputCapture receiver session;
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
8. Verify virtual events are not retransmitted.
9. Release capture and restore/suggest cursor placement.
10. Repeat on at least one GNOME and one KDE native host before claiming broad
    Wayland support.

Nested compositor tests may automate protocol behavior, but they do not replace
native permission and desktop-environment tests.

## Recommended Linux backend order

For a desktop application:

```text
Wayland session
  1. portal/libei backend
  2. explicit capability error if unavailable
  3. evdev/uinput only for an intentionally privileged deployment

X11 session
  1. XRecord/XTest for unprivileged global capture, simulation, and
     request-correlated self provenance
  2. evdev/uinput when kernel-level device access or suppression is required
```

For a headless agent, appliance, test container, or explicitly privileged
installation:

```text
evdev/uinput
```

Do not silently select XRecord/XTest on Wayland through XWayland. It cannot see
or control native Wayland clients as a complete global input backend.

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

## Implementation order

Recommended sequence:

1. Linux evdev/uinput privileged E2E for the implemented exact-device identity.
2. Linux X11 unrelated-client, stress, restart, and autorepeat acceptance.
3. A capability-reporting design for backend-specific guarantees.
4. Wayland portal/libei proof of concept on one supported compositor.
5. Wayland zone/barrier and capture/release integration.
6. Native GNOME/KDE compatibility matrix.

Each platform slice should be separately reviewable and must preserve the
macOS behavior.

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
