import CoreGraphics
import Foundation
import MonioInput

/// `monio-capture-demo` — watch what the tap sees, live.
///
/// The Swift counterpart of the Rust crate's `examples/basic.rs`. Prints every
/// observed event with its provenance, so the thing you are looking for is the
/// difference between what your hands produce (`unknown`) and what this process
/// produces (`selfInjected`).
///
/// Pass `--grab` to swallow everything instead of passing it through. Be ready:
/// while grabbing, the rest of the machine receives no input at all. Ctrl-C
/// still works because the terminal's SIGINT is delivered by the OS, not by the
/// event stream.
///
/// Needs Accessibility.
let shouldGrab = CommandLine.arguments.contains("--grab")

guard EventTap.isPermitted else {
  print(
    """
    Not trusted for Accessibility.

    System Settings > Privacy & Security > Accessibility. A binary run from a
    terminal is granted through THAT terminal — add Terminal.app or iTerm.
    """)
  exit(2)
}

print("displays:")
for display in MonioDisplays.active() {
  let mark = display.isPrimary ? " (primary)" : ""
  print(
    "  \(display.id): \(Int(display.bounds.width))x\(Int(display.bounds.height))"
      + " at \(Int(display.bounds.minX)),\(Int(display.bounds.minY))"
      + " @\(display.scale)x\(mark)")
}

print("")
print(shouldGrab ? "grabbing — the rest of the machine gets nothing. Ctrl-C to stop." : "watching. Ctrl-C to stop.")
print("")

let tap = EventTap { event in
  let origin = event.origin == .selfInjected ? "self" : "unknown"
  switch event.kind {
  case let .key(key, pressed):
    print("key      \(pressed ? "down" : "up  ")  \(key)  [\(origin)]")
  case let .button(button, pressed):
    print("button   \(pressed ? "down" : "up  ")  \(button)  [\(origin)]")
  case .pointerMoved, .pointerDragged:
    let delta = event.delta ?? .zero
    var label = "pointer "
    if case let .pointerDragged(button) = event.kind {
      label = "drag(\(button))"
    }
    // Deltas are the interesting part while grabbing: the location stops
    // changing the moment the cursor is frozen.
    print(
      "\(label)  \(Int(event.location.x)),\(Int(event.location.y))"
        + "  d(\(Int(delta.dx)),\(Int(delta.dy)))  [\(origin)]")
  case let .scroll(deltaX, deltaY):
    print("scroll   \(Int(deltaX)),\(Int(deltaY))  [\(origin)]")
  }
  return shouldGrab ? .consume : .pass
}

do {
  try tap.start()
} catch {
  print("could not start the tap: \(error)")
  exit(2)
}

if shouldGrab {
  RelativePointerCapture.shared.begin()
}

// Restore the cursor even on Ctrl-C: leaving it hidden and frozen is not a
// state a user can get out of without logging out.
let interrupt = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
interrupt.setEventHandler {
  RelativePointerCapture.shared.end()
  tap.stop()
  exit(0)
}
interrupt.resume()
signal(SIGINT, SIG_IGN)

RunLoop.main.run()
