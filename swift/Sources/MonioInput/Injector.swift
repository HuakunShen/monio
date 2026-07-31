import CoreGraphics
import Foundation

public enum InjectError: Error, CustomStringConvertible {
  case notPermitted
  case unrepresentableKey(MonioKey)
  case eventCreationFailed

  public var description: String {
    switch self {
    case .notPermitted:
      """
      This process is not trusted for Accessibility, so it cannot post input. \
      Grant it in System Settings > Privacy & Security > Accessibility.
      """
    case let .unrepresentableKey(key):
      "this Mac has no key code for \(key)"
    case .eventCreationFailed:
      "CoreGraphics refused to create the event"
    }
  }
}

/// Posts synthetic input, tagged so this process recognizes its own events.
///
/// Every event goes out through one `CGEventSource`, and every event is stamped
/// by `Provenance` before it is posted. Both matter: an untagged injection
/// comes straight back through this process's own tap and gets forwarded on,
/// which is a loop that never stops.
public final class Injector: @unchecked Sendable {
  private let source: CGEventSource?
  private let lock = NSLock()
  /// Modifier state has to be carried on every synthetic event, because
  /// CoreGraphics does not remember it for you: posting `shift down` then `a
  /// down` produces a lowercase `a` unless the second event also says shift is
  /// held.
  private var heldFlags: CGEventFlags = []

  public init() {
    // `.hidSystemState` rather than `.privateState`: private-state events do
    // not update the system's own modifier bookkeeping, so a Shift injected
    // that way is invisible to the app receiving the next keystroke.
    source = CGEventSource(stateID: .hidSystemState)
  }

  public static var isPermitted: Bool {
    CGPreflightPostEventAccess()
  }

  @discardableResult
  public static func requestPermission() -> Bool {
    CGRequestPostEventAccess()
  }

  public func key(_ key: MonioKey, pressed: Bool) throws {
    guard let code = MacKeyCodes.code(for: key) else {
      throw InjectError.unrepresentableKey(key)
    }
    lock.lock()
    if let mask = ModifierFlags.deviceMask(for: key) {
      // Track the general bit too — that is the one applications read.
      let general = Self.generalFlag(for: key)
      if pressed {
        heldFlags.insert(CGEventFlags(rawValue: mask))
        if let general { heldFlags.insert(general) }
      } else {
        heldFlags.remove(CGEventFlags(rawValue: mask))
        // The general bit only clears when NO device bit for that modifier is
        // still set. Clearing it on the first release turns "both shifts down,
        // let go of one" into "shift is up", and the far machine starts typing
        // in lowercase mid-word.
        if let general, !Self.anyDeviceBitSet(for: general, in: heldFlags) {
          heldFlags.remove(general)
        }
      }
    }
    let flags = heldFlags
    lock.unlock()

    guard let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: pressed)
    else {
      throw InjectError.eventCreationFailed
    }
    event.flags = flags
    post(event)
  }

  public func button(_ button: MonioButton, pressed: Bool, at location: CGPoint) throws {
    let type: CGEventType =
      switch button {
      case .left: pressed ? .leftMouseDown : .leftMouseUp
      case .right: pressed ? .rightMouseDown : .rightMouseUp
      default: pressed ? .otherMouseDown : .otherMouseUp
      }
    let cgButton: CGMouseButton =
      switch button {
      case .left: .left
      case .right: .right
      case .middle: .center
      case let .other(value): CGMouseButton(rawValue: UInt32(max(value, 1) - 1)) ?? .center
      }
    guard
      let event = CGEvent(
        mouseEventSource: source, mouseType: type, mouseCursorPosition: location,
        mouseButton: cgButton)
    else {
      throw InjectError.eventCreationFailed
    }
    lock.lock()
    event.flags = heldFlags
    lock.unlock()
    post(event)
  }

  /// Move the cursor to an absolute point.
  public func movePointer(to location: CGPoint) throws {
    guard
      let event = CGEvent(
        mouseEventSource: source, mouseType: .mouseMoved, mouseCursorPosition: location,
        mouseButton: .left)
    else {
      throw InjectError.eventCreationFailed
    }
    post(event)
  }

  public func scroll(deltaX: Double, deltaY: Double) throws {
    guard
      let event = CGEvent(
        scrollWheelEvent2Source: source, units: .pixel, wheelCount: 2,
        wheel1: Int32(clamping: Int(deltaY)), wheel2: Int32(clamping: Int(deltaX)), wheel3: 0)
    else {
      throw InjectError.eventCreationFailed
    }
    post(event)
  }

  /// Put the cursor somewhere without producing motion.
  ///
  /// Distinct from `movePointer` on purpose: a warp is how the cursor is
  /// repositioned when input arrives at this machine, and synthesizing motion
  /// for it would be recaptured and routed as if the user had moved the mouse.
  public func warpPointer(to location: CGPoint) {
    CGWarpMouseCursorPosition(location)
  }

  /// Forget any modifier state this injector believes it is holding.
  ///
  /// The caller is responsible for actually releasing the keys first; this only
  /// clears the flags that would otherwise be stamped onto the next event.
  public func resetModifiers() {
    lock.lock()
    heldFlags = []
    lock.unlock()
  }

  private func post(_ event: CGEvent) {
    Provenance.tag(event)
    event.post(tap: .cghidEventTap)
  }

  static func generalFlag(for key: MonioKey) -> CGEventFlags? {
    switch key {
    case .shiftLeft, .shiftRight: .maskShift
    case .controlLeft, .controlRight: .maskControl
    case .altLeft, .altRight: .maskAlternate
    case .metaLeft, .metaRight: .maskCommand
    case .capsLock: .maskAlphaShift
    default: nil
    }
  }

  /// Whether either device bit behind `general` is still set.
  static func anyDeviceBitSet(for general: CGEventFlags, in flags: CGEventFlags) -> Bool {
    let pair: [MonioKey] =
      switch general {
      case .maskShift: [.shiftLeft, .shiftRight]
      case .maskControl: [.controlLeft, .controlRight]
      case .maskAlternate: [.altLeft, .altRight]
      case .maskCommand: [.metaLeft, .metaRight]
      default: []
      }
    return pair.contains { key in
      guard let mask = ModifierFlags.deviceMask(for: key) else { return false }
      return flags.rawValue & mask != 0
    }
  }
}
