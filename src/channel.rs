//! Channel-based event receiving for non-blocking event processing.
//!
//! This module provides channel-based alternatives to callback-based hooks,
//! allowing you to receive events in the background and process them
//! asynchronously in your main application.
//!
//! # Example (Sync)
//!
//! ```no_run
//! use monio::channel::listen_channel;
//! use std::time::Duration;
//!
//! let (handle, rx) = listen_channel(100).expect("Failed to start hook");
//!
//! // Process events in a loop
//! loop {
//!     match rx.recv_timeout(Duration::from_millis(100)) {
//!         Ok(event) => println!("{:?}", event.event_type),
//!         Err(_) => {
//!             // Timeout - do other work or check exit condition
//!         }
//!     }
//! }
//!
//! // Stop when done
//! handle.stop().unwrap();
//! ```
//!
//! # Example (Async with Tokio)
//!
//! ```ignore
//! use monio::channel::listen_async_channel;
//!
//! #[tokio::main]
//! async fn main() {
//!     let (handle, mut rx) = listen_async_channel(100).expect("Failed to start hook");
//!
//!     while let Some(event) = rx.recv().await {
//!         println!("{:?}", event.event_type);
//!     }
//! }
//! ```

use crate::error::{Error, Result};
use crate::event::Event;
use crate::hook::{EventHandler, GrabHandler};
use crate::platform;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};

/// Handle to control a channel-based hook.
///
/// Use this to stop the hook when you're done receiving events.
/// The hook will also stop automatically when this handle is dropped.
pub struct ChannelHookHandle {
    running: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl ChannelHookHandle {
    /// Stop the hook and wait for the background thread to finish.
    pub fn stop(mut self) -> Result<()> {
        self.stop_inner()
    }

    /// Check if the hook is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn stop_inner(&mut self) -> Result<()> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Ok(()); // Already stopped
        }

        platform::stop_hook()?;

        if let Some(handle) = self.thread_handle.take() {
            handle
                .join()
                .map_err(|_| Error::ThreadError("failed to join hook thread".into()))?;
        }

        Ok(())
    }
}

impl Drop for ChannelHookHandle {
    fn drop(&mut self) {
        let _ = self.stop_inner();
    }
}

/// Handler that sends events to a bounded sync channel.
struct ChannelHandler {
    sender: SyncSender<Event>,
}

impl EventHandler for ChannelHandler {
    fn handle_event(&self, event: &Event) {
        // Try to send, but don't block if the channel is full
        // This prevents the hook from blocking input if the consumer is slow
        let _ = self.sender.try_send(event.clone());
    }
}

/// Handler that sends events to an unbounded sync channel.
struct UnboundedChannelHandler {
    sender: Sender<Event>,
}

impl EventHandler for UnboundedChannelHandler {
    fn handle_event(&self, event: &Event) {
        let _ = self.sender.send(event.clone());
    }
}

/// Start a hook that sends events to a bounded channel.
///
/// Returns a handle to control the hook and a receiver for events.
/// The hook runs in a background thread.
///
/// # Arguments
///
/// * `capacity` - Maximum number of events to buffer. If the buffer is full,
///   new events are dropped to prevent blocking input.
///
/// # Example
///
/// ```no_run
/// use monio::channel::listen_channel;
///
/// let (handle, rx) = listen_channel(100).expect("Failed to start hook");
///
/// for event in rx.iter() {
///     println!("{:?}", event.event_type);
/// }
/// ```
pub fn listen_channel(capacity: usize) -> Result<(ChannelHookHandle, Receiver<Event>)> {
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Reset state before starting
    crate::state::reset_mask();

    let thread_handle = thread::spawn(move || {
        let handler = ChannelHandler { sender };
        let _ = platform::run_hook(&running_clone, handler);
        running_clone.store(false, Ordering::SeqCst);
    });

    let handle = ChannelHookHandle {
        running,
        thread_handle: Some(thread_handle),
    };

    Ok((handle, receiver))
}

/// Start a hook that sends events to an unbounded channel.
///
/// Similar to `listen_channel`, but uses an unbounded channel.
/// Use this if you need to ensure no events are dropped, but be careful
/// of memory usage if the consumer is slow.
///
/// # Example
///
/// ```no_run
/// use monio::channel::listen_unbounded_channel;
///
/// let (handle, rx) = listen_unbounded_channel().expect("Failed to start hook");
///
/// for event in rx.iter() {
///     println!("{:?}", event.event_type);
/// }
/// ```
pub fn listen_unbounded_channel() -> Result<(ChannelHookHandle, Receiver<Event>)> {
    let (sender, receiver) = mpsc::channel();
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Reset state before starting
    crate::state::reset_mask();

    let thread_handle = thread::spawn(move || {
        let handler = UnboundedChannelHandler { sender };
        let _ = platform::run_hook(&running_clone, handler);
        running_clone.store(false, Ordering::SeqCst);
    });

    let handle = ChannelHookHandle {
        running,
        thread_handle: Some(thread_handle),
    };

    Ok((handle, receiver))
}

/// Handler for grab mode with a filter function and channel.
struct GrabChannelHandler<F>
where
    F: Fn(&Event) -> bool + Send + Sync,
{
    sender: SyncSender<Event>,
    filter: F,
}

impl<F> GrabHandler for GrabChannelHandler<F>
where
    F: Fn(&Event) -> bool + Send + Sync,
{
    fn handle_event(&self, event: &Event) -> Option<Event> {
        // Send event to channel regardless of filter result
        let _ = self.sender.try_send(event.clone());

        // Filter decides whether to pass through or consume
        if (self.filter)(event) {
            Some(event.clone())
        } else {
            None // Consume the event
        }
    }
}

/// Start a grab hook that sends events to a channel with a sync filter.
///
/// The filter function is called synchronously for each event and must decide
/// immediately whether to pass the event through (`true`) or consume it (`false`).
/// All events (whether consumed or not) are sent to the channel.
///
/// # Arguments
///
/// * `capacity` - Maximum number of events to buffer
/// * `filter` - Function that returns `true` to pass event through, `false` to consume
///
/// # Example
///
/// ```no_run
/// use monio::channel::grab_channel;
/// use monio::{EventType, Key};
///
/// // Block F1 key, pass everything else through
/// let (handle, rx) = grab_channel(100, |event| {
///     if event.event_type == EventType::KeyPressed {
///         if let Some(kb) = &event.keyboard {
///             if kb.key == Key::F1 {
///                 return false; // Consume F1
///             }
///         }
///     }
///     true // Pass through
/// }).expect("Failed to start hook");
///
/// for event in rx.iter() {
///     println!("{:?}", event.event_type);
/// }
/// ```
pub fn grab_channel<F>(capacity: usize, filter: F) -> Result<(ChannelHookHandle, Receiver<Event>)>
where
    F: Fn(&Event) -> bool + Send + Sync + 'static,
{
    let (sender, receiver) = mpsc::sync_channel(capacity);
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Reset state before starting
    crate::state::reset_mask();

    let thread_handle = thread::spawn(move || {
        let handler = GrabChannelHandler { sender, filter };
        let _ = platform::run_grab_hook(&running_clone, handler);
        running_clone.store(false, Ordering::SeqCst);
    });

    let handle = ChannelHookHandle {
        running,
        thread_handle: Some(thread_handle),
    };

    Ok((handle, receiver))
}

// ============================================================================
// Tokio async support (behind feature flag)
// ============================================================================

#[cfg(feature = "tokio")]
pub use tokio_channel::*;

#[cfg(feature = "tokio")]
mod tokio_channel {
    use super::*;
    use tokio::sync::mpsc as tokio_mpsc;

    /// Handler that sends events to a tokio async channel.
    struct TokioChannelHandler {
        sender: tokio_mpsc::Sender<Event>,
    }

    impl EventHandler for TokioChannelHandler {
        fn handle_event(&self, event: &Event) {
            // Use try_send to avoid blocking the hook thread
            let _ = self.sender.try_send(event.clone());
        }
    }

    /// Start a hook that sends events to a tokio async channel.
    ///
    /// Returns a handle to control the hook and an async receiver for events.
    /// The hook runs in a background thread.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events to buffer
    ///
    /// # Example
    ///
    /// ```ignore
    /// use monio::channel::listen_async_channel;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (handle, mut rx) = listen_async_channel(100).expect("Failed to start hook");
    ///
    ///     while let Some(event) = rx.recv().await {
    ///         println!("{:?}", event.event_type);
    ///     }
    /// }
    /// ```
    pub fn listen_async_channel(
        capacity: usize,
    ) -> Result<(ChannelHookHandle, tokio_mpsc::Receiver<Event>)> {
        let (sender, receiver) = tokio_mpsc::channel(capacity);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // Reset state before starting
        crate::state::reset_mask();

        let thread_handle = thread::spawn(move || {
            let handler = TokioChannelHandler { sender };
            let _ = platform::run_hook(&running_clone, handler);
            running_clone.store(false, Ordering::SeqCst);
        });

        let handle = ChannelHookHandle {
            running,
            thread_handle: Some(thread_handle),
        };

        Ok((handle, receiver))
    }

    /// Handler for grab mode with tokio channel.
    struct TokioGrabChannelHandler<F>
    where
        F: Fn(&Event) -> bool + Send + Sync,
    {
        sender: tokio_mpsc::Sender<Event>,
        filter: F,
    }

    impl<F> GrabHandler for TokioGrabChannelHandler<F>
    where
        F: Fn(&Event) -> bool + Send + Sync,
    {
        fn handle_event(&self, event: &Event) -> Option<Event> {
            let _ = self.sender.try_send(event.clone());

            if (self.filter)(event) {
                Some(event.clone())
            } else {
                None
            }
        }
    }

    /// Start a grab hook that sends events to a tokio async channel.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events to buffer
    /// * `filter` - Sync function that returns `true` to pass event through, `false` to consume
    ///
    /// # Example
    ///
    /// ```ignore
    /// use monio::channel::grab_async_channel;
    /// use monio::{EventType, Key};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (handle, mut rx) = grab_async_channel(100, |event| {
    ///         // Block F1 key
    ///         if event.event_type == EventType::KeyPressed {
    ///             if let Some(kb) = &event.keyboard {
    ///                 if kb.key == Key::F1 {
    ///                     return false;
    ///                 }
    ///             }
    ///         }
    ///         true
    ///     }).expect("Failed to start hook");
    ///
    ///     while let Some(event) = rx.recv().await {
    ///         println!("{:?}", event.event_type);
    ///     }
    /// }
    /// ```
    pub fn grab_async_channel<F>(
        capacity: usize,
        filter: F,
    ) -> Result<(ChannelHookHandle, tokio_mpsc::Receiver<Event>)>
    where
        F: Fn(&Event) -> bool + Send + Sync + 'static,
    {
        let (sender, receiver) = tokio_mpsc::channel(capacity);
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // Reset state before starting
        crate::state::reset_mask();

        let thread_handle = thread::spawn(move || {
            let handler = TokioGrabChannelHandler { sender, filter };
            let _ = platform::run_grab_hook(&running_clone, handler);
            running_clone.store(false, Ordering::SeqCst);
        });

        let handle = ChannelHookHandle {
            running,
            thread_handle: Some(thread_handle),
        };

        Ok((handle, receiver))
    }
}
