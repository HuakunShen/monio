//! Platform-specific implementations.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

// Ensure at least one platform is supported
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
compile_error!("monio only supports macOS, Windows, and Linux");
