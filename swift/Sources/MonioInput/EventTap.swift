import CoreGraphics
import Foundation

public enum TapDecision: Sendable {
  /// Let the event reach the rest of the machine.
  case pass
  /// Swallow it. This is what "grabbing" means: the local desktop never sees
  /// what the user is typing into the remote machine.
  case consume
}

public enum TapError: Error, CustomStringConvertible {
  case notPermitted
  case alreadyRunning

  public var description: String {
    switch self {
    case .notPermitted:
      """
      This process is not trusted for Accessibility, so it cannot observe input. \
      Grant it in System Settings > Privacy & Security > Accessibility. A process \
      launched from a terminal is granted through THAT terminal, not by its own name.
      """
    case .alreadyRunning:
      "the event tap is already running"
    }
  }
}

/// A CoreGraphics event tap, on its own thread with its own run loop.
///
/// ## Why the decision is made inside the callback
///
/// `handler` runs on the tap thread, synchronously, while the OS waits for an
/// answer. That is deliberate and it is the whole reason this class exists in
/// this shape: deciding whether to swallow an event at a screen edge cannot be
/// deferred to another task. A round trip is visible as cursor stutter, and the
/// events that leak in the meantime land in whatever local window happened to
/// be under the pointer.
///
/// The corollary is that `handler` must not block. Anything slow belongs behind
/// a queue the handler writes to without waiting.
public final class EventTap: @unchecked Sendable {
  private let handler: @Sendable (MonioEvent) -> TapDecision
  private let lock = NSLock()
  private var machPort: CFMachPort?
  private var runLoop: CFRunLoop?
  private var thread: Thread?

  public init(handler: @escaping @Sendable (MonioEvent) -> TapDecision) {
    self.handler = handler
  }

  /// Whether this process may observe input at all.
  ///
  /// A plain query with no prompt: prompting is a decision for the application,
  /// not for a library that might be running inside a test.
  public static var isPermitted: Bool {
    CGPreflightListenEventAccess()
  }

  /// Ask the OS to prompt the user, once.
  ///
  /// macOS shows the dialog only the first time per process identity; after
  /// that it silently returns the stored answer, and the user has to go to
  /// System Settings themselves.
  @discardableResult
  public static func requestPermission() -> Bool {
    CGRequestListenEventAccess()
  }

  /// Start observing. Returns once the tap is live on its own thread.
  public func start() throws {
    lock.lock()
    guard thread == nil else {
      lock.unlock()
      throw TapError.alreadyRunning
    }
    lock.unlock()

    guard Self.isPermitted else { throw TapError.notPermitted }

    let ready = DispatchSemaphore(value: 0)
    // `nonisolated(unsafe)` because the box is written once on the new thread
    // and read once here, with the semaphore as the barrier.
    nonisolated(unsafe) var startupError: TapError?

    let thread = Thread { [weak self] in
      guard let self else {
        ready.signal()
        return
      }
      let mask: CGEventMask =
        (1 << CGEventType.keyDown.rawValue)
        | (1 << CGEventType.keyUp.rawValue)
        | (1 << CGEventType.flagsChanged.rawValue)
        | (1 << CGEventType.mouseMoved.rawValue)
        | (1 << CGEventType.leftMouseDown.rawValue)
        | (1 << CGEventType.leftMouseUp.rawValue)
        | (1 << CGEventType.leftMouseDragged.rawValue)
        | (1 << CGEventType.rightMouseDown.rawValue)
        | (1 << CGEventType.rightMouseUp.rawValue)
        | (1 << CGEventType.rightMouseDragged.rawValue)
        | (1 << CGEventType.otherMouseDown.rawValue)
        | (1 << CGEventType.otherMouseUp.rawValue)
        | (1 << CGEventType.otherMouseDragged.rawValue)
        | (1 << CGEventType.scrollWheel.rawValue)

      guard
        let port = CGEvent.tapCreate(
          tap: .cgSessionEventTap,
          place: .headInsertEventTap,
          // `.defaultTap`, not `.listenOnly`: a listen-only tap cannot swallow
          // events, and swallowing is what a grab IS.
          options: .defaultTap,
          eventsOfInterest: mask,
          callback: tapCallback,
          userInfo: Unmanaged.passUnretained(self).toOpaque()
        )
      else {
        startupError = .notPermitted
        ready.signal()
        return
      }

      let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, port, 0)
      let loop = CFRunLoopGetCurrent()
      CFRunLoopAddSource(loop, source, .commonModes)
      CGEvent.tapEnable(tap: port, enable: true)

      self.lock.lock()
      self.machPort = port
      self.runLoop = loop
      self.lock.unlock()

      ready.signal()
      CFRunLoopRun()
    }
    thread.name = "monio.event-tap"
    // Above default so a busy app cannot starve the thread the OS is
    // synchronously waiting on. Not `.userInteractive`: this is not the main
    // thread and should not outrank it.
    thread.qualityOfService = .userInitiated

    lock.lock()
    self.thread = thread
    lock.unlock()
    thread.start()
    ready.wait()

    if let startupError {
      lock.lock()
      self.thread = nil
      lock.unlock()
      throw startupError
    }
  }

  public func stop() {
    lock.lock()
    let port = machPort
    let loop = runLoop
    machPort = nil
    runLoop = nil
    thread = nil
    lock.unlock()

    if let port {
      CGEvent.tapEnable(tap: port, enable: false)
    }
    if let loop {
      CFRunLoopStop(loop)
    }
  }

  deinit {
    stop()
  }

  /// Called from the tap thread, with the OS waiting.
  fileprivate func handle(type: CGEventType, event: CGEvent) -> Unmanaged<CGEvent>? {
    // The OS disables a tap that took too long, or that was interrupted. It
    // does NOT re-enable it, and a disabled tap looks exactly like a quiet
    // keyboard — so without this the head goes deaf, silently, forever.
    if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
      lock.lock()
      let port = machPort
      lock.unlock()
      if let port {
        CGEvent.tapEnable(tap: port, enable: true)
      }
      return Unmanaged.passUnretained(event)
    }

    guard let observed = Self.translate(type: type, event: event) else {
      return Unmanaged.passUnretained(event)
    }
    switch handler(observed) {
    case .pass:
      return Unmanaged.passUnretained(event)
    case .consume:
      return nil
    }
  }

  /// A CoreGraphics event in Monio's vocabulary, or `nil` for one that carries
  /// no input this library reports.
  static func translate(type: CGEventType, event: CGEvent) -> MonioEvent? {
    let origin = Provenance.origin(of: event)
    let location = event.location
    let delta = CGVector(
      dx: Double(event.getIntegerValueField(.mouseEventDeltaX)),
      dy: Double(event.getIntegerValueField(.mouseEventDeltaY))
    )
    let timestamp = UInt64(event.timestamp)

    func make(_ kind: MonioEvent.Kind, delta: CGVector?) -> MonioEvent {
      MonioEvent(
        kind: kind, location: location, delta: delta, origin: origin, timestamp: timestamp)
    }

    switch type {
    case .keyDown, .keyUp:
      let code = CGKeyCode(event.getIntegerValueField(.keyboardEventKeycode))
      return make(.key(MacKeyCodes.key(for: code), pressed: type == .keyDown), delta: nil)

    case .flagsChanged:
      // Modifiers do not produce keyDown/keyUp on macOS. Deriving press state
      // from the device-specific bit is what keeps left and right Shift
      // independent — see `ModifierFlags`.
      let code = CGKeyCode(event.getIntegerValueField(.keyboardEventKeycode))
      let key = MacKeyCodes.key(for: code)
      guard let pressed = ModifierFlags.isPressed(key: key, flags: event.flags.rawValue) else {
        return nil
      }
      return make(.key(key, pressed: pressed), delta: nil)

    case .mouseMoved, .leftMouseDragged, .rightMouseDragged, .otherMouseDragged:
      return make(.pointerMoved, delta: delta)

    case .leftMouseDown, .leftMouseUp:
      return make(.button(.left, pressed: type == .leftMouseDown), delta: nil)

    case .rightMouseDown, .rightMouseUp:
      return make(.button(.right, pressed: type == .rightMouseDown), delta: nil)

    case .otherMouseDown, .otherMouseUp:
      let number = UInt8(clamping: event.getIntegerValueField(.mouseEventButtonNumber) + 1)
      return make(
        .button(MonioButton(number: number), pressed: type == .otherMouseDown), delta: nil)

    case .scrollWheel:
      return make(
        .scroll(
          deltaX: Double(event.getIntegerValueField(.scrollWheelEventDeltaAxis2)),
          deltaY: Double(event.getIntegerValueField(.scrollWheelEventDeltaAxis1))
        ),
        delta: nil
      )

    default:
      return nil
    }
  }
}

/// A C function pointer, so it may capture nothing; the tap is recovered from
/// the refcon the tap was created with.
private func tapCallback(
  proxy: CGEventTapProxy,
  type: CGEventType,
  event: CGEvent,
  refcon: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
  guard let refcon else { return Unmanaged.passUnretained(event) }
  return Unmanaged<EventTap>.fromOpaque(refcon).takeUnretainedValue()
    .handle(type: type, event: event)
}
