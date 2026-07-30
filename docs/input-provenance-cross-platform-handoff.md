# Cross-platform input provenance handoff

Status: macOS self-injection detection is implemented and verified. Windows,
Linux X11, Linux evdev/uinput, and Wayland portal/libei work described below is
not implemented unless explicitly marked otherwise.

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
| Windows low-level hooks | Random process-session `dwExtraInfo`, with injected hook flags retained as additional evidence | Proposed; current backend discards both |
| Linux evdev/uinput | Exact Monio-created virtual-device identity | Proposed; current listener discards device identity |
| Linux X11 XRecord/XTest | No direct per-event tag in the current path | Unresolved legacy fallback |
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

## Windows handoff

### Facts confirmed from current source

Current capture:

- `src/platform/windows/listen.rs`
- low-level `WH_KEYBOARD_LL` and `WH_MOUSE_LL` hooks;
- callbacks receive `KBDLLHOOKSTRUCT` and `MSLLHOOKSTRUCT`;
- conversion currently retains key, button, position, wheel, and mask data;
- conversion does not retain hook flags or `dwExtraInfo`.

Current injection:

- `src/platform/windows/simulate.rs`
- uses `SendInput`;
- both `KEYBDINPUT.dwExtraInfo` and `MOUSEINPUT.dwExtraInfo` are currently `0`.

Therefore Windows currently reports `InputOrigin::Unknown`, including for
Monio's own `SendInput` events.

### Relevant Win32 evidence

Low-level hook structures provide:

- `KBDLLHOOKSTRUCT.flags`, including `LLKHF_INJECTED` and
  `LLKHF_LOWER_IL_INJECTED`;
- `MSLLHOOKSTRUCT.flags`, including `LLMHF_INJECTED` and
  `LLMHF_LOWER_IL_INJECTED`;
- `dwExtraInfo` in both hook structures;
- `dwExtraInfo` in `KEYBDINPUT` and `MOUSEINPUT`.

Primary references:

- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-kbdllhookstruct>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-msllhookstruct>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-keybdinput>
- <https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-mouseinput>
- <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-sendinput>

### Proposed Windows implementation

1. Create a process-global random, non-zero session tag with the native
   pointer-sized `dwExtraInfo` representation.
2. Write it into every keyboard and mouse `SendInput` path.
3. Preserve hook flags and `dwExtraInfo` before constructing the public event.
4. Report `ThisMonioSession` only for an exact active tag match.
5. Retain injected flags as backend evidence, but do not call a missing injected
   flag proof of physical input.
6. Keep all other events `Unknown` in the initial slice.

Whether an exact self tag should additionally require the Windows injected flag
must be decided by a native characterization test. The conservative starting
point is to require both when Windows reliably supplies both for Monio's own
`SendInput` path.

### Required Windows TDD/native experiments

Run on a real Windows host:

1. Add a failing test showing a new event defaults to `Unknown` if necessary.
2. Add pure classification tests:
   - exact non-zero tag and expected injected flag -> self;
   - zero tag -> unknown;
   - mismatched tag -> unknown;
   - injected flag from another injector -> unknown.
3. Modify the existing diagnostic, not a timing-only replacement:

   ```powershell
   cargo run --example synthetic_input_detection
   ```

4. Require keyboard press/release and pointer target/restoration to report
   `ThisMonioSession`.
5. Add a negative native test using an independently chosen `dwExtraInfo` value.
6. Run:

   ```powershell
   cargo test --all-features --all-targets
   cargo clippy --all-features --all-targets -- -D warnings
   cargo fmt --check
   ```

Do not report completion from a macOS cross-compile. A cross-compile proves API
compatibility, not Win32 runtime behavior.

## Linux X11 handoff

### Facts confirmed from current source

Current X11 capture:

- `src/platform/linux/x11/listen.rs`;
- XRecord records device events from all clients;
- conversion keeps only event type, code, pointer coordinates, and derived
  modifier/button state;
- the public origin remains `Unknown`.

Current X11 injection:

- `src/platform/linux/x11/simulate.rs`;
- uses XTest fake key, button, motion, and scroll events;
- XTest calls do not carry a Monio user-data field.

The XRecord specification explicitly says `XTestFakeInput` causes a device
event to be recorded. In the current device-event callback, that event has no
Monio session tag.

Primary references:

- <https://www.x.org/releases/X11R7.6/doc/recordproto/record.html>
- <https://www.x.org/releases/current/doc/xextproto/xtest.pdf>

### Consequence

The current pure XRecord/XTest path cannot provide the same direct self-tag
contract as macOS or Windows.

Timing, coordinates, key values, and a short suppression window are heuristics.
They can suppress unrelated physical input or fail under scheduling delay.
They must not be documented as reliable provenance.

### Hypothesis requiring investigation

XRecord can record protocol requests as well as device events. It may be
possible to record XTest requests from Monio's known X client and correlate
them with subsequently recorded device events in server order.

This is not yet proven to be:

- race-free with multiple clients;
- lossless at high pointer rates;
- able to associate one request with one output event without ambiguity;
- simpler or safer than using evdev/uinput.

Treat request correlation as a research fallback, not the recommended Linux
architecture.

## Linux evdev/uinput handoff

### Facts confirmed from current source

Current evdev capture:

- `src/platform/linux/evdev/listen.rs`;
- enumerates `/dev/input/event*` once when the hook starts;
- opens every accessible device supporting key or relative events;
- stores `Device` values by file descriptor;
- `convert_event` receives only an `InputEvent`, so device identity is lost
  before the public `Event` is created;
- it does not exclude Monio's own uinput device.

Current uinput injection:

- `src/platform/linux/evdev/simulate.rs`;
- lazily creates one `VirtualDevice`;
- current name is `monio grab passthrough`;
- reuses that device for keyboard, button, relative pointer, and wheel events.

Current risks:

1. If the listener starts before the lazy virtual device exists, it will not
   monitor that device during the current one-time enumeration.
2. If the listener starts or restarts after the virtual device exists, it may
   include Monio's own device.
3. Current conversion cannot say which device produced an event.
4. A name-only exclusion is spoofable and may exclude an unrelated device.
5. Hotplug and device removal are not modeled.

### Recommended evdev/uinput direction

Use exact device identity, not event-value correlation:

1. Create the Monio uinput device before capture enumeration.
2. Give the virtual device a process-session identity using the strongest
   metadata supported by the selected `evdev` crate and kernel APIs.
3. Resolve and retain the exact resulting `/dev/input/event*` node or equivalent
   stable identity owned by the `VirtualDevice`.
4. Change the event loop so conversion receives a typed device context together
   with each `InputEvent`.
5. Classify events from that exact device as `ThisMonioSession`, or exclude the
   device from local capture entirely.
6. Preserve physical/other-device events as `Unknown` until a separate
   device-backed contract is designed.
7. Handle device hotplug and hook restart explicitly.

The exact stable identity mechanism is an implementation question for the
native Linux host. Candidate evidence includes the created event node,
sysfs/udev identity, input ID, physical path, and unique field. Verify which
fields survive uinput creation and are exposed by the pinned `evdev` version.
Do not assume a unique device name alone is sufficient.

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
  1. evdev/uinput when the installation grants device permissions
  2. XRecord/XTest compatibility fallback with Unknown provenance
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

1. Windows tag/flags implementation and native diagnostic.
2. Linux evdev/uinput exact-device identity and privileged E2E.
3. A capability-reporting design so unsupported X11 provenance is explicit.
4. Wayland portal/libei proof of concept on one supported compositor.
5. Wayland zone/barrier and capture/release integration.
6. Native GNOME/KDE compatibility matrix.
7. Decide whether pure X11 request correlation is worth maintaining.

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

Suggested Windows prompt:

```text
Read AGENTS.md and docs/input-provenance-cross-platform-handoff.md completely.
Implement only the Windows self-injection provenance slice with TDD. Preserve
the macOS contract. Use a random process-session dwExtraInfo tag, retain the
low-level hook evidence, run the synthetic_input_detection example natively,
and update the handoff with exact commands and results. Do not claim that an
untagged event is physical.
```

Suggested Linux evdev/uinput prompt:

```text
Read AGENTS.md and docs/input-provenance-cross-platform-handoff.md completely.
Implement only exact self-device provenance for the evdev/uinput backend with
TDD and privileged native/container E2E. Preserve device identity through
capture, exclude or classify only Monio's exact virtual device, cover listener
restart and a second unrelated virtual device, and update the handoff with
exact evidence. Do not use device name alone as identity.
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
