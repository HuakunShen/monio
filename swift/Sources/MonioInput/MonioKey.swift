import CoreGraphics

/// Monio's platform-neutral key vocabulary, mirroring the Rust crate's `Key`.
///
/// Positional, not textual: `keyQ` is the key where Q sits on US-QWERTY. Layout
/// and IME translation happen on whichever machine finally receives the event,
/// which is what lets a Chinese IME on the target work when the source is a
/// plain US keyboard.
public enum MonioKey: Hashable, Sendable {
  // Letters
  case keyA, keyB, keyC, keyD, keyE, keyF, keyG, keyH, keyI, keyJ, keyK, keyL, keyM
  case keyN, keyO, keyP, keyQ, keyR, keyS, keyT, keyU, keyV, keyW, keyX, keyY, keyZ
  // Top-row digits
  case num0, num1, num2, num3, num4, num5, num6, num7, num8, num9
  // Function keys
  case f1, f2, f3, f4, f5, f6, f7, f8, f9, f10, f11, f12
  case f13, f14, f15, f16, f17, f18, f19, f20, f21, f22, f23, f24
  // Modifiers. Left and right stay distinct: a session that collapsed them
  // could not release the one it did not press.
  case shiftLeft, shiftRight, controlLeft, controlRight
  case altLeft, altRight, metaLeft, metaRight
  // Editing and navigation
  case escape, tab, capsLock, space, enter, backspace, insert, delete
  case home, end, pageUp, pageDown
  case arrowUp, arrowDown, arrowLeft, arrowRight
  case numLock, scrollLock, printScreen, pause, contextMenu
  // Punctuation, positional
  case grave, minus, equal, bracketLeft, bracketRight, backslash
  case semicolon, quote, comma, period, slash
  case intlBackslash, intlYen, intlRo
  // Numpad
  case numpad0, numpad1, numpad2, numpad3, numpad4
  case numpad5, numpad6, numpad7, numpad8, numpad9
  case numpadAdd, numpadSubtract, numpadMultiply, numpadDivide
  case numpadDecimal, numpadEnter, numpadEqual
  // Media and browser
  case volumeUp, volumeDown, volumeMute
  case mediaPlayPause, mediaStop, mediaNext, mediaPrevious
  case browserBack, browserForward, browserRefresh, browserStop
  case browserSearch, browserFavorites, browserHome
  case launchMail, launchApp1, launchApp2
  /// A key this build has no neutral name for, carrying the platform scancode.
  case unknown(UInt32)
}

/// macOS virtual key codes, in both directions.
///
/// ## Why one table instead of two switches
///
/// The Rust crate keeps `keycode_to_key` and `key_to_keycode` as separate
/// matches, which can silently drift apart — and a drift here is invisible
/// until somebody types one letter and a different one arrives. Deriving both
/// directions from a single table makes that impossible, and the round-trip
/// test below proves it rather than assuming it.
///
/// The values themselves mirror the Rust crate's macOS table exactly, so a
/// Swift head and a Rust head on the same Mac agree key for key.
public enum MacKeyCodes {
  /// The authoritative pairs. Carbon `kVK_*` values.
  static let table: [(code: CGKeyCode, key: MonioKey)] = [
    // Letters, in Carbon's own order — which is not alphabetical, and that is
    // the point of writing them out rather than computing them.
    (0x00, .keyA), (0x01, .keyS), (0x02, .keyD), (0x03, .keyF), (0x04, .keyH),
    (0x05, .keyG), (0x06, .keyZ), (0x07, .keyX), (0x08, .keyC), (0x09, .keyV),
    (0x0B, .keyB), (0x0C, .keyQ), (0x0D, .keyW), (0x0E, .keyE), (0x0F, .keyR),
    (0x10, .keyY), (0x11, .keyT), (0x1F, .keyO), (0x20, .keyU), (0x22, .keyI),
    (0x23, .keyP), (0x25, .keyL), (0x26, .keyJ), (0x28, .keyK), (0x2D, .keyN),
    (0x2E, .keyM),

    // Top-row digits. Note 5 and 6 are transposed relative to their codes.
    (0x12, .num1), (0x13, .num2), (0x14, .num3), (0x15, .num4), (0x17, .num5),
    (0x16, .num6), (0x1A, .num7), (0x1C, .num8), (0x19, .num9), (0x1D, .num0),

    // Punctuation
    (0x18, .equal), (0x1B, .minus), (0x1E, .bracketRight), (0x21, .bracketLeft),
    (0x27, .quote), (0x29, .semicolon), (0x2A, .backslash), (0x2B, .comma),
    (0x2C, .slash), (0x2F, .period), (0x32, .grave),

    // Special
    (0x24, .enter), (0x30, .tab), (0x31, .space), (0x33, .backspace),
    (0x35, .escape),

    // Modifiers
    (0x36, .metaRight), (0x37, .metaLeft), (0x38, .shiftLeft), (0x39, .capsLock),
    (0x3A, .altLeft), (0x3B, .controlLeft), (0x3C, .shiftRight),
    (0x3D, .altRight), (0x3E, .controlRight),

    // Function keys, in code order rather than F-number order because that is
    // how Apple assigned them.
    (0x7A, .f1), (0x78, .f2), (0x63, .f3), (0x76, .f4), (0x60, .f5), (0x61, .f6),
    (0x62, .f7), (0x64, .f8), (0x65, .f9), (0x6D, .f10), (0x67, .f11),
    (0x6F, .f12), (0x69, .f13), (0x6B, .f14), (0x71, .f15), (0x6A, .f16),
    (0x40, .f17), (0x4F, .f18), (0x50, .f19), (0x5A, .f20),

    // Navigation
    (0x73, .home), (0x77, .end), (0x74, .pageUp), (0x79, .pageDown),
    (0x7B, .arrowLeft), (0x7C, .arrowRight), (0x7D, .arrowDown), (0x7E, .arrowUp),
    // Help doubles as Insert; forward-delete is Delete.
    (0x72, .insert), (0x75, .delete),

    // Numpad. 0x5A is F20, which is why 8 and 9 skip it.
    (0x52, .numpad0), (0x53, .numpad1), (0x54, .numpad2), (0x55, .numpad3),
    (0x56, .numpad4), (0x57, .numpad5), (0x58, .numpad6), (0x59, .numpad7),
    (0x5B, .numpad8), (0x5C, .numpad9),
    (0x41, .numpadDecimal), (0x43, .numpadMultiply), (0x45, .numpadAdd),
    (0x4B, .numpadDivide), (0x4C, .numpadEnter), (0x4E, .numpadSubtract),
    (0x51, .numpadEqual),
    // Clear, which is where a Mac keyboard puts Num Lock.
    (0x47, .numLock),

    // Media
    (0x48, .volumeUp), (0x49, .volumeDown), (0x4A, .volumeMute),
  ]

  private static let byCode: [CGKeyCode: MonioKey] = Dictionary(
    uniqueKeysWithValues: table.map { ($0.code, $0.key) }
  )

  private static let byKey: [MonioKey: CGKeyCode] = Dictionary(
    uniqueKeysWithValues: table.map { ($0.key, $0.code) }
  )

  /// The neutral key for a macOS virtual key code.
  ///
  /// Anything unmapped becomes `.unknown(code)` rather than being dropped: a
  /// key that works between two Macs must keep working even when this build has
  /// no name for it.
  public static func key(for code: CGKeyCode) -> MonioKey {
    byCode[code] ?? .unknown(UInt32(code))
  }

  /// The macOS virtual key code for a neutral key.
  ///
  /// `nil` for a key this Mac cannot express. Dropping it is the only safe
  /// answer — pressing an arbitrary substitute types something nobody asked
  /// for, and nothing would ever send its release.
  public static func code(for key: MonioKey) -> CGKeyCode? {
    if case let .unknown(raw) = key {
      return raw <= UInt32(UInt16.max) ? CGKeyCode(raw) : nil
    }
    return byKey[key]
  }
}
