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

@Suite("Injected pointer motion")
struct MotionTests {
  /// With nothing held, motion is a move. This is the only case where posting
  /// `mouseMoved` is right.
  @Test func idleMotionIsAMove() {
    let (type, button) = Injector.motion(heldButtons: [])
    #expect(type == .mouseMoved)
    #expect(button == .left)
  }

  /// The bug this pins: macOS does not turn a `mouseMoved` into a drag just
  /// because a button is down. An application watching `mouseDragged:` — text
  /// selection, dragging a file, moving a window — sees nothing at all, so the
  /// remote user watches the cursor travel and no drag happen.
  @Test func motionWhileHoldingAButtonIsADrag() {
    #expect(Injector.motion(heldButtons: [MonioButton.left.number]).0 == .leftMouseDragged)
    #expect(Injector.motion(heldButtons: [MonioButton.right.number]).0 == .rightMouseDragged)
    #expect(Injector.motion(heldButtons: [MonioButton.middle.number]).0 == .otherMouseDragged)
  }

  /// Only one type can be posted, so several held buttons need a defined
  /// winner rather than whatever the set happens to iterate first.
  @Test func leftWinsOverRightWinsOverOther() {
    let all: Set<UInt8> = [
      MonioButton.left.number, MonioButton.right.number, MonioButton.middle.number,
    ]
    #expect(Injector.motion(heldButtons: all).0 == .leftMouseDragged)
    #expect(
      Injector.motion(heldButtons: [MonioButton.right.number, MonioButton.middle.number]).0
        == .rightMouseDragged)
  }

  /// Buttons are 1-indexed in Monio's vocabulary and 0-indexed in
  /// CoreGraphics', and getting that wrong sends a side button as the middle
  /// one.
  @Test func otherButtonsKeepTheirIdentity() {
    #expect(Injector.motion(heldButtons: [MonioButton.middle.number]).1 == .center)
    #expect(Injector.motion(heldButtons: [4]).1.rawValue == 3)
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

  /// Point-to-display uses one definition of "contains" so that two machines
  /// cannot disagree about which pixel is the last one on a screen — a
  /// disagreement there is a pointer that crosses early on one side and late on
  /// the other.
  @Test func aPointResolvesToTheDisplayHoldingIt() {
    let left = MonioDisplay(
      id: 1, bounds: CGRect(x: 0, y: 0, width: 1440, height: 900), scale: 2, isPrimary: true)
    let right = MonioDisplay(
      id: 2, bounds: CGRect(x: 1440, y: 0, width: 1920, height: 1080), scale: 1,
      isPrimary: false)
    let screens = [left, right]

    #expect(MonioDisplays.display(at: CGPoint(x: 10, y: 10), in: screens) == left)
    #expect(MonioDisplays.display(at: CGPoint(x: 1500, y: 10), in: screens) == right)
    // The shared boundary belongs to exactly one of them: 1439 is the last
    // column of the left screen, 1440 the first of the right.
    #expect(MonioDisplays.display(at: CGPoint(x: 1439, y: 10), in: screens) == left)
    #expect(MonioDisplays.display(at: CGPoint(x: 1440, y: 10), in: screens) == right)
    // Off every screen is a real answer, not a fallback to the primary.
    #expect(MonioDisplays.display(at: CGPoint(x: -1, y: 10), in: screens) == nil)
    #expect(MonioDisplays.display(at: CGPoint(x: 10, y: 1000), in: screens) == nil)
  }
}
