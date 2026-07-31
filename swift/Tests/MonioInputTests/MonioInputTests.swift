import CoreGraphics
import Testing

@testable import MonioInput

/// Everything here runs with **no** permissions, no display server and no real
/// keyboard, so it works in CI and on a machine that has never granted
/// Accessibility.
///
/// What cannot be covered this way — does a tap really see a keystroke, does an
/// injection really land — is covered by `monio-selftest`, which needs a
/// permitted process and is run deliberately.
@Suite("Key codes")
struct KeyCodeTests {
  /// Every mapped key must survive a trip through the macOS code and back. The
  /// failure this catches is invisible until somebody types one letter and a
  /// different one arrives.
  @Test func everyMappedKeyRoundTrips() {
    for (code, key) in MacKeyCodes.table {
      #expect(MacKeyCodes.key(for: code) == key)
      #expect(MacKeyCodes.code(for: key) == code)
    }
  }

  /// The table is the single source of truth for both directions, so a
  /// duplicate on either side would silently shadow an entry.
  @Test func theTableHasNoDuplicates() {
    let codes = MacKeyCodes.table.map(\.code)
    let keys = MacKeyCodes.table.map(\.key)
    #expect(Set(codes).count == codes.count)
    #expect(Set(keys).count == keys.count)
  }

  /// A few values pinned by hand against Carbon's `kVK_*`, because a
  /// self-consistent table can still be uniformly wrong.
  @Test func carbonValuesArePinned() {
    #expect(MacKeyCodes.key(for: 0x00) == .keyA)
    #expect(MacKeyCodes.key(for: 0x0C) == .keyQ)
    #expect(MacKeyCodes.key(for: 0x24) == .enter)
    #expect(MacKeyCodes.key(for: 0x31) == .space)
    #expect(MacKeyCodes.key(for: 0x38) == .shiftLeft)
    #expect(MacKeyCodes.key(for: 0x3C) == .shiftRight)
    // 5 and 6 are transposed relative to their key codes on a Mac.
    #expect(MacKeyCodes.key(for: 0x17) == .num5)
    #expect(MacKeyCodes.key(for: 0x16) == .num6)
  }

  /// An unmapped code must come back tagged rather than be dropped, or a key
  /// that works between two Macs stops working the moment it is unnamed.
  @Test func anUnmappedCodeRoundTripsAsUnknown() {
    // 0x0A is ISO Section, which this table does not name.
    let key = MacKeyCodes.key(for: 0x0A)
    #expect(key == .unknown(0x0A))
    #expect(MacKeyCodes.code(for: key) == 0x0A)
  }
}

@Suite("Self-injection provenance")
struct ProvenanceTests {
  /// Both halves are required. Either alone is a false positive waiting to
  /// happen — and a false positive here DISCARDS the event, so it would drop
  /// the user's real keystrokes.
  @Test func bothTagAndPidMustMatch() {
    #expect(Provenance.classify(observedTag: 7, expectedTag: 7, sourcePID: 42, ownPID: 42) == .selfInjected)
    // Right tag, wrong process: another program copied the tag.
    #expect(Provenance.classify(observedTag: 7, expectedTag: 7, sourcePID: 99, ownPID: 42) == .unknown)
    // Right process, no tag: this process posted it some other way.
    #expect(Provenance.classify(observedTag: 0, expectedTag: 7, sourcePID: 42, ownPID: 42) == .unknown)
    #expect(Provenance.classify(observedTag: 8, expectedTag: 7, sourcePID: 42, ownPID: 42) == .unknown)
  }

  /// An untagged event reads back as 0, so a zero session tag would classify
  /// every physical keystroke on the machine as self-injected — and every one
  /// of them would then be silently swallowed.
  @Test func aZeroSessionTagNeverMatches() {
    #expect(Provenance.classify(observedTag: 0, expectedTag: 0, sourcePID: 42, ownPID: 42) == .unknown)
    #expect(Provenance.sessionTag != 0)
  }
}

@Suite("Modifier flags")
struct ModifierTests {
  /// macOS reports modifiers as `flagsChanged`, and the *general* mask stays
  /// set while the other side of the pair is still held. Using it would report
  /// a release that has not happened, and the far machine would start typing in
  /// lowercase mid-word.
  @Test func leftAndRightModifiersAreIndependent() {
    let leftDown = ModifierFlags.deviceMask(for: .shiftLeft)!
    let rightDown = ModifierFlags.deviceMask(for: .shiftRight)!
    #expect(leftDown != rightDown)

    let bothHeld = leftDown | rightDown
    #expect(ModifierFlags.isPressed(key: .shiftLeft, flags: bothHeld) == true)
    #expect(ModifierFlags.isPressed(key: .shiftRight, flags: bothHeld) == true)

    // Let go of the left one only.
    #expect(ModifierFlags.isPressed(key: .shiftLeft, flags: rightDown) == false)
    #expect(ModifierFlags.isPressed(key: .shiftRight, flags: rightDown) == true)
  }

  @Test func everyModifierPairHasDistinctBits() {
    let pairs: [(MonioKey, MonioKey)] = [
      (.shiftLeft, .shiftRight), (.controlLeft, .controlRight),
      (.altLeft, .altRight), (.metaLeft, .metaRight),
    ]
    var seen = Set<UInt64>()
    for (left, right) in pairs {
      let leftMask = ModifierFlags.deviceMask(for: left)!
      let rightMask = ModifierFlags.deviceMask(for: right)!
      #expect(leftMask != rightMask)
      #expect(seen.insert(leftMask).inserted)
      #expect(seen.insert(rightMask).inserted)
    }
  }

  @Test func anOrdinaryKeyIsNotAModifier() {
    #expect(ModifierFlags.deviceMask(for: .keyA) == nil)
    #expect(ModifierFlags.isPressed(key: .keyA, flags: .max) == nil)
  }

  /// The general bit must survive one half of a pair being released, or an app
  /// receiving the next keystroke thinks Shift is up while the user is still
  /// holding it.
  @Test func theGeneralBitClearsOnlyWhenBothSidesAreUp() {
    let left = CGEventFlags(rawValue: ModifierFlags.deviceMask(for: .shiftLeft)!)
    let right = CGEventFlags(rawValue: ModifierFlags.deviceMask(for: .shiftRight)!)
    #expect(Injector.anyDeviceBitSet(for: .maskShift, in: left.union(right)))
    #expect(Injector.anyDeviceBitSet(for: .maskShift, in: right))
    #expect(!Injector.anyDeviceBitSet(for: .maskShift, in: []))
  }
}

@Suite("Displays")
struct DisplayTests {
  /// Display enumeration needs no permission, which makes it the one OS call
  /// here that a plain `swift test` can make for real.
  @Test func activeDisplaysAreCoherent() {
    let displays = MonioDisplays.active()
    guard !displays.isEmpty else {
      // A headless CI runner genuinely has none. That is not a failure.
      return
    }
    #expect(displays.filter(\.isPrimary).count == 1)
    for display in displays {
      #expect(display.bounds.width > 0)
      #expect(display.bounds.height > 0)
      #expect(display.scale >= 1.0)
    }
    #expect(Set(displays.map(\.id)).count == displays.count)
  }
}
