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

## Permissions, and what that means for testing

| | Accessibility needed | Testable headlessly |
| --- | --- | --- |
| `MonioDisplays.active()` | no | yes, including CI |
| key tables, provenance classification, modifier flags | no | yes, including CI |
| `EventTap` | yes | only from a permitted process |
| `Injector` | yes | only from a permitted process |

`swift test` covers everything in the first two rows — 11 tests, no display
server required.

For the rest there is `monio-selftest`, which needs no human:

```
swift run monio-selftest
```

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
