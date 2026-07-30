# Relative Pointer Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a CrossFlow-ready process lease that safely disassociates and
restores the macOS cursor while preserving one cross-platform Rust API.

**Architecture:** A public `RelativePointerCapture` RAII guard owns one global
logical lease and delegates native setup/cleanup to small platform functions.
macOS stores restoration progress so failed cleanup can retry without
unbalancing cursor visibility. Hook and channel shutdown call the same global
release helper as a safety net.

**Tech Stack:** Rust 2024, `objc2-core-graphics`, Core Graphics remote-operation
and direct-display APIs, Cargo unit/doc tests.

## Global Constraints

- Do not change the existing `MouseData::relative` or
  `mouse_move_relative()` signatures.
- Only one relative-pointer capture lease may be active per process.
- Generic `grab()` behavior must remain unchanged when no lease is active.
- macOS restoration order is associate, warp, then show.
- Linux X11/evdev lease operations are no-ops; Windows acquisition returns
  `Error::NotSupported`.
- Windows Raw Input, macOS IOHID, and suggested remote return positions remain
  out of scope.

---

### Task 1: Public capture lease

**Files:**

- Create: `src/pointer_capture.rs`
- Modify: `src/error.rs`
- Modify: `src/lib.rs`
- Modify: `src/platform/mod.rs`
- Modify: `src/platform/linux/mod.rs`
- Modify: `src/platform/windows/mod.rs`
- Test: `src/pointer_capture.rs`

**Interfaces:**

- Consumes: `platform::begin_relative_pointer_capture() -> Result<()>` and
  `platform::end_relative_pointer_capture() -> Result<()>`.
- Produces: `RelativePointerCapture::acquire() -> Result<Self>`,
  `release(self) -> Result<()>`, `is_active(&self) -> bool`, and
  `pub(crate) fn release_active() -> Result<()>`.

- [x] **Step 1: Write the failing public lifecycle tests**

```rust
#[test]
fn second_process_lease_is_rejected() {
    let first = RelativePointerCapture::acquire().unwrap();
    assert!(matches!(
        RelativePointerCapture::acquire(),
        Err(Error::RelativePointerCaptureAlreadyActive)
    ));
    first.release().unwrap();
}

#[test]
fn dropping_owner_allows_reacquisition() {
    drop(RelativePointerCapture::acquire().unwrap());
    RelativePointerCapture::acquire().unwrap().release().unwrap();
}
```

The tests use the Linux no-op platform contract under a Linux target and a
short real Core Graphics acquire/release under macOS.

- [x] **Step 2: Run the focused tests and observe RED**

Run:

```bash
cargo test relative_pointer_capture --no-run
```

Expected: compilation fails because `RelativePointerCapture` and the new error
variant do not exist.

- [x] **Step 3: Implement the process lease**

Use an `AtomicBool` compare-exchange for ownership. On platform setup failure,
clear ownership. Explicit release clears ownership only after native cleanup
succeeds. Drop retries once, logs failure, then clears logical ownership so a
future acquire can ask the backend to repair stale native state.

- [x] **Step 4: Add platform stubs and verify GREEN**

Linux begin/end return `Ok(())`. Windows begin returns:

```rust
Err(Error::NotSupported(
    "relative pointer capture is not implemented on Windows; Raw Input support is required"
        .into(),
))
```

Windows end returns `Ok(())`. Run the focused tests on macOS after Task 2 adds
its backend; use cross-target `cargo check` meanwhile.

### Task 2: macOS cursor state and restoration

**Files:**

- Create: `src/platform/macos/pointer_capture.rs`
- Modify: `src/platform/macos/mod.rs`
- Test: `src/platform/macos/pointer_capture.rs`

**Interfaces:**

- Produces: `begin_relative_pointer_capture() -> Result<()>` and
  `end_relative_pointer_capture() -> Result<()>`.
- Uses: `CGDisplayHideCursor`, `CGAssociateMouseAndMouseCursorPosition`,
  `CGWarpMouseCursorPosition`, `CGDisplayShowCursor`,
  `CGGetDisplaysWithPoint`, and `CGMainDisplayID`.

- [x] **Step 1: Write RED tests for restoration progress**

Create a `CaptureState` with literal position/display data. Drive
`restore_with` using closures that fail association once, then succeed. Assert
that the first attempt still tries warp/show, the second attempt retries only
association, and the final state is complete.

```rust
#[test]
fn restoration_retries_only_unfinished_steps() {
    use std::cell::{Cell, RefCell};

    let mut state = CaptureState {
        saved_position: CGPoint { x: 120.0, y: 240.0 },
        display_id: 7,
        associated: false,
        position_restored: false,
        cursor_hidden: true,
    };
    let calls = RefCell::new(Vec::new());
    let fail_association = Cell::new(true);

    let first = restore_with(
        &mut state,
        || {
            calls.borrow_mut().push("associate");
            if fail_association.replace(false) {
                Err(Error::Platform("associate failed".into()))
            } else {
                Ok(())
            }
        },
        |_| {
            calls.borrow_mut().push("warp");
            Ok(())
        },
        |_| {
            calls.borrow_mut().push("show");
            Ok(())
        },
    );

    assert!(first.is_err());
    assert_eq!(&*calls.borrow(), &["associate", "warp", "show"]);
    assert!(!state.associated);
    assert!(state.position_restored);
    assert!(!state.cursor_hidden);

    restore_with(
        &mut state,
        || {
            calls.borrow_mut().push("associate");
            Ok(())
        },
        |_| panic!("warp must not repeat"),
        |_| panic!("show must not repeat"),
    )
    .unwrap();

    assert_eq!(
        &*calls.borrow(),
        &["associate", "warp", "show", "associate"]
    );
    assert!(state.is_restored());
}
```

- [x] **Step 2: Run and observe RED**

Run:

```bash
cargo test platform::macos::pointer_capture --no-run
```

Expected: compilation fails because `CaptureState` and `restore_with` do not
exist.

- [x] **Step 3: Implement acquisition and restoration**

Acquisition saves the global cursor point, resolves its display, hides the
cursor, disassociates motion, and stores:

```rust
CaptureState {
    saved_position,
    display_id,
    associated: false,
    position_restored: false,
    cursor_hidden: true,
}
```

`restore_with` attempts every unfinished step and returns the first error.
Successful steps update state before the next operation. Backend begin first
finishes any stale state left by an earlier failed drop.

- [x] **Step 4: Run focused real and state-machine tests**

Run outside the default sandbox because Core Graphics calls need WindowServer:

```bash
cargo test relative_pointer_capture -- --nocapture
```

Expected: lifecycle and retry tests pass and the cursor is visible/restored
after the test.

### Task 3: Hook and channel shutdown safety net

**Files:**

- Modify: `src/hook.rs`
- Modify: `src/channel.rs`
- Test: `src/pointer_capture.rs`

**Interfaces:**

- Consumes: `pointer_capture::release_active() -> Result<()>`.
- Produces: all blocking/async Hook exits and `ChannelHookHandle::stop_inner`
  attempt capture restoration before reporting completion.

- [x] **Step 1: Write a failing idempotent external-release test**

Acquire a lease, call the crate-private `release_active()`, assert the owner
reports inactive, then drop it and reacquire. This catches a shutdown path that
restores native state but leaves global ownership stuck.

```rust
#[test]
fn hook_shutdown_releases_relative_pointer_capture() {
    let owner = RelativePointerCapture::acquire().unwrap();

    release_active().unwrap();

    assert!(!owner.is_active());
    drop(owner);
    RelativePointerCapture::acquire().unwrap().release().unwrap();
}
```

- [x] **Step 2: Observe RED**

Run:

```bash
cargo test hook_shutdown_releases_relative_pointer_capture -- --nocapture
```

Expected: compilation fails because the shutdown helper does not exist.

- [x] **Step 3: Wire cleanup into Hook and channel exits**

Blocking methods combine backend and cleanup results, preserving the backend
error and logging a simultaneous cleanup error. Async closures log cleanup
errors before clearing `running`. `stop()` and channel stop attempt cleanup
after stopping/joining.

- [x] **Step 4: Verify GREEN**

Run:

```bash
cargo test relative_pointer_capture -- --nocapture
```

Expected: all capture lifecycle tests pass.

### Task 4: Public documentation and full verification

**Files:**

- Modify: `README.md`
- Modify: `docs/input-provenance-cross-platform-handoff.md`
- Modify: `src/lib.rs`

**Interfaces:**

- Documents the exact `Hook::grab_async()` plus
  `RelativePointerCapture::acquire()` CrossFlow flow.

- [x] **Step 1: Add a compiling public example**

The example must acquire the lease for the remote-active interval and
explicitly release it before returning local. It must state that the handler
still controls event suppression.

- [x] **Step 2: Update platform status**

Mark the macOS cursor-disassociated lease implemented, retain physical
screen-edge and crash-recovery native tests as unresolved acceptance work, and
leave Windows Raw Input as future work.

- [x] **Step 3: Run full verification**

```bash
cargo fmt --all -- --check
cargo test --all-features
cargo clippy --all-features --all-targets -- -D warnings
cargo doc --all-features --no-deps
cargo check --target x86_64-unknown-linux-gnu --no-default-features --features evdev --all-targets
cargo check --target x86_64-pc-windows-msvc --all-targets
git diff --check
```

Core Graphics tests run outside the default sandbox. Cross-target checks do not
claim native runtime validation.

- [x] **Step 4: Review and commit**

Review every changed file, confirm only goal-owned changes are present, then:

```bash
git add .journal/2026-07-30-2324.md Cargo.toml README.md \
  docs/input-provenance-cross-platform-handoff.md \
  docs/superpowers/plans/2026-07-30-relative-pointer-capture.md \
  examples/crossflow_relative_capture.rs \
  src/channel.rs src/error.rs src/hook.rs src/lib.rs \
  src/pointer_capture.rs \
  src/platform/linux/mod.rs src/platform/windows/mod.rs \
  src/platform/macos/mod.rs src/platform/macos/pointer_capture.rs
git commit -m "feat: add relative pointer capture lease"
```
