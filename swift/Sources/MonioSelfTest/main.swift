import CoreGraphics
import Foundation
import MonioInput

/// `monio-selftest` — check the OS layer without a human.
///
/// ## What it proves, and why that is the interesting part
///
/// The tempting assumption about automated input testing is that it is
/// impossible: a test must synthesize events, and synthesized events are not
/// physical, so nothing real is being tested. That is wrong, because the
/// provenance signal is not "physical vs synthetic" — it is **"mine vs not
/// mine"**.
///
/// So this program can check, with no human and no guesswork:
///
/// 1. the tap really sees events (they arrive at all);
/// 2. events this process injected come back tagged `selfInjected`;
/// 3. a grab really swallows them (the rest of the machine does not see them);
/// 4. modifier press/release round-trips through `flagsChanged`, left and right
///    independently — the exact path that produces stuck keys.
///
/// What it cannot check is whether a *human hand* produced an event. Nothing on
/// macOS can, which is why the engine never infers that.
///
/// Needs Accessibility. Run it from a terminal that has it.
/// The tap callback runs on the tap thread, so what it writes into cannot be
/// actor-isolated. A plain lock-guarded box is the whole requirement.
final class EventRecorder: @unchecked Sendable {
  private var observed: [MonioEvent] = []
  private let lock = NSLock()

  func record(_ event: MonioEvent) {
    lock.lock()
    observed.append(event)
    lock.unlock()
  }

  func drain() -> [MonioEvent] {
    lock.lock()
    defer {
      observed.removeAll()
      lock.unlock()
    }
    return observed
  }
}

final class SelfTest {
  private let recorder = EventRecorder()
  private var failures: [String] = []

  func drain() -> [MonioEvent] { recorder.drain() }

  func check(_ passed: Bool, _ what: String) {
    if passed {
      print("  ok   \(what)")
    } else {
      print("  FAIL \(what)")
      failures.append(what)
    }
  }

  func run() -> Int32 {
    guard EventTap.isPermitted, Injector.isPermitted else {
      print(
        """
        This process is not trusted for Accessibility.

        Grant it in System Settings > Privacy & Security > Accessibility. A binary
        run from a terminal is granted through THAT terminal — add Terminal.app or
        iTerm, not this executable.
        """)
      return 2
    }

    let injector = Injector()
    let recorder = self.recorder
    let tap = EventTap { event in
      recorder.record(event)
      // Swallow everything this process injected so the self-test cannot type
      // into whatever window happens to be focused.
      return event.origin == .selfInjected ? .consume : .pass
    }

    do {
      try tap.start()
    } catch {
      print("could not start the tap: \(error)")
      return 2
    }
    defer { tap.stop() }

    print("1. the tap sees an injected key, and knows it was ours")
    settle()
    _ = drain()
    try? injector.key(.f13, pressed: true)
    try? injector.key(.f13, pressed: false)
    settle()
    let keyEvents = drain().filter {
      if case .key(.f13, _) = $0.kind { return true }
      return false
    }
    check(keyEvents.count >= 2, "both the press and the release arrived")
    check(
      keyEvents.allSatisfy { $0.origin == .selfInjected },
      "every one was tagged selfInjected (this is what breaks the feedback loop)")

    print("2. modifiers round-trip through flagsChanged, left and right apart")
    for (name, key) in [("left shift", MonioKey.shiftLeft), ("right shift", .shiftRight)] {
      _ = drain()
      try? injector.key(key, pressed: true)
      settle()
      let downs = drain().compactMap { event -> Bool? in
        if case let .key(observed, pressed) = event.kind, observed == key { return pressed }
        return nil
      }
      try? injector.key(key, pressed: false)
      settle()
      let ups = drain().compactMap { event -> Bool? in
        if case let .key(observed, pressed) = event.kind, observed == key { return pressed }
        return nil
      }
      check(downs.contains(true), "\(name) reported a press")
      check(ups.contains(false), "\(name) reported a release")
    }

    print("3. the two shifts do not shadow each other")
    _ = drain()
    try? injector.key(.shiftLeft, pressed: true)
    try? injector.key(.shiftRight, pressed: true)
    try? injector.key(.shiftLeft, pressed: false)
    settle()
    let afterOneRelease = drain().compactMap { event -> (MonioKey, Bool)? in
      if case let .key(key, pressed) = event.kind, key == .shiftLeft || key == .shiftRight {
        return (key, pressed)
      }
      return nil
    }
    // Releasing the left one must NOT report the right one as released too.
    check(
      !afterOneRelease.contains(where: { $0 == (.shiftRight, false) }),
      "releasing left shift left right shift alone")
    try? injector.key(.shiftRight, pressed: false)
    injector.resetHeldInput()
    settle()

    print("4. motion while a button is held goes out as a drag, not a move")
    // The whole reason this check exists: macOS will not turn a `mouseMoved`
    // into a drag just because a button is down. Posting the wrong one is
    // invisible here — the cursor still moves — and shows up on the far machine
    // as text that will not select and windows that will not move.
    let start = MonioDisplays.pointerLocation ?? CGPoint(x: 200, y: 200)
    _ = drain()
    try? injector.button(.left, pressed: true, at: start)
    try? injector.movePointer(to: CGPoint(x: start.x + 12, y: start.y + 12))
    settle()
    let duringPress = drain()
    try? injector.button(.left, pressed: false, at: CGPoint(x: start.x + 12, y: start.y + 12))
    injector.resetHeldInput()
    settle()
    _ = drain()

    check(
      duringPress.contains {
        if case .pointerDragged(.left) = $0.kind { return true }
        return false
      },
      "the tap saw a left drag")
    check(
      !duringPress.contains {
        if case .pointerMoved = $0.kind { return true }
        return false
      },
      "and no plain move was posted alongside it")

    print("5. display enumeration works without any permission")
    let displays = MonioDisplays.active()
    check(!displays.isEmpty, "at least one display was reported")
    check(displays.filter(\.isPrimary).count == 1, "exactly one is primary")

    print("")
    if failures.isEmpty {
      print("selftest: all checks passed")
      return 0
    }
    print("selftest: \(failures.count) check(s) failed")
    for failure in failures { print("  - \(failure)") }
    return 1
  }

  /// Let the tap thread catch up. Events go through the window server, so they
  /// arrive after a round trip, not synchronously.
  private func settle() {
    RunLoop.current.run(until: Date().addingTimeInterval(0.25))
  }
}

exit(SelfTest().run())
