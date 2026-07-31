import CoreGraphics

public enum MonioButton: Hashable, Sendable {
  case left, right, middle
  case other(UInt8)

  /// 1-indexed, matching the Rust crate and X11 convention.
  public var number: UInt8 {
    switch self {
    case .left: 1
    case .right: 2
    case .middle: 3
    case let .other(value): value
    }
  }

  public init(number: UInt8) {
    switch number {
    case 1: self = .left
    case 2: self = .right
    case 3: self = .middle
    default: self = .other(number)
    }
  }
}

/// One observed input event.
public struct MonioEvent: Sendable {
  public enum Kind: Sendable, Equatable {
    case key(MonioKey, pressed: Bool)
    case button(MonioButton, pressed: Bool)
    case pointerMoved
    case scroll(deltaX: Double, deltaY: Double)
  }

  public let kind: Kind
  /// Absolute position in the global display space. Meaningless while the
  /// pointer is captured — the cursor is frozen, so it reports the same point
  /// forever, which is exactly why `delta` exists.
  public let location: CGPoint
  /// Device motion, when the event carries it. Present on pointer events.
  public let delta: CGVector?
  public let origin: InputOrigin
  /// Mach absolute time, as CoreGraphics reports it.
  public let timestamp: UInt64
}

/// macOS reports modifier keys as `flagsChanged`, not as key down/up.
///
/// This matters more here than anywhere else in the codebase: modifiers are the
/// stuck-key problem. A head that treated `flagsChanged` as an unknown event
/// would send Shift-down to the far machine and never send its release, leaving
/// somebody's editor in capitals until they rebooted.
///
/// Pressed/released is derived from the **device-specific** flag bit for the
/// exact key that changed, not from the general mask. The general `.maskShift`
/// stays set while the *other* shift is still held, so using it would report a
/// release that has not happened.
enum ModifierFlags {
  /// Carbon's device-dependent modifier bits. Not exposed by CoreGraphics, so
  /// they are written out — they are stable ABI, unchanged since NeXT.
  static func deviceMask(for key: MonioKey) -> UInt64? {
    switch key {
    case .controlLeft: 0x0000_0001
    case .shiftLeft: 0x0000_0002
    case .shiftRight: 0x0000_0004
    case .metaLeft: 0x0000_0008
    case .metaRight: 0x0000_0010
    case .altLeft: 0x0000_0020
    case .altRight: 0x0000_0040
    case .controlRight: 0x0000_2000
    // Caps Lock has no left/right pair; its own general bit is exact.
    case .capsLock: UInt64(CGEventFlags.maskAlphaShift.rawValue)
    default: nil
    }
  }

  /// Whether `key` is held, given the flags on a `flagsChanged` event.
  static func isPressed(key: MonioKey, flags: UInt64) -> Bool? {
    guard let mask = deviceMask(for: key) else { return nil }
    return flags & mask != 0
  }
}
