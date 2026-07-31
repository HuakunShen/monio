import CoreGraphics
import Foundation

/// Where an observed event came from, as far as this process can actually
/// prove.
///
/// Read `unknown` carefully: it means **"not provably injected by this
/// process"**, not "physical". Other applications and virtual input drivers
/// synthesize untagged events, and nothing on macOS distinguishes a human hand
/// from a well-behaved program at this layer.
///
/// That limit is not a gap to be closed later — it is what the whole design
/// accounts for. Deciding *who is driving* is done by an explicit, ordered
/// source claim, never by inferring physical presence from `unknown`.
public enum InputOrigin: Sendable, Equatable {
  case unknown
  /// Provably posted by this process's current session: a matching random
  /// per-session tag AND a matching source PID.
  case selfInjected
}

/// The self-injection tag, mirroring the Rust crate's scheme byte for byte.
///
/// ## What this is for
///
/// Exactly one thing: breaking the feedback loop. When this machine is being
/// driven by another, it injects events, and its own tap sees them. Without a
/// way to recognize its own injections it would forward them back and they
/// would circle the topology forever.
///
/// ## Why both a tag and a PID
///
/// `EventSourceUserData` is a plain integer any process may set, so a tag alone
/// proves nothing — anyone can copy it. `EventSourceUnixProcessID` is filled in
/// by the OS and cannot be forged from user space. Requiring both means a
/// spoofed tag from another process still classifies as `unknown`, which is the
/// safe answer: at worst an event is treated as somebody else's.
///
/// This is a feedback-loop marker only. It is **never** an authentication or
/// authorization signal.
public enum Provenance {
  /// Random, non-zero, and generated once per process.
  ///
  /// Random rather than a constant so two Monio processes on one machine do not
  /// mistake each other's injections for their own; non-zero so an untagged
  /// event (which reads back as 0) can never match.
  public static let sessionTag: Int64 = {
    var tag: Int64 = 0
    while tag == 0 {
      var raw: UInt64 = 0
      _ = withUnsafeMutableBytes(of: &raw) { buffer in
        SecRandomCopyBytes(kSecRandomDefault, buffer.count, buffer.baseAddress!)
      }
      tag = Int64(bitPattern: raw & UInt64(Int64.max))
    }
    return tag
  }()

  /// Stamp an event this process is about to post.
  public static func tag(_ event: CGEvent) {
    event.setIntegerValueField(.eventSourceUserData, value: sessionTag)
  }

  /// Classify an observed event.
  public static func origin(of event: CGEvent) -> InputOrigin {
    classify(
      observedTag: event.getIntegerValueField(.eventSourceUserData),
      expectedTag: sessionTag,
      sourcePID: event.getIntegerValueField(.eventSourceUnixProcessID),
      ownPID: Int64(ProcessInfo.processInfo.processIdentifier)
    )
  }

  /// The decision itself, with no CoreGraphics in sight so it can be tested on
  /// any machine, with no permissions and no display server.
  ///
  /// Both halves must match. Either one alone is a false positive waiting to
  /// happen, and a false positive here is worse than a false negative: an event
  /// wrongly called `selfInjected` is **discarded**, so it would silently drop
  /// the user's real keystrokes.
  public static func classify(
    observedTag: Int64,
    expectedTag: Int64,
    sourcePID: Int64,
    ownPID: Int64
  ) -> InputOrigin {
    guard expectedTag != 0, observedTag == expectedTag, sourcePID == ownPID else {
      return .unknown
    }
    return .selfInjected
  }
}
