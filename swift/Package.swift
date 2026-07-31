// swift-tools-version: 6.0
import PackageDescription

// Monio's native Apple input layer.
//
// A sibling of the Rust crate rather than a binding to it: capture and
// injection on Apple platforms need a CFRunLoop, a signed bundle identity for
// TCC, and CoreGraphics types that are far more natural from Swift. The two
// implementations share a *contract* — the same neutral key codes, the same
// self-injection provenance scheme — not code.
let package = Package(
  name: "MonioInput",
  platforms: [.macOS(.v13)],
  products: [
    .library(name: "MonioInput", targets: ["MonioInput"]),
    // The equivalent of the Rust crate's `examples/`: run them by hand on a
    // machine that has Accessibility, because that is the only way to observe
    // what the OS actually does.
    .executable(name: "monio-capture-demo", targets: ["MonioCaptureDemo"]),
    .executable(name: "monio-selftest", targets: ["MonioSelfTest"]),
  ],
  targets: [
    .target(name: "MonioInput"),
    .executableTarget(name: "MonioCaptureDemo", dependencies: ["MonioInput"]),
    .executableTarget(name: "MonioSelfTest", dependencies: ["MonioInput"]),
    .testTarget(name: "MonioInputTests", dependencies: ["MonioInput"]),
  ]
)
