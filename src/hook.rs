//! Main Hook struct and EventHandler trait.

use crate::error::{Error, Result};
use crate::event::Event;
use crate::platform;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

/// Trait for handling input events (listen-only mode).
///
/// Implement this trait to receive events from the hook.
/// Events are passed through to other applications.
pub trait EventHandler: Send + Sync {
    /// Called when an input event occurs.
    fn handle_event(&self, event: &Event);
}

/// Implement EventHandler for closures.
impl<F> EventHandler for F
where
    F: Fn(&Event) + Send + Sync,
{
    fn handle_event(&self, event: &Event) {
        self(event);
    }
}

/// Trait for handling input events with grab capability.
///
/// Implement this trait to intercept and optionally consume events.
/// Return `None` to consume the event (prevent it from reaching other apps).
/// Return `Some(event)` to pass the event through.
///
/// # Platform Support
///
/// - **macOS**: Full support via CGEventTap
/// - **Windows**: Full support via low-level hooks
/// - **Linux/X11**: Not supported (XRecord is listen-only). Falls back to listen mode.
///
/// # Example
///
/// ```no_run
/// use monio::{grab, Event, EventType, Key};
///
/// grab(|event: &Event| {
///     // Block the 'A' key
///     if event.event_type == EventType::KeyPressed {
///         if let Some(kb) = &event.keyboard {
///             if kb.key == Key::KeyA {
///                 return None; // Consume the event
///             }
///         }
///     }
///     Some(event.clone()) // Pass through
/// }).expect("Failed to start grab");
/// ```
pub trait GrabHandler: Send + Sync {
    /// Called when an input event occurs.
    ///
    /// Return `None` to consume the event, `Some(event)` to pass it through.
    fn handle_event(&self, event: &Event) -> Option<Event>;
}

/// Implement GrabHandler for closures.
impl<F> GrabHandler for F
where
    F: Fn(&Event) -> Option<Event> + Send + Sync,
{
    fn handle_event(&self, event: &Event) -> Option<Event> {
        self(event)
    }
}

/// Input hook that captures keyboard and mouse events.
pub struct Hook {
    running: Arc<AtomicBool>,
    thread_handle: RwLock<Option<JoinHandle<()>>>,
}

impl Default for Hook {
    fn default() -> Self {
        Self::new()
    }
}

impl Hook {
    /// Create a new Hook instance.
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            thread_handle: RwLock::new(None),
        }
    }

    /// Start listening for events (blocking, listen-only mode).
    ///
    /// This will block the current thread until `stop()` is called
    /// from another thread. Events are passed through to other applications.
    pub fn run<H: EventHandler + 'static>(&self, handler: H) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        // Reset state before starting
        crate::state::reset_mask();

        let result = platform::run_hook(&self.running, handler);

        self.running.store(false, Ordering::SeqCst);
        result
    }

    /// Start listening in a background thread (non-blocking, listen-only mode).
    ///
    /// Returns immediately. Use `stop()` to terminate the hook.
    /// Events are passed through to other applications.
    pub fn run_async<H: EventHandler + 'static>(&self, handler: H) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        // Reset state before starting
        crate::state::reset_mask();

        let running = self.running.clone();
        let handle = std::thread::spawn(move || {
            let _ = platform::run_hook(&running, handler);
            running.store(false, Ordering::SeqCst);
        });

        *self.thread_handle.write().unwrap() = Some(handle);
        Ok(())
    }

    /// Start grabbing events (blocking, can consume events).
    ///
    /// This will block the current thread until `stop()` is called.
    /// The handler can return `None` to consume events (prevent them from
    /// reaching other applications) or `Some(event)` to pass them through.
    ///
    /// # Platform Support
    ///
    /// - **macOS**: Full support
    /// - **Windows**: Full support
    /// - **Linux/X11**: Falls back to listen mode (XRecord cannot grab)
    pub fn grab<H: GrabHandler + 'static>(&self, handler: H) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        // Reset state before starting
        crate::state::reset_mask();

        let result = platform::run_grab_hook(&self.running, handler);

        self.running.store(false, Ordering::SeqCst);
        result
    }

    /// Start grabbing events in a background thread (non-blocking).
    ///
    /// Returns immediately. Use `stop()` to terminate the hook.
    /// The handler can return `None` to consume events.
    pub fn grab_async<H: GrabHandler + 'static>(&self, handler: H) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        // Reset state before starting
        crate::state::reset_mask();

        let running = self.running.clone();
        let handle = std::thread::spawn(move || {
            let _ = platform::run_grab_hook(&running, handler);
            running.store(false, Ordering::SeqCst);
        });

        *self.thread_handle.write().unwrap() = Some(handle);
        Ok(())
    }

    /// Stop the hook.
    pub fn stop(&self) -> Result<()> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Err(Error::NotRunning);
        }

        platform::stop_hook()?;

        // Wait for the thread to finish if running async
        if let Some(handle) = self.thread_handle.write().unwrap().take() {
            handle
                .join()
                .map_err(|_| Error::ThreadError("failed to join hook thread".into()))?;
        }

        Ok(())
    }

    /// Check if the hook is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        if self.is_running() {
            let _ = self.stop();
        }
    }
}

/// Convenience function to start listening for events.
///
/// This is a simpler alternative to creating a Hook instance.
/// Blocks until the hook is stopped externally or an error occurs.
///
/// # Example
///
/// ```no_run
/// use monio::{listen, Event, EventType};
///
/// listen(|event: &Event| {
///     match event.event_type {
///         EventType::MouseDragged => {
///             if let Some(mouse) = &event.mouse {
///                 println!("Dragging at ({}, {})", mouse.x, mouse.y);
///             }
///         }
///         EventType::KeyPressed => {
///             if let Some(kb) = &event.keyboard {
///                 println!("Key pressed: {:?}", kb.key);
///             }
///         }
///         _ => {}
///     }
/// }).expect("Failed to start hook");
/// ```
pub fn listen<F>(callback: F) -> Result<()>
where
    F: Fn(&Event) + Send + Sync + 'static,
{
    let hook = Hook::new();
    hook.run(callback)
}

/// Convenience function to start grabbing events with the ability to consume them.
///
/// Return `None` from the callback to consume the event (prevent it from reaching other apps).
/// Return `Some(event)` to pass the event through.
///
/// # Platform Support
///
/// - **macOS**: Full support via CGEventTap
/// - **Windows**: Full support via low-level hooks
/// - **Linux/X11**: Falls back to listen mode (XRecord cannot grab)
///
/// # Example
///
/// ```no_run
/// use monio::{grab, Event, EventType, Key};
///
/// grab(|event: &Event| {
///     // Block the Escape key
///     if event.event_type == EventType::KeyPressed {
///         if let Some(kb) = &event.keyboard {
///             if kb.key == Key::Escape {
///                 println!("Blocked Escape key!");
///                 return None;
///             }
///         }
///     }
///     Some(event.clone())
/// }).expect("Failed to start grab");
/// ```
pub fn grab<F>(callback: F) -> Result<()>
where
    F: Fn(&Event) -> Option<Event> + Send + Sync + 'static,
{
    let hook = Hook::new();
    hook.grab(callback)
}
