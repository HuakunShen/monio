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
}
