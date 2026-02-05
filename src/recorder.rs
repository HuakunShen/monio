//! Event recording and playback for automation and macro scripts.
//!
//! This module provides functionality to record user input events with timestamps
//! and replay them later. Useful for:
//! - Automated testing
//! - Macro scripts
//! - Operation tutorials
//!
//! # Example
//!
//! ```no_run
//! use monio::recorder::{EventRecorder, Recording};
//! use std::time::Duration;
//!
//! // Record events
//! let mut recorder = EventRecorder::new();
//! recorder.start_recording().unwrap();
//!
//! // ... user performs actions ...
//!
//! let recording = recorder.stop_recording().unwrap();
//! recording.save("macro.json").unwrap();
//!
//! // Playback later
//! let recording = Recording::load("macro.json").unwrap();
//! recording.playback().unwrap();
//! ```

use crate::Hook;
use crate::error::{Error, Result};
use crate::event::{Event, EventType};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// A recorded event with its timestamp relative to recording start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// Time elapsed since recording start.
    pub elapsed: Duration,
    /// The event that occurred.
    pub event: Event,
}

/// A complete recording of user input events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    /// Recorded events with timestamps.
    pub events: Vec<RecordedEvent>,
    /// When the recording was created.
    pub created_at: SystemTime,
    /// Optional description.
    pub description: Option<String>,
}

impl Recording {
    /// Create a new empty recording.
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            created_at: SystemTime::now(),
            description: None,
        }
    }

    /// Set a description for this recording.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Get the duration of this recording.
    pub fn duration(&self) -> Duration {
        self.events
            .last()
            .map(|e| e.elapsed)
            .unwrap_or(Duration::ZERO)
    }

    /// Get the number of events in this recording.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Save the recording to a file (JSON format).
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Other(format!("Failed to serialize recording: {}", e)))?;
        std::fs::write(path, json)
            .map_err(|e| Error::Other(format!("Failed to write recording file: {}", e)))?;
        Ok(())
    }

    /// Load a recording from a file (JSON format).
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| Error::Other(format!("Failed to read recording file: {}", e)))?;
        let recording: Recording = serde_json::from_str(&json)
            .map_err(|e| Error::Other(format!("Failed to deserialize recording: {}", e)))?;
        Ok(recording)
    }

    /// Playback this recording, simulating all recorded events.
    ///
    /// Events are replayed with their original timing intervals.
    pub fn playback(&self) -> Result<()> {
        self.playback_with_speed(1.0)
    }

    /// Playback this recording with a speed multiplier.
    ///
    /// # Arguments
    ///
    /// * `speed` - Speed multiplier (1.0 = normal speed, 2.0 = double speed, 0.5 = half speed)
    ///
    /// # Example
    ///
    /// ```no_run
    /// use monio::recorder::Recording;
    ///
    /// let recording = Recording::load("macro.json").unwrap();
    /// // Playback at 2x speed
    /// recording.playback_with_speed(2.0).unwrap();
    /// ```
    pub fn playback_with_speed(&self, speed: f64) -> Result<()> {
        if speed <= 0.0 {
            return Err(Error::Other("Playback speed must be positive".into()));
        }

        if self.events.is_empty() {
            return Ok(());
        }

        let start = Instant::now();
        let mut _last_elapsed = Duration::ZERO;

        for recorded in &self.events {
            // Skip HookEnabled/HookDisabled events during playback
            match recorded.event.event_type {
                EventType::HookEnabled | EventType::HookDisabled => continue,
                _ => {}
            }

            // Calculate target time with speed adjustment
            let target_elapsed = recorded.elapsed.as_secs_f64() / speed;
            let target_duration = Duration::from_secs_f64(target_elapsed);

            // Wait until it's time for this event
            let elapsed = start.elapsed();
            if target_duration > elapsed {
                std::thread::sleep(target_duration - elapsed);
            }

            // Simulate the event
            crate::platform::simulate(&recorded.event)?;

            _last_elapsed = recorded.elapsed;
        }

        Ok(())
    }

    /// Playback without timing (as fast as possible).
    pub fn playback_fast(&self) -> Result<()> {
        for recorded in &self.events {
            match recorded.event.event_type {
                EventType::HookEnabled | EventType::HookDisabled => continue,
                _ => {}
            }
            crate::platform::simulate(&recorded.event)?;
        }
        Ok(())
    }
}

impl Default for Recording {
    fn default() -> Self {
        Self::new()
    }
}

/// Records user input events for later playback.
pub struct EventRecorder {
    recording: Arc<Mutex<Option<Recording>>>,
    start_time: Arc<Mutex<Option<Instant>>>,
    hook: Option<Hook>,
    running: Arc<AtomicBool>,
}

impl EventRecorder {
    /// Create a new event recorder.
    pub fn new() -> Self {
        Self {
            recording: Arc::new(Mutex::new(None)),
            start_time: Arc::new(Mutex::new(None)),
            hook: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start recording events.
    ///
    /// This starts a background hook that captures all input events.
    /// Call `stop_recording()` to finish and get the recording.
    pub fn start_recording(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let recording = self.recording.clone();
        let start_time = self.start_time.clone();
        let running = self.running.clone();

        // Initialize recording
        {
            let mut rec = recording
                .lock()
                .map_err(|_| Error::ThreadError("recording mutex poisoned".into()))?;
            *rec = Some(Recording::new());
        }
        {
            let mut time = start_time
                .lock()
                .map_err(|_| Error::ThreadError("time mutex poisoned".into()))?;
            *time = Some(Instant::now());
        }

        // Create hook
        let hook = Hook::new();

        // Start recording in background
        hook.run_async(move |event: &Event| {
            if !running.load(Ordering::SeqCst) {
                return;
            }

            // Skip hook lifecycle events in recording
            match event.event_type {
                EventType::HookEnabled | EventType::HookDisabled => return,
                _ => {}
            }

            let elapsed = {
                let time = start_time.lock();
                match time {
                    Ok(t) => t.map(|instant| instant.elapsed()).unwrap_or(Duration::ZERO),
                    Err(_) => return, // Mutex poisoned, skip this event
                }
            };

            let recorded = RecordedEvent {
                elapsed,
                event: event.clone(),
            };

            if let Ok(ref mut r) = recording.lock()
                && let Some(ref mut rec) = **r
            {
                rec.events.push(recorded);
            }
        })?;

        // Only set running flag after hook is successfully started
        self.running.store(true, Ordering::SeqCst);
        self.hook = Some(hook);
        Ok(())
    }

    /// Stop recording and return the recording.
    pub fn stop_recording(&mut self) -> Result<Recording> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Err(Error::NotRunning);
        }

        // Stop the hook
        if let Some(hook) = self.hook.take() {
            hook.stop()?;
        }

        // Return the recording
        let mut rec = self
            .recording
            .lock()
            .map_err(|_| Error::ThreadError("recording mutex poisoned".into()))?;
        rec.take()
            .ok_or_else(|| Error::Other("No recording available".into()))
    }

    /// Check if currently recording.
    pub fn is_recording(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Record for a specific duration.
    ///
    /// Convenience method that starts recording, waits for the specified duration,
    /// then stops and returns the recording.
    pub fn record_for(duration: Duration) -> Result<Recording> {
        let mut recorder = Self::new();
        recorder.start_recording()?;
        std::thread::sleep(duration);
        recorder.stop_recording()
    }
}

impl Default for EventRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_new() {
        let recording = Recording::new();
        assert!(recording.events.is_empty());
        assert_eq!(recording.duration(), Duration::ZERO);
        assert_eq!(recording.event_count(), 0);
    }

    #[test]
    fn test_recording_with_description() {
        let recording = Recording::new().with_description("Test macro");
        assert_eq!(recording.description, Some("Test macro".to_string()));
    }

    #[test]
    fn test_recording_duration() {
        let mut recording = Recording::new();
        recording.events.push(RecordedEvent {
            elapsed: Duration::from_secs(5),
            event: Event::new(EventType::KeyPressed),
        });
        assert_eq!(recording.duration(), Duration::from_secs(5));
    }

    #[test]
    fn test_save_load_roundtrip() {
        let mut recording = Recording::new().with_description("Test");
        recording.events.push(RecordedEvent {
            elapsed: Duration::from_millis(100),
            event: Event::key_pressed(crate::Key::KeyA, 30),
        });

        let temp_path = std::env::temp_dir().join("monio_test_recording.json");
        recording.save(&temp_path).unwrap();

        let loaded = Recording::load(&temp_path).unwrap();
        assert_eq!(loaded.description, recording.description);
        assert_eq!(loaded.event_count(), recording.event_count());

        std::fs::remove_file(&temp_path).unwrap();
    }
}
