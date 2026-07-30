# HarmonyOS PC input backend

Status: implementation compile-checked; native linking and PC verification pending.

Last updated: 2026-07-30

Target: HarmonyOS PC/2in1, API 26.0.0 or newer.

This document records the evidence, design decisions, compile strategy, known
limitations, and native acceptance work for adding a HarmonyOS PC backend to
Monio. Compile checks alone must not be described as native verification.

## Decision summary

- Support HarmonyOS PC/2in1 only. Phones, wearables, TVs, and older HarmonyOS
  API levels are outside the first implementation.
- Set the product/runtime floor to API 26.0.0.
- Use `ohos.permission.CONTROL_DEVICE` for direct key and mouse injection.
  Do not implement the older `OH_Input_RequestInjection` popup authorization
  flow.
- Use `ohos-input-sys` 0.3.4 as the raw FFI dependency with its `api-23`
  feature. The required C functions were introduced by API 21 or earlier;
  API 26 changes the permission path rather than replacing those functions.
- Use `ohos-sys-opaque-types` 0.1.10 directly for the callback event pointer
  types referenced by `ohos-input-sys` function signatures.
- Implement global keyboard grab with `OH_Input_AddKeyEventHook`.
- Keep pointer events observe-only in the generic `grab()` API. The public
  Input Kit mouse interceptor has no equivalent of
  `OH_Input_DispatchToNextHandler`, so it cannot safely implement Monio's
  per-event consume-or-pass-through contract.
- Report captured input as `InputOrigin::Unknown`. Current public Input Kit
  callbacks do not provide evidence strong enough to identify input injected
  by this Monio process.
- Put HarmonyOS behind `target_env = "ohos"`, not `target_os = "linux"`.
- Treat display topology and system input settings as unsupported in the first
  backend. Pointer position remains supported through Input Kit.

## Evidence from current documentation

Context7 resolved Huawei's official HarmonyOS API reference as:

```text
/websites/developer_huawei_consumer_cn_doc_harmonyos-references
```

The relevant native header is:

```c
#include <multimodalinput/oh_input_manager.h>
```

It is provided by `libohinput.so`.

Primary API reference:

- <https://developer.huawei.com/consumer/cn/doc/harmonyos-references/capi-oh-input-manager-h>

### Monitoring

The API reference exposes key, mouse, and axis monitors:

```c
OH_Input_AddKeyEventMonitor(...)
OH_Input_AddMouseEventMonitor(...)
OH_Input_AddAxisEventMonitorForAll(...)
```

These APIs require `ohos.permission.INPUT_MONITORING`. The mouse monitor
documentation states that it is effective during screen recording. Desktop
sharing/remote desktop is an intended permission scenario, but behavior
outside a qualifying application and recording/sharing lifecycle requires
native verification.

Monitor callbacks observe events; they do not consume them.

### Keyboard hook and pass-through

API 21 introduced:

```c
OH_Input_AddKeyEventHook(...)
OH_Input_GetKeyEventId(...)
OH_Input_DispatchToNextHandler(...)
OH_Input_RemoveKeyEventHook(...)
```

The hook requires `ohos.permission.HOOK_KEY_EVENT`, presented to users as the
Keyboard Input Assistance permission. The user enables it in system settings.

The hook receives keyboard events before the next handler. Omitting
`OH_Input_DispatchToNextHandler` consumes an event. Dispatching its event ID
passes the original event onward. Dispatch must preserve event order and key
pairing and must occur within three seconds.

HarmonyOS dispatches the original event identified by its event ID. Monio's
`Some(modified_event)` form therefore cannot rewrite a passed key event on
this backend. Only the `Some` versus `None` decision is honored.

### Mouse interception limitation

Input Kit exposes:

```c
OH_Input_AddInputEventInterceptor(...)
OH_Input_RemoveInputEventInterceptor(...)
```

The interceptor covers mouse, touch, and axis callbacks when input hits the
application window and requires `ohos.permission.INTERCEPT_INPUT_EVENT`.
Its callbacks return `void`, and the documented public API has no pointer
equivalent of `OH_Input_DispatchToNextHandler`.

Consequences:

- intercepted pointer input can be consumed;
- an individual intercepted pointer event cannot be safely passed through;
- removing the interceptor, injecting a replacement, and re-registering it
  would introduce races and event loss;
- re-injecting while the interceptor remains installed risks recapture and
  feedback.

The first backend therefore does not install the pointer interceptor from
generic `grab()`. It sends pointer monitor events to `GrabHandler`, but the
handler's return value cannot suppress pointer delivery. A future
all-or-nothing remote capture session may use the interceptor as a dedicated
consume-only capability after native testing. It must not be presented as
generic per-event grab.

### Direct injection

The backend uses:

```c
OH_Input_InjectKeyEvent(...)
OH_Input_InjectMouseEventGlobal(...)
OH_Input_GetPointerLocation(...)
```

Starting with API 26.0.0, callers holding
`ohos.permission.CONTROL_DEVICE` can invoke the existing injection APIs
directly without first completing `OH_Input_RequestInjection`.

`CONTROL_DEVICE` does not add new event types, improve monitoring, or add
pointer grab. Its benefit is a direct and synchronous authorization path for
the existing injection functions. The permission is restricted to PC/2in1
devices and is managed through system settings.

Permission reference:

- <https://gitee.com/openharmony/docs/blob/master/zh-cn/application-dev/security/AccessToken/restricted-permissions.md>

The application embedding Monio is responsible for declaring and obtaining:

```text
ohos.permission.INPUT_MONITORING
ohos.permission.HOOK_KEY_EVENT
ohos.permission.CONTROL_DEVICE
```

The first implementation does not require
`ohos.permission.INTERCEPT_INPUT_EVENT`, because it does not install the
pointer interceptor.

## Rust and SDK host support

Rust provides these OpenHarmony targets:

```text
aarch64-unknown-linux-ohos
armv7-unknown-linux-ohos
x86_64-unknown-linux-ohos
```

The first backend verifies the ARM64 target:

```text
aarch64-unknown-linux-ohos
```

Rust target reference:

- <https://doc.rust-lang.org/rustc/platform-support/openharmony.html>

On the current Linux development host, the ARM64 target is installed and
reports:

```text
target_arch="aarch64"
target_env="ohos"
target_os="linux"
target_vendor="unknown"
```

The OpenHarmony Rust documentation supports Linux-hosted cross-compilation
with an OpenHarmony Native SDK, clang, and sysroot. Huawei's current DevEco
Studio system requirements list Windows and macOS, not Linux:

- <https://developer.huawei.com/consumer/cn/deveco-studio>
- <https://developer.huawei.com/consumer/cn/deveco-studio/resources/>

The practical split is:

- Rust source development and `cargo check` can run on Linux.
- A Linux-hosted OpenHarmony Native SDK can provide clang and a sysroot for
  lower-level OpenHarmony cross-compilation.
- The officially supported Huawei HarmonyOS IDE, application packaging,
  signing, emulator, and latest commercial SDK workflow runs on Windows or
  macOS.
- Host toolchain binaries are host-specific. A macOS clang binary cannot be
  copied to Linux and executed there.

The current Linux host does not have a HarmonyOS/OpenHarmony Native SDK,
`oh_input_manager.h`, `libohinput.so`, or an OHOS linker installed. The first
implementation can therefore perform Rust target checking but cannot claim a
linked HarmonyOS executable or HAP.

## Why current Monio misclassifies OHOS

Rust reports OHOS as:

```text
target_os = "linux"
target_env = "ohos"
```

The existing platform routing uses only `target_os = "linux"`, so an OHOS
build selects the Linux X11/evdev backend. The default `x11` feature is not a
valid HarmonyOS application backend, and an ordinary HarmonyOS application
does not have direct `/dev/input` access for the evdev backend.

The routing must become:

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

Linux dependencies must use the same exclusion:

```toml
[target.'cfg(all(target_os = "linux", not(target_env = "ohos")))'.dependencies]
```

The OHOS dependency is target-specific:

```toml
[target.'cfg(target_env = "ohos")'.dependencies]
ohos-input-sys = { version = "0.3.4", features = ["api-23"] }
ohos-sys-opaque-types = "0.1.10"
```

`ohos-input-sys` links the native library as `ohinput`. Rust metadata checking
does not prove that a final application can resolve `libohinput.so`.

## Backend structure

The implementation is isolated under:

```text
src/platform/ohos/
├── mod.rs
├── constants.rs
├── lifecycle.rs
├── result.rs
├── listen.rs
├── translate.rs
├── keycodes.rs
├── simulate.rs
├── display.rs
└── test_module.rs
```

Responsibilities:

- `mod.rs`: expose the platform contract expected by `Hook`, channels,
  recorder, statistics, display helpers, and simulation helpers.
- `constants.rs`: keep documented native integer values available to the
  host-tested translation code without importing the native binding.
- `lifecycle.rs`: implement transactional registration, reverse-order
  rollback/cleanup, and the keyboard fail-open dispatch policy.
- `result.rs`: classify documented Input Kit result codes and map them to
  operation-specific Monio errors.
- `listen.rs`: own monitor/hook registration, callback entry points, active
  session state, error propagation, stop signaling, and cleanup.
- `translate.rs`: convert primitive OHOS action, key, button, coordinate, and
  axis values to owned Monio events. Keep conversion independent of native
  pointer lifetimes so it can be unit tested on the Linux host.
- `keycodes.rs`: map HarmonyOS key codes to and from `monio::Key`.
- `simulate.rs`: create, configure, inject, and destroy native key/mouse
  objects. All native objects use scoped cleanup.
- `display.rs`: implement pointer position through Input Kit and return
  `Error::NotSupported` for display topology and system settings.
- `test_module.rs`: compile the pure OHOS modules into Linux-hosted unit tests
  without linking `libohinput.so`.

Unsafe code stays inside the OHOS platform module. Public callers do not
receive native pointers, event IDs, or HarmonyOS-specific types.

## Listen data flow

`run_hook()` registers:

```text
key monitor
mouse monitor
all-axis monitor
```

Registration is transactional:

```text
install handler
  -> add key monitor
  -> add mouse monitor
  -> add axis monitor
  -> emit HookEnabled only after all registrations succeed
```

If any step fails, every earlier successful registration is removed before
returning an error.

Native event objects are valid only during their callback. Each callback:

1. validates its pointer;
2. reads primitive fields while the object is alive;
3. converts them into an owned `Event`;
4. updates modifier/button state;
5. drops the session lock before calling user code.

Mouse moves use the repository's global state rule:

```text
mouse move
  -> state::is_button_held()
  -> MouseDragged or MouseMoved
```

Axis scroll events become `MouseWheel`. Unsupported action or axis values are
ignored rather than guessed.

## Grab data flow

`run_grab_hook()` registers:

```text
global key hook
mouse monitor
all-axis monitor
```

Keyboard handling:

```text
native key event
  -> copy event ID and primitive key fields
  -> convert to Monio Event
  -> GrabHandler
     -> None: do not dispatch; consume
     -> Some(_): dispatch original event ID to next handler
```

Pointer handling:

```text
native mouse/axis monitor event
  -> convert to Monio Event
  -> GrabHandler for observation
  -> ignore return value because monitor events cannot be consumed
```

This asymmetry is a documented backend capability, not evidence of complete
global pointer grab.

## Session and callback safety

Input Kit callbacks do not carry a caller-defined context pointer. The backend
therefore supports one active process session and stores its handler behind
global synchronized state.

The state stores an `Arc` to the active handler. A callback clones the `Arc`
while holding the lock, releases the lock, and only then calls user code.
This prevents a handler that calls `Hook::stop()` from deadlocking on the
session lock.

User code must not unwind through the C ABI:

- listen callback panic: catch, log, and drop that callback;
- keyboard grab callback panic: catch and fail open by attempting to dispatch
  the original key event;
- poisoned/missing handler state in keyboard grab: fail open for the same
  reason.

Errors that occur inside a C callback cannot be returned directly. The
callback stores the first background error in session state. The blocking
loop observes it, stops, unregisters all active callbacks, clears the handler,
resets the global input mask, and returns the error.

Normal stop and partial startup failure use the same idempotent cleanup path.
`HookEnabled` is emitted only after startup succeeds. `HookDisabled` is
emitted only for a session that reached the enabled state.

## Simulation behavior

Supported events:

- key press and release;
- key tap;
- mouse button press and release;
- mouse click;
- absolute global mouse movement.

Unsupported or invalid Monio events return `Error::NotSupported` or
`Error::SimulateFailed`; they are not silently converted to another key or
button.

Native key/mouse objects are destroyed on both success and error paths.
Modifier press events must be paired with timely release events; convenience
functions preserve that pairing.

`simulate()` remains synchronous. A missing `CONTROL_DEVICE` permission maps
to `Error::PermissionDenied`.

## Error mapping

The backend retains the operation name and numeric native result in error
messages.

| Native outcome | Monio error |
| --- | --- |
| permission denied | `Error::PermissionDenied` |
| device/API not supported | `Error::NotSupported` |
| monitor/hook registration failure | `Error::HookStartFailed` |
| injection failure | `Error::SimulateFailed` |
| input service exception during registration | `Error::HookStartFailed` with native code |
| input service exception during injection | `Error::SimulateFailed` with native code |
| input service exception during pointer query | `Error::Platform` with native code |
| null callback event pointer | log and drop that callback without dereferencing |

Permission errors identify the relevant permission:

```text
listen: INPUT_MONITORING
keyboard grab: HOOK_KEY_EVENT
simulation: CONTROL_DEVICE
```

## Input provenance

All captured OHOS events remain:

```rust
InputOrigin::Unknown
```

`Unknown` means only that the backend lacks stronger evidence. It does not
mean physical, human-generated, trusted, non-injected, or safe to retransmit.

Input Kit's public callbacks do not expose a Monio-controlled tag or an exact
injector identity. Timing, matching coordinates, matching key codes, and
authorization ownership are insufficient evidence.

Consequently, the first OHOS backend does not satisfy Monio's
`ThisMonioSession` feedback-loop guarantee. A remote input product must not
enable simultaneous OHOS capture and retransmission on the assumption that
injected events can be filtered. Native research must establish a
platform-backed identity before adding self-injection classification.

## Automated verification

Pure Rust unit tests run on the Linux host for:

- representative key-code mappings in both directions;
- unknown key preservation;
- key press/release/cancel conversion;
- mouse button press/release conversion;
- move versus drag selection;
- vertical and horizontal wheel conversion;
- modifier and button mask transitions;
- native error-code classification.

OHOS-specific mapping files remain independent of `libohinput.so` so their
unit tests can run on the host. Native FFI signatures are checked by compiling
the OHOS target.

Required cross-target checks:

```bash
cargo check --target aarch64-unknown-linux-ohos
cargo check --target aarch64-unknown-linux-ohos --all-features
cargo clippy --target aarch64-unknown-linux-ohos --all-features -- -D warnings
```

Required host regressions:

```bash
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo fmt --all -- --check
cargo doc --all-features --no-deps
```

Implementation code is recorded in these ordered feature commits:

```text
8485c0c feat(ohos): add target routing
d57f684 feat(ohos): add event translation core
fa5f627 feat(ohos): add transactional registration
f1dd252 feat(ohos): add input simulation
e8fee52 feat(ohos): add input monitoring and key grab
```

The initial red target check failed because `x11` was selected for OHOS and
its build script attempted cross-platform `pkg-config`. The routing commit
excluded `target_env = "ohos"` from ordinary Linux dependencies; subsequent
OHOS checks select `ohos-input-sys` and no longer build X11.

The final result below was recorded against the documentation-complete tree.

Final verification on 2026-07-30:

```text
cargo test --all-features
  PASS: 29 unit tests
  PASS: 12 doc tests; 3 doc tests ignored
cargo clippy --all-features --all-targets -- -D warnings
  PASS
cargo fmt --all -- --check
  PASS
cargo doc --all-features --no-deps
  PASS
cargo check --target aarch64-unknown-linux-ohos
  PASS
cargo check --target aarch64-unknown-linux-ohos --all-features
  PASS
cargo clippy --target aarch64-unknown-linux-ohos --all-features -- -D warnings
  PASS
```

These commands compile Rust metadata and native FFI call signatures. They do
not invoke an OHOS linker, resolve `libohinput.so`, build a HAP, exercise the
permissions, or run a callback on a HarmonyOS PC.

## Native PC acceptance matrix

No item in this section is verified by cross-compilation. Before claiming
native HarmonyOS support, run the matrix on an API 26.0.0+ PC/2in1 with a
signed application and record the device, OS build, SDK, permissions, commands,
and observed output.

### Permissions and lifecycle

1. Start without `INPUT_MONITORING`; verify an explicit permission error.
2. Start without `HOOK_KEY_EVENT`; verify grab fails open and cleans up.
3. Inject without `CONTROL_DEVICE`; verify an explicit permission error.
4. Enable each permission through the supported settings flow and retry.
5. Stop and restart listen/grab repeatedly.
6. Kill the process while a key hook is active and verify normal input resumes.
7. Revoke a permission while active and verify cleanup.

### Listening

1. Verify key press/release, modifiers, autorepeat, and media keys.
2. Verify mouse buttons, absolute coordinates, movement, drag, and wheel axes.
3. Verify multi-display coordinates.
4. Verify the documented screen-recording/desktop-sharing requirement for
   mouse monitoring.
5. Verify input while the application is focused, unfocused, minimized, and
   backgrounded.

### Keyboard grab

1. Consume and pass representative key pairs.
2. Verify pass-through ordering and the three-second dispatch boundary.
3. Verify down/up/cancel pairing.
4. Verify handler panic and internal callback failure fail open.
5. Verify `Some(modified_event)` passes the original event unchanged.
6. Verify another key hook owner produces a clear conflict error.

### Pointer grab limitation

1. Confirm mouse events delivered through `grab()` remain observable.
2. Confirm returning `None` does not claim pointer suppression.
3. Separately prototype the window interceptor only for a future consume-all
   capture session.
4. Do not add interceptor pass-through by remove/inject/re-register unless a
   native stress test proves ordering and loss behavior.

### Injection

1. Inject every supported key category and verify press/release balance.
2. Inject mouse movement on each display.
3. Inject every supported mouse button and verify that `MouseWheel` simulation
   returns `Error::NotSupported` in this implementation.
4. Verify missing permission and service errors.
5. Determine whether injected events are recaptured by monitors and hooks.
6. Establish whether Input Kit exposes any evidence suitable for exact
   `ThisMonioSession` classification; leave events `Unknown` if it does not.

## Completion language

Use these status terms precisely:

- **Designed:** interfaces and constraints recorded; no implementation claim.
- **Compile-checked:** Rust target checks pass; no native runtime claim.
- **Linked:** an OHOS executable or application native library resolves
  `libohinput.so` with a real SDK.
- **Natively verified:** the signed application passes the acceptance matrix
  on a named HarmonyOS PC/2in1 build.

The backend is not complete merely because `cargo check` succeeds.
