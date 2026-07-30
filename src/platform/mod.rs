//! Platform-specific implementations.

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", any(feature = "x11", feature = "evdev"))
))]
mod motion;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_env = "ohos")]
mod ohos;
#[cfg(target_env = "ohos")]
pub use ohos::*;

#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
mod linux;
#[cfg(all(target_os = "linux", not(target_env = "ohos")))]
pub use linux::*;

// Ensure at least one platform is supported
#[cfg(not(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_env = "ohos"
)))]
compile_error!("monio only supports macOS, Windows, Linux, and HarmonyOS");

#[cfg(all(test, not(target_env = "ohos")))]
#[path = "ohos/test_module.rs"]
mod ohos_test;
