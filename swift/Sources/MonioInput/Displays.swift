import CoreGraphics

public struct MonioDisplay: Sendable, Equatable {
  /// `CGDirectDisplayID`, stable for as long as the display stays connected.
  public let id: UInt32
  /// Bounds in the global display space, in points. The primary display's
  /// origin is (0, 0) and others may be negative.
  public let bounds: CGRect
  /// Backing scale (2.0 on Retina). Reported so a caller can reason about
  /// pointer speed — never so it can rescale coordinates itself.
  public let scale: Double
  public let isPrimary: Bool
}

public enum MonioDisplays {
  /// Every active display.
  ///
  /// Needs **no** permission, unlike capture and injection — which makes it the
  /// one part of this library that is testable anywhere, including CI.
  public static func active() -> [MonioDisplay] {
    var count: UInt32 = 0
    guard CGGetActiveDisplayList(0, nil, &count) == .success, count > 0 else {
      return []
    }
    var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
    guard CGGetActiveDisplayList(count, &ids, &count) == .success else {
      return []
    }
    let main = CGMainDisplayID()
    return ids.prefix(Int(count)).map { id in
      let bounds = CGDisplayBounds(id)
      let mode = CGDisplayCopyDisplayMode(id)
      // `pixelWidth / width` is the honest way to get the backing factor:
      // there is no public "scale" on CGDisplay, and NSScreen is AppKit-only.
      let scale: Double =
        if let mode, mode.width > 0 {
          Double(mode.pixelWidth) / Double(mode.width)
        } else {
          1.0
        }
      return MonioDisplay(
        id: id,
        bounds: bounds,
        scale: scale,
        isPrimary: id == main
      )
    }
  }

  /// The display containing `point`, or `nil` if the point is off every screen.
  ///
  /// The lookup is done here rather than by a caller comparing rectangles
  /// because the answer decides which edge the pointer is leaving from, and two
  /// implementations of "contains" that disagree at the boundary produce a
  /// pointer that crosses one pixel early on one machine and one pixel late on
  /// the other.
  public static func display(at point: CGPoint, in displays: [MonioDisplay]? = nil)
    -> MonioDisplay?
  {
    (displays ?? active()).first { $0.bounds.contains(point) }
  }

  /// Where the cursor is now, in the global display space.
  ///
  /// Read from a fresh event rather than remembered, because anything can move
  /// the cursor — another application, a hot corner, the user's other hand.
  public static var pointerLocation: CGPoint? {
    CGEvent(source: nil)?.location
  }
}
