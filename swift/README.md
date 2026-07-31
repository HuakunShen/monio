# MonioInput

Monio's native Apple input layer: capture, injection, display enumeration, and
self-injection provenance, in Swift.

A **sibling** of the Rust crate rather than a binding to it. Capture and
injection on Apple platforms need a `CFRunLoop`, a signed bundle identity for
TCC, and CoreGraphics types that are far more natural from Swift. The two
implementations share a *contract* — the same neutral key vocabulary, the same
provenance scheme — not code. The macOS key table here mirrors
`src/platform/macos/keycodes.rs` exactly, so a Swift head and a Rust head on the
same Mac agree key for key.

```swift
let tap = EventTap { event in
  print(event.kind, event.origin)
  return .pass          // or .consume, which is what "grabbing" means
}
try tap.start()
```

## What is here, against the Rust crate

This is the subset a CrossFlow head needs, not a port. The three primitives are
all present:

| Rust | Swift |
| --- | --- |
| `listen` | `EventTap`, returning `.pass` |
| `grab` | the same tap, returning `.consume` |
| `simulate`, `key_press`/`key_release`, `mouse_press`/`mouse_release`, `mouse_move` | `Injector.key`/`button`/`movePointer`/`scroll` |
| `MouseMoved` vs `MouseDragged` | `.pointerMoved` vs `.pointerDragged(button)` |
| `displays`, `primary_display`, `display_at_point` | `MonioDisplays.active()`, `.isPrimary`, `display(at:)` |
| `mouse_position` | `MonioDisplays.pointerLocation` |
| `RelativePointerCapture` | `RelativePointerCapture.shared` |
| `InputOrigin` | `InputOrigin`, same two-part evidence |

Deliberately absent, because the head does not use them and an untested
convenience is worse than none: `channel` (the head owns its own
`AsyncStream`), `recorder`, `statistics`, `key_tap`, `mouse_click`,
`KeyTyped` (dead-key composition — a head forwards key codes, and the far
machine's own input method composes), `HookEnabled`/`HookDisabled` (the tap
re-enables itself and does not report it), and `system_settings`.

## Permissions, and what that means for testing

| | Accessibility needed | Testable headlessly |
| --- | --- | --- |
| `MonioDisplays.active()` | no | yes, including CI |
| key tables, provenance classification, modifier flags | no | yes, including CI |
| `EventTap` | yes | only from a permitted process |
| `Injector` | yes | only from a permitted process |

Run everything from **this directory** (`vendors/monio/swift`) — the executables
are SwiftPM products, so they are started with `swift run <product>`. Handing
`swift` a source file directly runs it in script mode with no package around it,
which fails with `no such module 'MonioInput'`.

```
swift test                      # 16 tests, no permission, no display server
swift run monio-selftest        # needs Accessibility; no human
swift run monio-capture-demo    # needs Accessibility; --grab to swallow
```

`swift test` covers everything in the first two rows of the table.

For the rest there is `monio-selftest`, which needs no human:

It injects events into itself and asserts that its own tap saw them, tagged as
self-injected, with modifier press/release round-tripping and left/right
independent. The point worth understanding is *why* that works: provenance is
not "physical vs synthetic", it is **"mine vs not mine"**. A synthetic event
from a test harness is not tagged with this process's session tag, so it reads
as `unknown` — exactly like a real keystroke. What genuinely cannot be checked
automatically is whether a human hand produced an event, and nothing on macOS
can tell you that.

`monio-capture-demo` prints events live; `--grab` swallows them instead.

## Granting permission

A binary run from a terminal is trusted through **that terminal**, not by its
own name. Add Terminal.app or iTerm under System Settings > Privacy & Security >
Accessibility. A shipping app needs its own signed bundle identity.
