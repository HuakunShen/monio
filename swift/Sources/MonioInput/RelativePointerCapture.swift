import CoreGraphics
import Foundation

/// Freezes the cursor so physical motion produces deltas and nothing else.
///
/// ## Why this exists
///
/// While this machine is driving another one, the local cursor must not move —
/// it would wander into local windows, hover things, and end up somewhere
/// unexpected when control comes back. But the *motion* is still needed, as
/// deltas, because that is what steers the remote pointer.
///
/// `CGAssociateMouseAndMouseCursorPosition(0)` is exactly that: the mouse keeps
/// reporting movement, the cursor stops following it.
///
/// Process-wide, because the OS state it changes is process-wide. Acquiring
/// twice is a no-op rather than an error, so a re-grab mid-session cannot leave
/// two leases that need two releases.
public final class RelativePointerCapture: @unchecked Sendable {
  public static let shared = RelativePointerCapture()

  private let lock = NSLock()
  private var savedCursorPosition: CGPoint?

  private init() {}

  public var isActive: Bool {
    lock.lock()
    defer { lock.unlock() }
    return savedCursorPosition != nil
  }

  public func begin() {
    lock.lock()
    defer { lock.unlock() }
    guard savedCursorPosition == nil else { return }
    // Saved BEFORE disassociating: afterwards the reported position stops
    // tracking the device, and restoring to it would put the cursor wherever it
    // happened to be frozen.
    savedCursorPosition = CGEvent(source: nil)?.location
    CGAssociateMouseAndMouseCursorPosition(0)
    CGDisplayHideCursor(CGMainDisplayID())
  }

  /// Restore normal cursor behaviour.
  ///
  /// Idempotent and deliberately total: every failure path in a head calls
  /// this, and one that threw would leave the user with an invisible, frozen
  /// cursor and no way to fix it short of logging out.
  public func end() {
    lock.lock()
    let restore = savedCursorPosition
    savedCursorPosition = nil
    lock.unlock()

    guard restore != nil else { return }
    CGAssociateMouseAndMouseCursorPosition(1)
    CGDisplayShowCursor(CGMainDisplayID())
    if let restore {
      CGWarpMouseCursorPosition(restore)
    }
  }
}
