# HarmonyOS PC Input Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a compile-checked Monio backend for HarmonyOS PC/2in1 that monitors keyboard, mouse, and wheel input, selectively grabs keyboard input, injects keys and absolute mouse input, and queries pointer position.

**Architecture:** Route Rust targets with `target_env = "ohos"` to an isolated `src/platform/ohos` backend and bind Huawei Input Kit through `ohos-input-sys` 0.3.4. Keep native pointers and unsafe calls in callback/simulation adapters, while key mapping, event translation, native-result classification, and registration rollback remain pure Rust and host-testable.

**Tech Stack:** Rust 2024, `ohos-input-sys` 0.3.4 with `api-23`, HarmonyOS Input Kit C API, `aarch64-unknown-linux-ohos`, Cargo unit tests and target checks.

## Global Constraints

- Support HarmonyOS PC/2in1 only; phones, wearables, TVs, and older API levels are out of scope.
- Runtime and product minimum is HarmonyOS API 26.0.0.
- Use `ohos.permission.CONTROL_DEVICE` and direct injection; do not call `OH_Input_RequestInjection`.
- Use `ohos.permission.INPUT_MONITORING` for monitor mode and `ohos.permission.HOOK_KEY_EVENT` for keyboard grab.
- Do not request `ohos.permission.INTERCEPT_INPUT_EVENT` in this backend.
- `grab()` may consume keyboard events, but mouse and wheel events delivered to `GrabHandler` are observe-only and ignore its return value.
- `Some(modified_event)` from a keyboard `GrabHandler` dispatches the original HarmonyOS event unchanged.
- Every captured event has `InputOrigin::Unknown`; never infer physical or self-injected origin.
- Do not claim linked or native support from `cargo check`; the Linux host has no HarmonyOS Native SDK, `libohinput.so`, HAP packaging, signing, or PC runtime.
- Native event pointers are callback-scoped and must never escape `src/platform/ohos`.
- Native callback panics must not cross the C ABI; keyboard-hook failure paths dispatch the original event when its ID is available.
- Registration and cleanup are transactional and idempotent.

---

## File Structure

Create and modify these focused units:

```text
Cargo.toml                         target-specific dependency routing
Cargo.lock                         locked ohos-input-sys dependency
src/platform/mod.rs                OHOS versus ordinary Linux dispatch
src/platform/ohos/mod.rs           platform-contract exports
src/platform/ohos/constants.rs     native integer constants used by pure code
src/platform/ohos/keycodes.rs      bidirectional Monio/OHOS key mapping
src/platform/ohos/translate.rs     primitive native fields to owned Event
src/platform/ohos/result.rs        Input Kit result classification/mapping
src/platform/ohos/lifecycle.rs     registration transaction and rollback
src/platform/ohos/listen.rs        callbacks, session, blocking loop, cleanup
src/platform/ohos/simulate.rs      scoped native event allocation/injection
src/platform/ohos/display.rs       pointer query and explicit unsupported APIs
src/platform/ohos/test_module.rs   host compilation of pure OHOS units
src/hook.rs                        public grab capability documentation
src/lib.rs                         public platform support documentation
README.md                          platform matrix and permissions
docs/harmonyos-pc-input-backend.md implementation and verification status
```

The host-only test module includes `constants.rs`, `keycodes.rs`, `translate.rs`,
`result.rs`, and `lifecycle.rs` by path. This prevents Linux unit tests from
linking `libohinput.so`.

### Task 1: Route OHOS Away From Linux/X11

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/platform/mod.rs`
- Create: `src/platform/ohos/mod.rs`
- Create: `src/platform/ohos/display.rs`
- Create: `src/platform/ohos/listen.rs`
- Create: `src/platform/ohos/simulate.rs`

**Interfaces:**
- Consumes: the platform contract called from `hook.rs`, `simulate.rs`, and `display.rs`
- Produces: OHOS definitions of `run_hook`, `run_grab_hook`, `stop_hook`, `simulate`, key/mouse convenience functions, `mouse_position`, and display/system query functions

- [x] **Step 1: Preserve the current target-check failure as the red test**

Run:

```bash
cargo check --target aarch64-unknown-linux-ohos
```

Expected: FAIL in the `x11` build script because the current
`cfg(target_os = "linux")` dependency block treats OHOS as desktop Linux.

- [x] **Step 2: Narrow ordinary Linux and add the OHOS dependency**

Use these target dependency sections in `Cargo.toml`:

```toml
[target.'cfg(all(target_os = "linux", not(target_env = "ohos")))'.dependencies]
x11 = { version = "2.21", features = ["xlib", "xrecord", "xtst"], optional = true }
evdev = { version = "0.12", optional = true }
libc = { version = "0.2", optional = true }

[target.'cfg(target_env = "ohos")'.dependencies]
ohos-input-sys = { version = "0.3.4", features = ["api-23"] }
```

Route the platform in `src/platform/mod.rs`:

```rust
#[cfg(target_env = "ohos")]
mod ohos;
#[cfg(target_env = "ohos")]
pub use ohos::*;

#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
mod linux;
#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
pub use linux::*;
```

Keep OHOS accepted by the final `compile_error!` guard.

- [x] **Step 3: Add a complete compile-only platform scaffold**

In `src/platform/ohos/mod.rs`, declare `display`, `listen`, and `simulate`, then
re-export their platform-contract functions. In the three modules, use the
exact public signatures below and temporarily return explicit
`Error::NotSupported("HarmonyOS <capability> is not implemented".into())`:

```rust
pub fn run_hook<H: EventHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()>;
pub fn run_grab_hook<H: GrabHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()>;
pub fn stop_hook() -> Result<()>;

pub fn simulate(event: &Event) -> Result<()>;
pub fn key_press(key: Key) -> Result<()>;
pub fn key_release(key: Key) -> Result<()>;
pub fn key_tap(key: Key) -> Result<()>;
pub fn mouse_press(button: Button) -> Result<()>;
pub fn mouse_release(button: Button) -> Result<()>;
pub fn mouse_click(button: Button) -> Result<()>;
pub fn mouse_move(x: f64, y: f64) -> Result<()>;

pub fn mouse_position() -> Result<(f64, f64)>;
pub fn displays() -> Result<Vec<DisplayInfo>>;
pub fn primary_display() -> Result<DisplayInfo>;
pub fn display_at_point(x: f64, y: f64) -> Result<Option<DisplayInfo>>;
pub fn system_settings() -> Result<SystemSettings>;
```

- [x] **Step 4: Verify target selection and host regression**

Run:

```bash
cargo check --target aarch64-unknown-linux-ohos
cargo check
cargo test
```

Expected: all commands PASS; the OHOS check does not build `x11`.

- [x] **Step 5: Commit the routing**

```bash
git add Cargo.toml Cargo.lock src/platform/mod.rs src/platform/ohos
git commit -m "feat(ohos): add target routing"
```

### Task 2: Add Host-Tested Key and Event Translation

**Files:**
- Create: `src/platform/ohos/constants.rs`
- Create: `src/platform/ohos/keycodes.rs`
- Create: `src/platform/ohos/translate.rs`
- Create: `src/platform/ohos/test_module.rs`
- Modify: `src/platform/ohos/mod.rs`
- Modify: `src/platform/mod.rs`

**Interfaces:**
- Consumes: `Key`, `Button`, `Event`, `ScrollDirection`, and `crate::state`
- Produces:

```rust
pub(crate) fn keycode_to_key(code: i32) -> Key;
pub(crate) fn key_to_keycode(key: Key) -> Option<i32>;
pub(crate) fn button_from_native(button: i32) -> Button;
pub(crate) fn button_to_native(button: Button) -> Option<i32>;
pub(crate) fn translate_key(action: i32, keycode: i32) -> Option<Event>;
pub(crate) fn translate_mouse(
    action: i32,
    button: i32,
    x: f64,
    y: f64,
) -> Option<Event>;
pub(crate) fn translate_axis(
    axis_type: u32,
    x: f64,
    y: f64,
    value: f64,
) -> Option<Event>;
```

- [x] **Step 1: Add failing key-map tests**

Under `#[cfg(test)]` in `keycodes.rs`, add table-driven assertions for:

```rust
let pairs = [
    (2017, Key::KeyA),
    (2042, Key::KeyZ),
    (2000, Key::Num0),
    (2009, Key::Num9),
    (2090, Key::F1),
    (2101, Key::F12),
    (2047, Key::ShiftLeft),
    (2073, Key::ControlRight),
    (2070, Key::Escape),
    (2081, Key::Home),
    (2056, Key::Grave),
    (2116, Key::NumpadAdd),
    (16, Key::VolumeUp),
    (10, Key::MediaPlayPause),
    (2084, Key::BrowserForward),
    (2053, Key::LaunchMail),
    (2067, Key::ContextMenu),
];
```

Assert both directions for every pair, assert `keycode_to_key(99_999)` is
`Key::Unknown(99_999)`, assert `key_to_keycode(Key::Unknown(99_999))` returns
`Some(99_999)`, and assert unsupported `F13`, `IntlYen`, and `LaunchApp1`
return `None`.

- [x] **Step 2: Add failing event-translation tests**

Test all exact rules:

```rust
translate_key(1, 2017) -> KeyPressed(KeyA, raw 2017)
translate_key(2, 2017) -> KeyReleased(KeyA, raw 2017)
translate_key(0, 2017) -> KeyReleased(KeyA, raw 2017)
translate_key(99, 2017) -> None

translate_mouse(2, 0, 10.0, 20.0) -> MousePressed(Left)
translate_mouse(3, 4, 10.0, 20.0) -> MouseReleased(Button4)
translate_mouse(1, -1, 10.0, 20.0) -> MouseMoved when no button is held
translate_mouse(1, -1, 10.0, 20.0) -> MouseDragged when Left is held
translate_mouse(99, 0, 10.0, 20.0) -> None

translate_axis(1, 10.0, 20.0, 2.5) -> MouseWheel(Up, 2.5)
translate_axis(1, 10.0, 20.0, -2.5) -> MouseWheel(Down, 2.5)
translate_axis(2, 10.0, 20.0, 3.0) -> MouseWheel(Right, 3.0)
translate_axis(2, 10.0, 20.0, -3.0) -> MouseWheel(Left, 3.0)
translate_axis(0, 10.0, 20.0, 3.0) -> None
translate_axis(3, 10.0, 20.0, 3.0) -> None
```

Assert modifier/button masks are set before press-event construction and
cleared before release/cancel-event construction. Assert every event origin is
`InputOrigin::Unknown`.

- [x] **Step 3: Expose pure OHOS modules to host tests and verify red**

At the bottom of `src/platform/mod.rs`, add:

```rust
#[cfg(all(test, not(target_env = "ohos")))]
#[path = "ohos/test_module.rs"]
mod ohos_test;
```

In `test_module.rs`, include the pure files:

```rust
#[path = "constants.rs"]
mod constants;
#[path = "keycodes.rs"]
mod keycodes;
#[path = "translate.rs"]
mod translate;
```

Run:

```bash
cargo test platform::ohos_test
```

Expected: FAIL because the mapping and translation functions are not defined.

- [x] **Step 4: Implement the mappings and translations**

Define named constants for the action, mouse-button, and axis integer values.
Implement the complete API-23 mapping for:

```text
A-Z; Num0-Num9; F1-F12; left/right Shift, Control, Alt, Meta;
Escape, Tab, CapsLock, Space, Enter, Backspace, Insert, Delete,
Home, End, PageUp, PageDown, ArrowUp, ArrowDown, ArrowLeft, ArrowRight;
NumLock, ScrollLock, PrintScreen, Pause;
Grave, Minus, Equal, BracketLeft, BracketRight, Backslash,
Semicolon, Quote, Comma, Period, Slash;
Numpad0-Numpad9, Add, Subtract, Multiply, Divide, Decimal, Enter, Equal;
VolumeUp, VolumeDown, VolumeMute, MediaPlayPause, MediaStop,
MediaNext, MediaPrevious; BrowserBack, BrowserForward, BrowserHome;
LaunchMail; ContextMenu.
```

Map native mouse buttons `0 -> Left`, `1 -> Middle`, `2 -> Right`,
`4 -> Button4`, and `3 -> Button5`. Preserve other non-negative native button
codes as `Button::Unknown(code as u8)` only when they fit in `u8`; use
`Button::Unknown(u8::MAX)` otherwise.

Update `crate::state` before constructing press/release events, classify move
using `state::is_button_held()`, use absolute wheel magnitudes, and ignore zero
or unsupported axis values.

- [x] **Step 5: Run pure tests and cross-check**

Run:

```bash
cargo test platform::ohos_test
cargo check --target aarch64-unknown-linux-ohos
```

Expected: PASS.

- [x] **Step 6: Commit the translation core**

```bash
git add src/platform/mod.rs src/platform/ohos
git commit -m "feat(ohos): add event translation core"
```

### Task 3: Add Native Result Mapping and Transactional Registration

**Files:**
- Create: `src/platform/ohos/result.rs`
- Create: `src/platform/ohos/lifecycle.rs`
- Modify: `src/platform/ohos/test_module.rs`
- Modify: `src/platform/ohos/mod.rs`

**Interfaces:**
- Consumes: raw nonzero Input Kit result codes and ordered registration calls
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureKind {
    PermissionDenied,
    Unsupported,
    InvalidParameter,
    Service,
    Conflict,
    Other(u32),
}

pub(crate) fn classify_code(code: u32) -> FailureKind;
pub(crate) fn hook_start_error(operation: &str, code: u32, permission: &str) -> Error;
pub(crate) fn hook_stop_error(operation: &str, code: u32) -> Error;
pub(crate) fn simulate_error(operation: &str, code: u32) -> Error;
pub(crate) fn platform_error(operation: &str, code: u32) -> Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Registrations {
    pub(crate) key_hook: bool,
    pub(crate) key_monitor: bool,
    pub(crate) mouse_monitor: bool,
    pub(crate) axis_monitor: bool,
}

pub(crate) trait RegistrationApi {
    fn add_key_hook(&mut self) -> std::result::Result<(), u32>;
    fn add_key_monitor(&mut self) -> std::result::Result<(), u32>;
    fn add_mouse_monitor(&mut self) -> std::result::Result<(), u32>;
    fn add_axis_monitor(&mut self) -> std::result::Result<(), u32>;
    fn remove_key_hook(&mut self) -> std::result::Result<(), u32>;
    fn remove_key_monitor(&mut self) -> std::result::Result<(), u32>;
    fn remove_mouse_monitor(&mut self) -> std::result::Result<(), u32>;
    fn remove_axis_monitor(&mut self) -> std::result::Result<(), u32>;
}

pub(crate) enum RegistrationMode {
    Listen,
    Grab,
}

pub(crate) fn register<A: RegistrationApi>(
    api: &mut A,
    mode: RegistrationMode,
) -> std::result::Result<Registrations, u32>;
pub(crate) fn unregister<A: RegistrationApi>(
    api: &mut A,
    registrations: &mut Registrations,
) -> std::result::Result<(), u32>;
```

- [x] **Step 1: Add failing native-result tests**

Assert this classification:

```rust
201 -> PermissionDenied
801 -> Unsupported
401 -> InvalidParameter
3_800_001 -> Service
4_200_001 -> Conflict
123_456 -> Other(123_456)
```

Assert `hook_start_error` preserves operation, permission, and numeric result;
permission becomes `Error::PermissionDenied`, unsupported becomes
`Error::NotSupported`, and other outcomes become `Error::HookStartFailed`.
Assert injection permission errors mention `CONTROL_DEVICE`.

- [x] **Step 2: Add failing transaction tests**

Implement a `FakeApi` that stores a `Vec<&'static str>` call log and can fail
one named add/remove operation. Test:

```text
Listen success: add-key-monitor, add-mouse-monitor, add-axis-monitor
Grab success: add-key-hook, add-mouse-monitor, add-axis-monitor
Listen mouse failure: add-key-monitor, add-mouse-monitor, remove-key-monitor
Grab axis failure: add-key-hook, add-mouse-monitor, add-axis-monitor,
                   remove-mouse-monitor, remove-key-hook
Cleanup order: remove-axis-monitor, remove-mouse-monitor, remove-key-hook
Repeated cleanup: no additional calls and success
Cleanup failure: continue removing remaining registrations, return first code
```

- [x] **Step 3: Verify the new tests fail**

Run:

```bash
cargo test platform::ohos_test
```

Expected: FAIL because `FailureKind`, error mapping, and registration lifecycle
are not defined.

- [x] **Step 4: Implement result mapping and lifecycle**

Use numeric codes from `ohos-input-sys` without importing that crate into pure
modules. Roll registration back in reverse order on every add failure.
`unregister` clears each flag before attempting its remove call, attempts every
registered removal, and returns only the first removal error.

- [x] **Step 5: Verify pure tests and OHOS type checking**

Run:

```bash
cargo test platform::ohos_test
cargo check --target aarch64-unknown-linux-ohos
```

Expected: PASS.

- [x] **Step 6: Commit the lifecycle**

```bash
git add src/platform/ohos
git commit -m "feat(ohos): add transactional registration"
```

### Task 4: Implement Simulation and Pointer Query

**Files:**
- Modify: `src/platform/ohos/simulate.rs`
- Modify: `src/platform/ohos/display.rs`
- Modify: `src/platform/ohos/translate.rs`

**Interfaces:**
- Consumes: `key_to_keycode`, `button_to_native`, Input Kit native constructors/setters/destructors/injection, and result mapping
- Produces: all simulation convenience functions and `mouse_position`

- [x] **Step 1: Add failing simulation-plan tests**

Add a pure specification type and function to `translate.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SimulationSpec {
    Key { action: i32, keycode: i32 },
    Mouse {
        action: i32,
        button: i32,
        x: i32,
        y: i32,
    },
}

pub(crate) fn simulation_spec(event: &Event) -> Result<SimulationSpec>;
```

Test key press/release, mouse press/release, and absolute move. Test that
wheel/click/dragged/typed/hook lifecycle events are rejected, unsupported
keys/buttons are rejected, non-finite coordinates are rejected, and values
outside the `i32` range are rejected.

- [x] **Step 2: Run the focused tests to verify red**

Run:

```bash
cargo test platform::ohos_test::translate::tests::simulation
```

Expected: FAIL because `SimulationSpec` and `simulation_spec` do not exist.

- [x] **Step 3: Implement pure simulation validation**

Return `Error::NotSupported` for unsupported event kinds/keys/buttons. Convert
finite in-range mouse coordinates with Rust's truncation toward zero. Return
`Error::SimulateFailed` for non-finite or out-of-range coordinates.

- [x] **Step 4: Implement scoped native injection**

In `simulate.rs`, add private `NativeKeyEvent` and `NativeMouseEvent` owners.
Their constructors reject null native pointers, and `Drop` calls the matching
destroy function through a mutable pointer:

```rust
struct NativeKeyEvent(*mut Input_KeyEvent);
struct NativeMouseEvent(*mut Input_MouseEvent);
```

For keys, call `OH_Input_SetKeyEventAction`,
`OH_Input_SetKeyEventKeyCode`, then `OH_Input_InjectKeyEvent`. For mouse,
call `OH_Input_SetMouseEventAction`, `OH_Input_SetMouseEventButton`,
`OH_Input_SetMouseEventGlobalX`, `OH_Input_SetMouseEventGlobalY`, then
`OH_Input_InjectMouseEventGlobal`. Map nonzero injection codes with
`simulate_error`, including `CONTROL_DEVICE` for code 201.

Implement convenience operations as exact event sequences:

```text
key_press: one KeyPressed event
key_release: one KeyReleased event
key_tap: key_press, then key_release
mouse_press: query current position, then MousePressed
mouse_release: query current position, then MouseReleased
mouse_click: mouse_press, then mouse_release
mouse_move: one MouseMoved event at absolute global coordinates
```

- [x] **Step 5: Implement pointer position and explicit unsupported queries**

Call:

```rust
OH_Input_GetPointerLocation(&mut display_id, &mut x, &mut y)
```

Return `(x, y)` on success and map errors with operation
`OH_Input_GetPointerLocation`. Keep `displays`, `primary_display`,
`display_at_point`, and `system_settings` as explicit `Error::NotSupported`
with “HarmonyOS Input Kit does not expose this query” messages.

- [x] **Step 6: Verify tests and cross-target compilation**

Run:

```bash
cargo test platform::ohos_test
cargo check --target aarch64-unknown-linux-ohos
cargo check --target aarch64-unknown-linux-ohos --all-features
```

Expected: PASS without linking `libohinput.so`.

- [x] **Step 7: Commit simulation and pointer position**

```bash
git add src/platform/ohos
git commit -m "feat(ohos): add input simulation"
```

### Task 5: Implement Monitor and Keyboard-Grab Sessions

**Files:**
- Modify: `src/platform/ohos/listen.rs`
- Modify: `src/platform/ohos/mod.rs`

**Interfaces:**
- Consumes: `translate_key`, `translate_mouse`, `translate_axis`,
  `RegistrationApi`, `register`, `unregister`, and native result mapping
- Produces: functional blocking `run_hook`, `run_grab_hook`, and `stop_hook`

- [ ] **Step 1: Add failing session-policy tests**

Keep callback-free policy host-testable in `lifecycle.rs`. Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandlerMode {
    Listen,
    Grab,
}

pub(crate) fn should_dispatch_original(
    mode: HandlerMode,
    handler_returned_some: bool,
    handler_panicked: bool,
    handler_available: bool,
) -> bool;
```

Assert:

```text
grab + Some -> dispatch
grab + None -> consume
grab + panic -> dispatch
grab + unavailable handler -> dispatch
listen -> never calls keyboard-hook dispatch logic
```

- [ ] **Step 2: Run the policy test to verify red**

Run:

```bash
cargo test platform::ohos_test::lifecycle::tests::dispatch
```

Expected: FAIL because `HandlerMode` and `should_dispatch_original` do not
exist.

- [ ] **Step 3: Implement session storage and callback adapters**

In `listen.rs`, create one global:

```rust
static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();
```

Use an internal erased handler enum:

```rust
enum ActiveHandler {
    Listen(Arc<dyn EventHandler>),
    Grab(Arc<dyn GrabHandler>),
}

struct Session {
    handler: ActiveHandler,
    background_error: Option<Error>,
    enabled: bool,
}
```

Callbacks validate the pointer, copy native primitive fields, translate to an
owned `Event`, clone the relevant `Arc` while locked, release the lock, and
invoke the handler with:

```rust
catch_unwind(AssertUnwindSafe(|| handler.handle_event(&event)))
```

The key hook obtains the event ID with `OH_Input_GetKeyEventId` before invoking
user code. Dispatch the original ID on `Some`, panic, missing handler, poisoned
session state, or translation failure. Store the first dispatch/get-ID error
when the session lock is usable. Do not dispatch on `None`.

Mouse and axis callbacks call `GrabHandler` for observation but discard its
return value.

- [ ] **Step 4: Implement the native registration adapter**

Implement `RegistrationApi` using these callback pairs:

```text
OH_Input_AddKeyEventMonitor / OH_Input_RemoveKeyEventMonitor
OH_Input_AddKeyEventHook / OH_Input_RemoveKeyEventHook
OH_Input_AddMouseEventMonitor / OH_Input_RemoveMouseEventMonitor
OH_Input_AddAxisEventMonitorForAll / OH_Input_RemoveAxisEventMonitorForAll
```

Convert `Input_Result` to `Result<(), u32>` by extracting the nonzero numeric
code. Use the same function pointer for add and remove.

- [ ] **Step 5: Implement the blocking lifecycle**

For listen, install the session, register key/mouse/axis monitors, then emit
`HookEnabled`. For grab, install the session, register key hook plus mouse/axis
monitors, then emit `HookEnabled`.

Poll `running` every 10 milliseconds. Exit on `false` or stored background
error. Cleanup in all paths:

```text
unregister in reverse order
emit HookDisabled only if HookEnabled was emitted
clear SESSION
reset global mask
return the first background/cleanup error
```

Reject a second active process session with `Error::AlreadyRunning`.
`stop_hook()` does not unregister on the caller thread; it returns `Ok(())`
because `Hook::stop` has already cleared the shared running flag and the
blocking session owns cleanup.

- [ ] **Step 6: Verify the backend compiles and host behavior remains intact**

Run:

```bash
cargo test platform::ohos_test
cargo check --target aarch64-unknown-linux-ohos
cargo check --target aarch64-unknown-linux-ohos --all-features
cargo test --all-features
```

Expected: PASS.

- [ ] **Step 7: Commit listen and grab**

```bash
git add src/platform/ohos
git commit -m "feat(ohos): add input monitoring and key grab"
```

### Task 6: Document Public Capabilities and Verify the Branch

**Files:**
- Modify: `src/hook.rs`
- Modify: `src/lib.rs`
- Modify: `README.md`
- Modify: `docs/harmonyos-pc-input-backend.md`
- Modify: `docs/superpowers/plans/2026-07-30-harmonyos-pc-input-backend.md`

**Interfaces:**
- Consumes: the completed backend behavior and fresh command output
- Produces: accurate public documentation and a checked implementation record

- [ ] **Step 1: Update platform-facing documentation**

Document these exact facts:

```text
HarmonyOS PC/2in1 API 26.0.0+
INPUT_MONITORING for listen
HOOK_KEY_EVENT for keyboard suppression/pass-through
CONTROL_DEVICE for direct key/mouse injection
keyboard grab only; pointer GrabHandler return values cannot consume
Some(modified_event) passes the original keyboard event
captured InputOrigin is Unknown
display topology/system settings unsupported; pointer position supported
Linux cargo check is not native linking or runtime verification
```

Do not describe HarmonyOS support as natively verified.

- [ ] **Step 2: Update the dedicated research document status**

Change its status from “design approved; implementation has not started” to
“implementation compile-checked”. Add the final commit/file structure,
dependency resolution, exact verification commands, pass/fail results, and the
remaining Native SDK/PC acceptance matrix. Record any warning or limitation
found during implementation.

- [ ] **Step 3: Run formatting and inspect the diff**

Run:

```bash
cargo fmt --all
git diff --check
git diff --stat
git status --short
```

Expected: no whitespace errors and only HarmonyOS implementation/doc files.

- [ ] **Step 4: Run the complete fresh verification set**

Run:

```bash
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo doc --all-features --no-deps
cargo check --target aarch64-unknown-linux-ohos
cargo check --target aarch64-unknown-linux-ohos --all-features
```

Expected: all commands PASS. Record that no executable/HAP was linked because
the Linux host lacks the HarmonyOS Native SDK and `libohinput.so`.

- [ ] **Step 5: Mark this plan complete and commit documentation**

Check every completed step in this file, then:

```bash
git add README.md src/hook.rs src/lib.rs docs
git commit -m "docs: document HarmonyOS PC backend"
```

- [ ] **Step 6: Review branch state**

Run:

```bash
git status --short --branch
git log --oneline --decorate -8
```

Expected: clean `feat/harmonyos-pc-input` worktree with small, ordered commits.
