//! Error types for the input hook library.

use thiserror::Error;

/// Result type alias for monio operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during input hooking operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Hook is already running.
    #[error("hook is already running")]
    AlreadyRunning,

    /// Hook is not running.
    #[error("hook is not running")]
    NotRunning,

    /// Failed to start the hook.
    #[error("failed to start hook: {0}")]
    HookStartFailed(String),

    /// Failed to stop the hook.
    #[error("failed to stop hook: {0}")]
    HookStopFailed(String),

    /// Failed to simulate an event.
    #[error("failed to simulate event: {0}")]
    SimulateFailed(String),

    /// Platform-specific error.
    #[error("platform error: {0}")]
    Platform(String),

    /// The operation requires elevated permissions.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Thread-related error.
    #[error("thread error: {0}")]
    ThreadError(String),

    /// The requested feature is not supported on this platform.
    #[error("not supported: {0}")]
    NotSupported(String),

    /// Other errors.
    #[error("{0}")]
    Other(String),
}
