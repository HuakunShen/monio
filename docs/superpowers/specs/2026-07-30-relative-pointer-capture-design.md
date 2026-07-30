# CrossFlow Relative Pointer Capture Design

## Goal

Give CrossFlow one process-wide, platform-neutral lease that marks the interval
in which local pointer motion is routed to another machine. The existing
`Hook::grab_async()` continues to capture and suppress events; the new lease
only owns cursor coupling, visibility, and restoration.

The first production implementation targets macOS. Linux keeps the same API as
a no-op because its existing X11/evdev grab paths already own relative capture.
Windows returns `Error::NotSupported` until its Raw Input and cursor-ownership
design is implemented.

## Public API

```rust
#[must_use]
pub struct RelativePointerCapture { /* private */ }

impl RelativePointerCapture {
    pub fn acquire() -> Result<Self>;
    pub fn release(self) -> Result<()>;
    pub fn is_active(&self) -> bool;
}
```

The type is re-exported from the crate root. Only one lease may be active in a
process. A second `acquire()` returns
`Error::RelativePointerCaptureAlreadyActive`.

`release(self)` is explicit and fallible. `Drop` performs the same restoration
best-effort, logs any error, and never panics. If an explicit release fails,
the value's subsequent drop retries unfinished restoration steps once.

Hook and channel-handle shutdown also call the process-wide release operation
as a safety net. A guard that outlives hook shutdown observes `is_active() ==
false`; releasing or dropping it again is harmless.

## CrossFlow Usage

CrossFlow starts one grab hook and consumes local keyboard/pointer events only
while its route is remote:

```rust
let hook = Hook::new();
hook.grab_async(route_event)?;

let capture = RelativePointerCapture::acquire()?;
// Route MouseData::relative to the active remote target.
capture.release()?;
```

Edge detection can therefore run before capture acquisition. Switching back to
the local machine releases the lease without restarting the event tap.

The lease does not create a hook and does not suppress events by itself.
CrossFlow must keep using `grab()`/`grab_async()` and return `None` for events
owned by the remote route.

## macOS Lifecycle

Acquisition:

1. Reject a concurrent process lease.
2. Read and save the global cursor position.
3. Resolve the display containing that point, falling back to the main display.
4. Hide the cursor on that display.
5. Call `CGAssociateMouseAndMouseCursorPosition(false)`.
6. Return the armed lease.

While active, the visible cursor stays fixed and hidden, but the existing
`CGEventTap` continues publishing `MouseEventDeltaX/Y` through
`MouseData::relative`.

Restoration:

1. Call `CGAssociateMouseAndMouseCursorPosition(true)`.
2. Warp the cursor to the saved global position.
3. Show the cursor on the saved display.
4. Clear the process lease only after all completed operations succeed.

Restoration records which steps succeeded. A retry does not repeat a successful
show call or otherwise unbalance Core Graphics' cursor hide/show counter.
Normal release, explicit `Hook::stop()`, error unwinding, and owner drop all
reach the same restoration operation through the lease's RAII semantics.
Process abort and forced termination remain operating-system recovery cases.

Blocking hook exit and backend startup/runtime errors run capture cleanup before
returning. Background hook threads run cleanup before clearing their running
flag. Channel-based hook shutdown uses the same helper.

If acquisition fails after partially changing cursor state, it immediately
attempts the same ordered restoration before returning the original error.

## Platform Contract

- macOS: full implementation described above.
- Linux X11 and evdev: acquisition/release are currently no-ops. The caller
  still needs an active Monio grab. Existing relative capture remains the
  backend guarantee.
- Windows: acquisition returns `Error::NotSupported`. The API surface remains
  source-compatible with a future Raw Input plus low-level-hook implementation.

## Error Handling

Core Graphics return codes are converted to `Error::Platform` with the failed
operation and numeric `CGError`. Restoration attempts every unfinished step
even if an earlier step fails and reports the first failure.

A failed explicit release is not silently converted to success. Drop retries
and logs because Rust destructors cannot return errors.

## Tests

1. Public lifecycle state: acquire, reject a second lease, explicit release,
   then reacquire.
2. Drop lifecycle: dropping a lease clears process ownership and permits a new
   lease.
3. Hook/channel shutdown safety net releases an active process lease and is
   idempotent when the owner guard later drops.
4. macOS restoration state machine: successful ordered restoration and retry
   after a partial failure without repeating completed steps.
5. macOS real Core Graphics smoke: acquire/release in a short test with an RAII
   cleanup guard.
6. Existing relative capture, injection, provenance, all-feature, doctest,
   formatting, and Clippy suites remain green.
7. Linux evdev and Windows targets retain compile-compatible public API.

## Out of Scope

- Windows Raw Input implementation.
- macOS IOHIDManager or IOHIDEventSystemClient.
- Suggested return positions supplied by a remote topology.
- Multiple simultaneous capture owners or nesting.
- Automatically starting or stopping a hook.
