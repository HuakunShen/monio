# Windows Relative Grab Motion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Windows `grab()` emit unclipped Raw Input pointer deltas through the same `MouseData::relative` contract as X11 and verify that the backend is a viable input adapter for an interactive-session CrossFlow agent.

**Architecture:** Keep `WH_MOUSE_LL` for global suppression, buttons, wheel events, injected-event provenance, and projected absolute coordinates. During grab only, register a message-only window for mouse Raw Input, dispatch physical motion from `WM_INPUT`, and replay accepted motion through privately tagged relative `SendInput` so the low-level hook passes it without recursion.

**Tech Stack:** Rust 2024, `windows` 0.59 Win32 bindings (`WindowsAndMessaging`, `UI::Input`, `KeyboardAndMouse`), Raw Input, low-level keyboard/mouse hooks, `SendInput`, existing Monio event/provenance APIs.

## Global Constraints

- Ordinary Windows `listen()` continues to report `MouseData::relative == None`.
- Conventional relative mouse/touchpad motion in Windows `grab()` produces exactly one handler callback containing absolute `x/y` and raw `delta_x/delta_y`.
- Physical motion is consumed before the handler decision; `Some` replays it locally and `None` leaves it suppressed.
- Injected motion remains on the low-level-hook path so `dwExtraInfo` provenance is preserved.
- Private grab replay is recognized as `ThisMonioSession` but bypasses the grab handler; ordinary Monio simulation remains observable.
- Mouse Raw Input registration is owned only for the grab lifetime and the process's previous mouse registration is restored on cleanup.
- The process-global Windows backend rejects a concurrent hook session rather than allowing shared callback state to be overwritten.
- `mouse_move_relative()` uses signed relative `SendInput`, not read-current-position plus absolute injection.
- Rust remains the Windows platform implementation language; CrossFlow runs the input agent in each interactive user session, not as an interactive session-0 service.
- No claim of screen-edge completion is made without a physical-mouse native diagnostic.

---

## File map

- Create `src/platform/windows/raw_input.rs`: Raw Mouse decoding, absolute normalization, message-only window registration, previous-registration restoration, injected-message filtering, and `WM_INPUT` reads.
- Modify `src/platform/windows/mod.rs`: declare the new internal module.
- Modify `src/platform/windows/provenance.rs`: add a distinct private grab-replay tag and classify both process tags as `ThisMonioSession`.
- Modify `src/platform/windows/simulate.rs`: build native relative `MOUSEINPUT` records and provide private absolute/relative replay helpers.
- Modify `src/platform/windows/listen.rs`: coordinate the low-level hook and Raw Input streams, enforce singleton lifecycle, and guarantee partial-start cleanup.
- Create `examples/windows_relative_grab_detection.rs`: physical Windows consume/pass-through diagnostic.
- Modify `Cargo.toml`: register the Windows diagnostic example.
- Modify `README.md`, `AGENTS.md`, and `docs/input-provenance-cross-platform-handoff.md`: document Windows relative grab behavior, Rust-agent deployment, platform boundaries, and exact verification results.
- Modify `docs/superpowers/specs/2026-07-30-windows-relative-grab-motion-design.md`: change status and confirmed/pending acceptance facts after implementation.

---

### Task 1: Native Relative Injection and Private Replay Identity

**Files:**
- Modify: `src/platform/windows/provenance.rs`
- Modify: `src/platform/windows/simulate.rs`

**Interfaces:**
- Consumes: existing `provenance::session_tag() -> Result<usize>` and `build_mouse_input(...) -> Result<INPUT>`.
- Produces: `provenance::grab_replay_tag() -> Result<usize>`, `provenance::is_grab_replay(&MSLLHOOKSTRUCT) -> bool`, `simulate::replay_mouse_move_relative(f64, f64) -> Result<()>`, and `simulate::replay_mouse_move_absolute(f64, f64) -> Result<()>`.

- [ ] **Step 1: Write failing provenance tests**

Replace the single-tag-only assumptions in the Windows provenance tests with:

```rust
#[test]
fn injection_and_grab_replay_tags_are_distinct_nonzero_u32_values() {
    let injection = session_tag().expect("injection tag should initialize");
    let replay = grab_replay_tag().expect("replay tag should initialize");

    assert_ne!(injection, 0);
    assert_ne!(replay, 0);
    assert_ne!(injection, replay);
    assert_eq!(injection & !(u32::MAX as usize), 0);
    assert_eq!(replay & !(u32::MAX as usize), 0);
}

#[test]
fn injected_input_from_either_process_tag_is_this_session() {
    let expected = InputOrigin::Injected {
        injector: InjectorIdentity::ThisMonioSession,
    };

    assert_eq!(
        classify_source(true, session_tag().unwrap(), &recognized_tags().unwrap()),
        expected
    );
    assert_eq!(
        classify_source(
            true,
            grab_replay_tag().unwrap(),
            &recognized_tags().unwrap()
        ),
        expected
    );
}

#[test]
fn only_the_private_replay_tag_bypasses_grab_dispatch() {
    let replay = MSLLHOOKSTRUCT {
        flags: LLMHF_INJECTED,
        dwExtraInfo: grab_replay_tag().unwrap(),
        ..Default::default()
    };
    let ordinary = MSLLHOOKSTRUCT {
        flags: LLMHF_INJECTED,
        dwExtraInfo: session_tag().unwrap(),
        ..Default::default()
    };

    assert!(is_grab_replay(&replay));
    assert!(!is_grab_replay(&ordinary));
}
```

- [ ] **Step 2: Run provenance tests and verify the new API is missing**

Run:

```powershell
cargo test platform::windows::provenance::tests
```

Expected: compilation fails because `grab_replay_tag`, `recognized_tags`, and `is_grab_replay` do not exist.

- [ ] **Step 3: Implement two stable process tags**

Use one `OnceLock` holding both tags so initialization cannot partially succeed:

```rust
#[derive(Clone, Copy)]
struct SessionTags {
    injection: usize,
    grab_replay: usize,
}

static SESSION_TAGS: OnceLock<std::result::Result<SessionTags, String>> = OnceLock::new();

fn generate_session_tags() -> std::result::Result<SessionTags, String> {
    let injection = generate_nonzero_u32_tag()?;
    let grab_replay = loop {
        let candidate = generate_nonzero_u32_tag()?;
        if candidate != injection {
            break candidate;
        }
    };
    Ok(SessionTags {
        injection,
        grab_replay,
    })
}

fn tags() -> Result<SessionTags> {
    match SESSION_TAGS.get_or_init(generate_session_tags) {
        Ok(tags) => Ok(*tags),
        Err(message) => Err(Error::Platform(format!(
            "failed to initialize input injection tags: {message}"
        ))),
    }
}

pub(super) fn session_tag() -> Result<usize> {
    Ok(tags()?.injection)
}

pub(super) fn grab_replay_tag() -> Result<usize> {
    Ok(tags()?.grab_replay)
}

fn recognized_tags() -> Result<[usize; 2]> {
    let tags = tags()?;
    Ok([tags.injection, tags.grab_replay])
}
```

Change classification to accept either exact tag only when the low-level hook's injected flag is present:

```rust
fn classify_source(
    is_injected: bool,
    observed_tag: usize,
    expected_tags: &[usize],
) -> InputOrigin {
    if is_injected
        && observed_tag != 0
        && expected_tags.contains(&observed_tag)
    {
        InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        }
    } else {
        InputOrigin::Unknown
    }
}

pub(super) fn is_grab_replay(event: &MSLLHOOKSTRUCT) -> bool {
    event.flags & LLMHF_INJECTED != 0
        && grab_replay_tag()
            .is_ok_and(|expected| event.dwExtraInfo == expected)
}
```

- [ ] **Step 4: Write failing native-relative-input tests**

Add to `src/platform/windows/simulate.rs`:

```rust
#[test]
fn relative_axis_values_are_rounded_clamped_and_finite() {
    assert_eq!(normalize_relative_axis(4.4), 4);
    assert_eq!(normalize_relative_axis(4.6), 5);
    assert_eq!(normalize_relative_axis(-4.6), -5);
    assert_eq!(normalize_relative_axis(f64::NAN), 0);
    assert_eq!(normalize_relative_axis(f64::INFINITY), 0);
    assert_eq!(normalize_relative_axis(f64::MAX), i32::MAX);
    assert_eq!(normalize_relative_axis(f64::MIN), i32::MIN);
}

#[test]
fn relative_mouse_input_has_no_absolute_flags() {
    let input = build_relative_mouse_input(12.0, -7.0, provenance::session_tag().unwrap())
        .expect("relative input should build");
    let mouse = unsafe { input.Anonymous.mi };

    assert!(mouse.dwFlags.contains(MOUSEEVENTF_MOVE));
    assert!(!mouse.dwFlags.contains(MOUSEEVENTF_ABSOLUTE));
    assert!(!mouse.dwFlags.contains(MOUSEEVENTF_VIRTUALDESK));
    assert_eq!((mouse.dx, mouse.dy), (12, -7));
}

#[test]
fn grab_replay_input_uses_the_private_tag() {
    let input = build_relative_mouse_input(
        1.0,
        2.0,
        provenance::grab_replay_tag().unwrap(),
    )
    .expect("replay input should build");
    let mouse = unsafe { input.Anonymous.mi };

    assert_eq!(mouse.dwExtraInfo, provenance::grab_replay_tag().unwrap());
}
```

- [ ] **Step 5: Run simulation tests and verify they fail**

Run:

```powershell
cargo test platform::windows::simulate::tests
```

Expected: compilation fails because `normalize_relative_axis` and `build_relative_mouse_input` do not exist.

- [ ] **Step 6: Refactor mouse-input construction and implement native relative motion**

Add:

```rust
fn normalize_relative_axis(value: f64) -> i32 {
    if !value.is_finite() {
        0
    } else {
        value
            .round()
            .clamp(i32::MIN as f64, i32::MAX as f64) as i32
    }
}

fn build_mouse_input_with_tag(
    flags: MOUSE_EVENT_FLAGS,
    data: u32,
    dx: i32,
    dy: i32,
    tag: usize,
) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: tag,
            },
        },
    }
}

fn build_relative_mouse_input(delta_x: f64, delta_y: f64, tag: usize) -> Result<INPUT> {
    Ok(build_mouse_input_with_tag(
        MOUSEEVENTF_MOVE,
        0,
        normalize_relative_axis(delta_x),
        normalize_relative_axis(delta_y),
        tag,
    ))
}
```

Keep `build_mouse_input(...)` as the ordinary session-tagged wrapper. Change public relative motion to send signed deltas directly:

```rust
pub fn mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    let input = build_relative_mouse_input(delta_x, delta_y, provenance::session_tag()?)?;
    let mouse = unsafe { input.Anonymous.mi };
    if mouse.dx == 0 && mouse.dy == 0 {
        return Ok(());
    }
    send_input(input, "relative mouse movement")
}
```

Add private replay helpers using `provenance::grab_replay_tag()`:

```rust
pub(super) fn replay_mouse_move_relative(delta_x: f64, delta_y: f64) -> Result<()> {
    let input = build_relative_mouse_input(delta_x, delta_y, provenance::grab_replay_tag()?)?;
    send_input(input, "grab relative mouse replay")
}

pub(super) fn replay_mouse_move_absolute(x: f64, y: f64) -> Result<()> {
    let (dx, dy) = normalized_absolute_position(x, y)?;
    let input = build_mouse_input_with_tag(
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        0,
        dx,
        dy,
        provenance::grab_replay_tag()?,
    );
    send_input(input, "grab absolute mouse replay")
}
```

Extract `send_input(INPUT, context)` and `normalized_absolute_position(x, y)` from the existing functions so absolute public simulation remains session-tagged.

- [ ] **Step 7: Run focused Windows tests**

Run:

```powershell
cargo test platform::windows::provenance::tests
cargo test platform::windows::simulate::tests
```

Expected: all provenance and simulation tests pass.

- [ ] **Step 8: Commit native injection and replay identity**

```powershell
git add -- src/platform/windows/provenance.rs src/platform/windows/simulate.rs
git commit -m "feat(windows): inject native relative pointer motion"
```

---

### Task 2: Raw Mouse Decoding and Registration Lifecycle

**Files:**
- Create: `src/platform/windows/raw_input.rs`
- Modify: `src/platform/windows/mod.rs`

**Interfaces:**
- Consumes: Win32 `RAWMOUSE`, `RAWINPUT`, `RAWINPUTDEVICE`, `GetRawInputData`, and `RegisterRawInputDevices`.
- Produces: `RawMouseInput::acquire() -> Result<RawMouseInput>`, `RawMouseInput::window() -> HWND`, `RawMouseInput::read(LPARAM) -> Result<Option<RawMouseMotion>>`, `RawMouseInput::drain_pending() -> Result<()>`, `RawMouseInput::restore() -> Result<()>`, `event_from_motion(RawMouseMotion, (f64, f64), DesktopBounds, bool) -> Option<Event>`, and `RawMouseMotion`.

- [ ] **Step 1: Declare the module and write failing pure decoder tests**

Add `mod raw_input;` to `src/platform/windows/mod.rs`.

Create `src/platform/windows/raw_input.rs` with tests:

```rust
#[test]
fn decodes_relative_raw_mouse_motion() {
    let raw = raw_mouse(MOUSE_MOVE_RELATIVE, 14, -9);

    assert_eq!(
        decode_raw_mouse(&raw),
        Some(RawMouseMotion::Relative {
            delta_x: 14,
            delta_y: -9,
        })
    );
}

#[test]
fn ignores_zero_relative_motion() {
    let raw = raw_mouse(MOUSE_MOVE_RELATIVE, 0, 0);

    assert_eq!(decode_raw_mouse(&raw), None);
}

#[test]
fn decodes_absolute_virtual_desktop_motion() {
    let raw = raw_mouse(MOUSE_MOVE_ABSOLUTE | MOUSE_VIRTUAL_DESKTOP, 32_768, 65_535);

    assert_eq!(
        decode_raw_mouse(&raw),
        Some(RawMouseMotion::Absolute {
            normalized_x: 32_768,
            normalized_y: 65_535,
            virtual_desktop: true,
        })
    );
}

#[test]
fn normalizes_absolute_raw_coordinates_to_desktop_pixels() {
    let bounds = DesktopBounds {
        x: -1920,
        y: 0,
        width: 3840,
        height: 1080,
    };

    assert_eq!(
        absolute_point(0, 0, bounds),
        (-1920.0, 0.0)
    );
    assert_eq!(
        absolute_point(65_535, 65_535, bounds),
        (1919.0, 1079.0)
    );
}

#[test]
fn relative_event_retains_absolute_point_and_drag_state() {
    let moved = event_from_motion(
        RawMouseMotion::Relative {
            delta_x: 3,
            delta_y: -2,
        },
        (100.0, 200.0),
        DesktopBounds::default(),
        false,
    )
    .unwrap();
    let dragged = event_from_motion(
        RawMouseMotion::Relative {
            delta_x: 3,
            delta_y: -2,
        },
        (100.0, 200.0),
        DesktopBounds::default(),
        true,
    )
    .unwrap();

    assert_eq!(moved.event_type, EventType::MouseMoved);
    assert_eq!(dragged.event_type, EventType::MouseDragged);
    assert_eq!(
        moved.mouse.unwrap().relative,
        Some(RelativeMotion {
            delta_x: 3.0,
            delta_y: -2.0,
        })
    );
}
```

The test helper constructs `RAWMOUSE` without reading its button union:

```rust
fn raw_mouse(flags: MOUSE_STATE, x: i32, y: i32) -> RAWMOUSE {
    RAWMOUSE {
        usFlags: flags,
        lLastX: x,
        lLastY: y,
        ..Default::default()
    }
}
```

- [ ] **Step 2: Run the decoder tests and verify they fail**

Run:

```powershell
cargo test platform::windows::raw_input::tests
```

Expected: compilation fails because the decoder types and functions are not defined.

- [ ] **Step 3: Implement pure Raw Mouse translation**

Define:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawMouseMotion {
    Relative {
        delta_x: i32,
        delta_y: i32,
    },
    Absolute {
        normalized_x: i32,
        normalized_y: i32,
        virtual_desktop: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DesktopBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

fn decode_raw_mouse(raw: &RAWMOUSE) -> Option<RawMouseMotion> {
    let absolute = raw.usFlags.contains(MOUSE_MOVE_ABSOLUTE);
    if raw.lLastX == 0 && raw.lLastY == 0 && !absolute {
        return None;
    }
    if absolute {
        Some(RawMouseMotion::Absolute {
            normalized_x: raw.lLastX.clamp(0, 65_535),
            normalized_y: raw.lLastY.clamp(0, 65_535),
            virtual_desktop: raw.usFlags.contains(MOUSE_VIRTUAL_DESKTOP),
        })
    } else {
        Some(RawMouseMotion::Relative {
            delta_x: raw.lLastX,
            delta_y: raw.lLastY,
        })
    }
}
```

Implement normalized absolute conversion with `(extent - 1) / 65_535` and construct relative or absolute `Event` values. Relative events use `Event::mouse_moved_relative` or `Event::mouse_dragged_relative`; absolute events use the existing absolute constructors.

Expose the translator to the grab loop with this exact signature:

```rust
pub(super) fn event_from_motion(
    motion: RawMouseMotion,
    absolute_point: (f64, f64),
    bounds: DesktopBounds,
    dragging: bool,
) -> Option<Event>
```

- [ ] **Step 4: Write failing registration-selection tests**

Add pure tests:

```rust
#[test]
fn finds_only_generic_desktop_mouse_registration() {
    let registrations = [
        registration(0x01, 0x06, HWND(10 as _)),
        registration(0x01, 0x02, HWND(20 as _)),
    ];

    assert_eq!(
        existing_mouse_registration(&registrations).unwrap().hwndTarget,
        HWND(20 as _)
    );
}

#[test]
fn restore_is_allowed_only_while_monio_still_owns_mouse_registration() {
    let monio_window = HWND(30 as _);

    assert!(registration_is_owned_by(
        Some(registration(0x01, 0x02, monio_window)),
        monio_window
    ));
    assert!(!registration_is_owned_by(
        Some(registration(0x01, 0x02, HWND(31 as _))),
        monio_window
    ));
}
```

- [ ] **Step 5: Run registration tests and verify they fail**

Run:

```powershell
cargo test platform::windows::raw_input::tests
```

Expected: decoder tests pass and registration helper tests fail to compile.

- [ ] **Step 6: Implement Raw Input enumeration and RAII ownership**

Implement these constants and structure:

```rust
const GENERIC_DESKTOP_PAGE: u16 = 0x01;
const GENERIC_DESKTOP_MOUSE: u16 = 0x02;
const RAW_INPUT_ERROR: u32 = u32::MAX;

pub(super) struct RawMouseInput {
    window: HWND,
    previous_registration: Option<RAWINPUTDEVICE>,
    registered: bool,
}
```

`RawMouseInput::acquire()` must:

1. call `registered_devices()` and retain the Generic Desktop mouse entry;
2. create a message-only built-in `STATIC` window with `CreateWindowExW`,
   `HWND_MESSAGE`, zero size, and no menu or instance;
3. register one `RAWINPUTDEVICE` using `RIDEV_INPUTSINK` and the created window;
4. destroy the window immediately if registration fails.

Use:

```rust
let registration = RAWINPUTDEVICE {
    usUsagePage: GENERIC_DESKTOP_PAGE,
    usUsage: GENERIC_DESKTOP_MOUSE,
    dwFlags: RIDEV_INPUTSINK,
    hwndTarget: window,
};
unsafe {
    RegisterRawInputDevices(&[registration], size_of::<RAWINPUTDEVICE>() as u32)
}
.map_err(|error| Error::HookStartFailed(format!(
    "Failed to register Windows Raw Input mouse: {error}"
)))?;
```

`registered_devices()` must use the documented two-call size/read pattern and retry when the supplied buffer becomes insufficient. Return an actionable `HookStartFailed` on any `u32::MAX` result not caused by buffer growth.

`restore()` must:

1. query the current mouse registration;
2. when its target is still `self.window`, register `RIDEV_REMOVE` with a null target;
3. re-register `previous_registration` when present;
4. destroy the message-only window;
5. mark the structure restored so `Drop` is a no-op.

`Drop` calls the same operations best-effort and never panics.

- [ ] **Step 7: Implement `WM_INPUT` reading and injected-source filtering**

Implement:

```rust
pub(super) fn read(&self, lparam: LPARAM) -> Result<Option<RawMouseMotion>> {
    let mut source = INPUT_MESSAGE_SOURCE::default();
    if unsafe { GetCurrentInputMessageSource(&mut source) }.is_ok()
        && source.originId == IMO_INJECTED
    {
        return Ok(None);
    }

    let mut raw = MaybeUninit::<RAWINPUT>::zeroed();
    let mut size = size_of::<RAWINPUT>() as u32;
    let copied = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as *mut c_void),
            RID_INPUT,
            Some(raw.as_mut_ptr().cast()),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if copied == u32::MAX {
        return Err(Error::Platform(
            "GetRawInputData failed for Windows mouse input".into(),
        ));
    }
    if copied < size_of::<RAWINPUTHEADER>() as u32 {
        return Err(Error::Platform(format!(
            "GetRawInputData returned a truncated header ({copied} bytes)"
        )));
    }

    let raw = unsafe { raw.assume_init() };
    if raw.header.dwType != RIM_TYPEMOUSE.0 {
        return Ok(None);
    }
    Ok(decode_raw_mouse(unsafe { &raw.data.mouse }))
}
```

`drain_pending()` repeatedly calls `PeekMessageW` for `WM_INPUT` and this target window with `PM_REMOVE`, reads and discards each raw handle, then dispatches the message so the built-in window procedure performs system cleanup.

- [ ] **Step 8: Run focused Raw Input tests and Windows check**

Run:

```powershell
cargo test platform::windows::raw_input::tests
cargo check
```

Expected: all new unit tests pass and the Windows target compiles without adding another dependency feature because `Win32_UI_Input_KeyboardAndMouse` already enables its parent `Win32_UI_Input`.

- [ ] **Step 9: Commit the Raw Input adapter**

```powershell
git add -- src/platform/windows/mod.rs src/platform/windows/raw_input.rs
git commit -m "feat(windows): add raw mouse input adapter"
```

---

### Task 3: Coordinate Raw Input with Windows Grab

**Files:**
- Modify: `src/platform/windows/listen.rs`

**Interfaces:**
- Consumes: `RawMouseInput`, `RawMouseMotion`, `event_from_motion`, `simulate::replay_mouse_move_relative`, `simulate::replay_mouse_move_absolute`, and `provenance::is_grab_replay`.
- Produces: Windows `run_grab_hook` behavior in which physical movement is dispatched once from Raw Input and suppressed/replayed according to `GrabHandler`.

- [ ] **Step 1: Write failing singleton-session and motion-routing tests**

Add tests around pure/RAII helpers:

```rust
#[test]
fn backend_session_guard_rejects_overlap_and_releases_on_drop() {
    let first = ActiveSession::claim().expect("first session should claim");
    assert!(matches!(ActiveSession::claim(), Err(Error::AlreadyRunning)));
    drop(first);
    ActiveSession::claim().expect("claim should be reusable after drop");
}

#[test]
fn physical_grab_motion_uses_raw_input_path_only_when_ready() {
    assert_eq!(
        mouse_move_route(false, false, false),
        MouseMoveRoute::Legacy
    );
    assert_eq!(
        mouse_move_route(true, false, false),
        MouseMoveRoute::Legacy
    );
    assert_eq!(
        mouse_move_route(true, true, false),
        MouseMoveRoute::RawPhysical
    );
    assert_eq!(
        mouse_move_route(true, true, true),
        MouseMoveRoute::Injected
    );
}
```

Define `mouse_move_route(grab_mode, grab_ready, injected)` as a pure helper so the callback policy is testable without installing a global hook.

- [ ] **Step 2: Run focused listen tests and verify they fail**

Run:

```powershell
cargo test platform::windows::listen::tests
```

Expected: compilation fails because `ActiveSession`, `MouseMoveRoute`, and `mouse_move_route` do not exist.

- [ ] **Step 3: Add backend lifecycle and readiness state**

Add:

```rust
static ACTIVE_SESSION: AtomicBool = AtomicBool::new(false);
static GRAB_READY: AtomicBool = AtomicBool::new(false);
static LATEST_PHYSICAL_POINT: Mutex<Option<(i32, i32)>> = Mutex::new(None);

struct ActiveSession;

impl ActiveSession {
    fn claim() -> Result<Self> {
        ACTIVE_SESSION
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| Error::AlreadyRunning)?;
        Ok(Self)
    }
}

impl Drop for ActiveSession {
    fn drop(&mut self) {
        ACTIVE_SESSION.store(false, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseMoveRoute {
    Legacy,
    RawPhysical,
    Injected,
}

fn mouse_move_route(grab_mode: bool, grab_ready: bool, injected: bool) -> MouseMoveRoute {
    if !grab_mode || !grab_ready {
        MouseMoveRoute::Legacy
    } else if injected {
        MouseMoveRoute::Injected
    } else {
        MouseMoveRoute::RawPhysical
    }
}
```

Claim `ActiveSession` at the beginning of both blocking platform entry points. Clear readiness and latest physical point during initialization and cleanup.

- [ ] **Step 4: Route low-level mouse movement without duplicate callbacks**

At the beginning of `mouse_callback` for `HC_ACTION`, inspect `MSLLHOOKSTRUCT` when `wparam == WM_MOUSEMOVE`.

Apply this order:

1. private replay: call `CallNextHookEx` immediately without conversion or handler dispatch;
2. ready physical grab movement: store `pt` in `LATEST_PHYSICAL_POINT` and return `LRESULT(1)`;
3. injected or non-ready movement: continue through existing `convert_event`.

Use the low-level injected bit:

```rust
let mouse = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
if provenance::is_grab_replay(mouse) {
    return call_next_mouse_hook(code, wparam, lparam);
}
let injected = mouse.flags & LLMHF_INJECTED != 0;
if mouse_move_route(
    GRAB_MODE.load(Ordering::SeqCst),
    GRAB_READY.load(Ordering::SeqCst),
    injected,
) == MouseMoveRoute::RawPhysical
{
    if let Ok(mut point) = LATEST_PHYSICAL_POINT.lock() {
        *point = Some((mouse.pt.x, mouse.pt.y));
    }
    return LRESULT(1);
}
```

Extract `call_next_mouse_hook` so every branch obtains the stored hook handle consistently.

- [ ] **Step 5: Write failing raw-dispatch tests with a recording handler**

Add a focused helper:

```rust
fn handle_raw_motion<H: GrabHandler>(
    handler: &H,
    motion: RawMouseMotion,
    absolute_point: (f64, f64),
    desktop_bounds: DesktopBounds,
) -> Result<()>
```

Test consume and replay selection by extracting:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
enum MotionReplay {
    Relative { delta_x: f64, delta_y: f64 },
    Absolute { x: f64, y: f64 },
}

fn replay_for_accepted_event(event: &Event) -> Option<MotionReplay>
```

Tests:

```rust
#[test]
fn accepted_relative_event_selects_relative_replay() {
    let event = Event::mouse_moved_relative(100.0, 200.0, 8.0, -3.0);

    assert_eq!(
        replay_for_accepted_event(&event),
        Some(MotionReplay::Relative {
            delta_x: 8.0,
            delta_y: -3.0,
        })
    );
}

#[test]
fn accepted_absolute_event_selects_absolute_replay() {
    let event = Event::mouse_moved(100.0, 200.0);

    assert_eq!(
        replay_for_accepted_event(&event),
        Some(MotionReplay::Absolute { x: 100.0, y: 200.0 })
    );
}
```

- [ ] **Step 6: Run listen tests and verify the replay helper is missing**

Run:

```powershell
cargo test platform::windows::listen::tests
```

Expected: the lifecycle tests pass and replay-selection tests fail to compile.

- [ ] **Step 7: Implement Raw Input handler dispatch**

`handle_raw_motion` must:

1. call `raw_input::event_from_motion` using `state::is_button_held()`;
2. return immediately for a zero/ignored sample;
3. call `handler.handle_event(&event)` exactly once;
4. do nothing when the handler returns `None`;
5. replay the original captured motion when it returns `Some`.

Use:

```rust
match replay_for_accepted_event(&event) {
    Some(MotionReplay::Relative { delta_x, delta_y }) => {
        simulate::replay_mouse_move_relative(delta_x, delta_y)?;
    }
    Some(MotionReplay::Absolute { x, y }) => {
        simulate::replay_mouse_move_absolute(x, y)?;
    }
    None => {}
}
```

Do not replay the handler's returned clone; current Windows semantics use `Some` only as the pass/consume decision.

- [ ] **Step 8: Refactor hook installation for partial-start cleanup**

Add an `InstalledHooks` RAII owner:

```rust
struct InstalledHooks {
    keyboard: Option<SendableHHOOK>,
    mouse: Option<SendableHHOOK>,
}
```

`InstalledHooks::install()`:

1. installs and stores the keyboard hook;
2. publishes it to `KEYBOARD_HOOK`;
3. installs the mouse hook;
4. if mouse installation fails, unhooks and clears the keyboard handle before returning;
5. publishes the mouse hook only after success.

`restore()`/`Drop` unhook mouse then keyboard, clear both static handles, and never double-unhook.

Use this owner in both listen and grab entry points so the new singleton guard cannot be left active after a setup error.

- [ ] **Step 9: Integrate Raw Input into the grab message loop**

In `run_grab_hook`:

1. claim `ActiveSession`;
2. initialize and store global handler state;
3. set `GRAB_MODE = true`, `GRAB_READY = false`;
4. acquire `RawMouseInput`;
5. install both hooks;
6. drain pending Raw Input;
7. set `GRAB_READY = true`;
8. emit `HookEnabled`;
9. process `GetMessageW`.

For each message:

```rust
if msg.message == WM_INPUT && msg.hwnd == raw_mouse.window() {
    if let Some(motion) = raw_mouse.read(msg.lParam)? {
        let point = latest_physical_point_or_cursor()?;
        let bounds = desktop_bounds_for(motion);
        handle_raw_motion(&handler, motion, point, bounds)?;
    }
    unsafe {
        DispatchMessageW(&msg);
    }
    continue;
}
unsafe {
    DispatchMessageW(&msg);
}
```

Check `GetMessageW`'s integer result explicitly: `0` means quit, `-1` becomes `Error::Platform`, and positive values are dispatched.

Before cleanup, set `GRAB_READY = false`. Then unhook, restore Raw Input registration/window, emit `HookDisabled` only after a prior `HookEnabled`, and clear handler/thread/global mode state.

- [ ] **Step 10: Run Windows tests and lints**

Run:

```powershell
cargo fmt --all
cargo test platform::windows
cargo clippy --all-features --all-targets -- -D warnings
```

Expected: all Windows unit tests pass and clippy reports no warnings.

- [ ] **Step 11: Commit Windows grab integration**

```powershell
git add -- src/platform/windows/listen.rs
git commit -m "feat(windows): report relative grab motion"
```

---

### Task 4: Windows Relative Grab Diagnostic

**Files:**
- Create: `examples/windows_relative_grab_detection.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: public `Hook::grab_async`, `Event`, `MouseData::relative`, and `Key::Escape`.
- Produces: `cargo run --example windows_relative_grab_detection -- --pass-through`.

- [ ] **Step 1: Add observation tests before the executable logic**

Create an `Observation` matching the X11 diagnostic but ignore self-injected events:

```rust
#[derive(Clone, Copy, Debug, Default)]
struct Observation {
    relative_events: usize,
    missing_relative_events: usize,
    drag_events: usize,
    positive_x: bool,
    positive_y: bool,
    negative_x: bool,
    negative_y: bool,
}

#[test]
fn records_relative_motion_signs_and_drag() {
    let mut observed = Observation::default();
    observed.observe(&Event::mouse_moved_relative(10.0, 20.0, 3.0, -2.0));
    observed.observe(&Event::mouse_dragged_relative(10.0, 20.0, -4.0, 5.0));

    assert_eq!(observed.relative_events, 2);
    assert_eq!(observed.drag_events, 1);
    assert!(observed.positive_x);
    assert!(observed.positive_y);
    assert!(observed.negative_x);
    assert!(observed.negative_y);
}

#[test]
fn skips_this_session_injected_motion() {
    let mut event = Event::mouse_moved(10.0, 20.0);
    event.origin = InputOrigin::Injected {
        injector: InjectorIdentity::ThisMonioSession,
    };
    let mut observed = Observation::default();

    observed.observe(&event);

    assert_eq!(observed.missing_relative_events, 0);
}
```

- [ ] **Step 2: Register the example and verify tests fail**

Add to `Cargo.toml`:

```toml
[[example]]
name = "windows_relative_grab_detection"
path = "examples/windows_relative_grab_detection.rs"
```

Run:

```powershell
cargo test --example windows_relative_grab_detection
```

Expected: compilation fails until `Observation::observe` and platform-gated `main` are implemented.

- [ ] **Step 3: Implement the physical diagnostic**

Use the same timeout and startup-channel pattern as `x11_relative_grab_detection`, with:

- `#[cfg(target_os = "windows")]` around the Windows implementation;
- `--pass-through` selecting local replay;
- a 10-second default run;
- Escape, Ctrl+C, and timeout as release paths;
- consume mode suppressing pointer button and motion events;
- output containing absolute coordinates, raw deltas, sign coverage, drag count, missing-relative count, and `Grab released`;
- a nonzero exit if a physical motion event has no relative data;
- a printed notice that `SendInput` cannot substitute for physical Raw Input edge verification.

On non-Windows targets, print that the diagnostic is Windows-only and exit successfully so `cargo check --examples` remains cross-platform.

- [ ] **Step 4: Run the example tests and compile it**

Run:

```powershell
cargo test --example windows_relative_grab_detection
cargo check --example windows_relative_grab_detection
```

Expected: both commands pass.

- [ ] **Step 5: Commit the diagnostic**

```powershell
git add -- Cargo.toml examples/windows_relative_grab_detection.rs
git commit -m "test(windows): diagnose relative grab motion"
```

---

### Task 5: Document Windows CrossFlow Readiness

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/input-provenance-cross-platform-handoff.md`
- Modify: `docs/superpowers/specs/2026-07-30-windows-relative-grab-motion-design.md`

**Interfaces:**
- Consumes: verified implementation behavior and exact command results from Tasks 1-4.
- Produces: user-facing Windows API documentation and a bounded CrossFlow Windows readiness assessment.

- [ ] **Step 1: Update README Windows behavior**

Document:

- Windows `listen()` remains absolute-only;
- Windows `grab()` combines `WH_MOUSE_LL` and Raw Input and fills `mouse.relative` for relative devices;
- `mouse_move_relative()` uses native relative `SendInput`;
- grab temporarily owns this process's mouse Raw Input registration and restores it;
- CrossFlow should run as a per-interactive-session native agent;
- `SendInput` UIPI and secure-desktop boundaries;
- the diagnostic command:

```text
cargo run --example windows_relative_grab_detection -- --pass-through
```

- [ ] **Step 2: Update AGENTS.md**

Add the diagnostic to Running Examples and describe Windows grab as:

```text
WH_KEYBOARD_LL/WH_MOUSE_LL suppression plus WM_INPUT raw relative motion.
Ordinary listen motion remains absolute-only. Grab pass-through uses a private
tagged SendInput replay that bypasses handler recursion.
```

- [ ] **Step 3: Add a Windows relative-grab and CrossFlow acceptance section to the handoff**

Record:

- exact files and mechanism;
- process Raw Input registration ownership/restoration;
- injected-message filtering and private replay tag;
- exact unit/build/native commands and observed output;
- whether physical edge delta and pass-through were verified;
- confirmed feasibility for a per-user Rust agent;
- hard platform boundaries: session 0, same desktop/session hook scope, UIPI, secure desktop;
- repository gaps that remain separate: keyboard scan-code/layout fidelity, wheel-direction replay, async startup handshake, held-state cleanup;
- native product matrix: multi-monitor DPI, 1000 Hz mouse, elevated app, RDP, fast user switching, lock/unlock, sleep/resume, disconnect, process crash.

- [ ] **Step 4: Update design status without overstating native acceptance**

Set the design status to implemented only after code checks pass. If the physical diagnostic has not run, retain an explicit pending line for screen-edge and one-to-one pass-through behavior.

- [ ] **Step 5: Validate documentation**

Run:

```powershell
cargo doc --all-features --no-deps
git diff --check
rg -n "Windows.*relative|Raw Input|UIPI|session 0|windows_relative_grab_detection" README.md AGENTS.md docs
```

Expected: rustdoc succeeds, diff check is clean, and every required Windows/CrossFlow topic is findable.

- [ ] **Step 6: Commit documentation**

```powershell
git add -- README.md AGENTS.md docs/input-provenance-cross-platform-handoff.md docs/superpowers/specs/2026-07-30-windows-relative-grab-motion-design.md
git commit -m "docs: document Windows relative grab motion"
```

---

### Task 6: Full Verification and Native Windows Acceptance

**Files:**
- Verify all modified files.
- Modify documentation only if exact observed results differ from Task 5.

**Interfaces:**
- Consumes: complete implementation and diagnostic.
- Produces: evidence-backed completion status with no claim beyond observed native behavior.

- [ ] **Step 1: Run formatting and the complete unit suite**

Run:

```powershell
cargo fmt --all -- --check
cargo test --all-features
```

Expected: both commands exit successfully.

- [ ] **Step 2: Run strict lint, example, and documentation checks**

Run:

```powershell
cargo clippy --all-features --all-targets -- -D warnings
cargo check --examples
cargo doc --all-features --no-deps
```

Expected: all commands exit successfully with no warnings promoted to errors.

- [ ] **Step 3: Re-run ordinary injection provenance**

Run:

```powershell
cargo run --example synthetic_input_detection
```

Expected: ControlLeft press/release and both mouse movements are classified as `Injected { injector: ThisMonioSession }`; the command exits zero.

- [ ] **Step 4: Run Windows relative grab in local pass-through mode**

Run:

```powershell
cargo run --example windows_relative_grab_detection -- --pass-through
```

During its 10-second window:

1. move right/down and left/up;
2. hold a mouse button while moving;
3. push repeatedly against at least one screen edge;
4. confirm local pointer movement feels one-to-one.

Expected:

- relative event count is nonzero;
- missing-relative count is zero for physical relative mouse motion;
- positive and negative X/Y signs are observed;
- drag count is nonzero after the held-button movement;
- pointer continues generating deltas at the edge;
- grab releases at timeout or Escape;
- no double movement or replay feedback is visible.

- [ ] **Step 5: Run Windows relative grab in consume mode**

Run:

```powershell
cargo run --example windows_relative_grab_detection
```

Expected: physical motion events continue to print with deltas while the local pointer is suppressed, Escape or timeout releases the grab, and local control returns immediately.

- [ ] **Step 6: Reconcile native evidence**

If the physical diagnostic ran, record exact counts and observations in `docs/input-provenance-cross-platform-handoff.md`. If physical input was unavailable, state exactly:

```text
Windows code/unit/provenance checks passed; physical screen-edge,
MouseDragged, consume, and pass-through acceptance remain pending.
```

Do not infer physical Raw Input behavior from `SendInput`.

- [ ] **Step 7: Inspect the final diff and history**

Run:

```powershell
git status --short
git diff --check
git log --oneline --decorate -8
```

Expected: no unintended files, no whitespace errors, and independently reviewable implementation commits.
