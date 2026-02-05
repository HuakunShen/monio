//! Input event statistics collection and analysis.
//!
//! This module provides functionality to collect and analyze input event statistics,
//! useful for:
//! - Productivity analysis (like WakaTime for input)
//! - Health reminders (detect continuous typing for too long)
//! - User behavior analysis
//!
//! # Example
//!
//! ```no_run
//! use monio::statistics::StatisticsCollector;
//! use std::time::Duration;
//!
//! let stats = StatisticsCollector::collect_for(Duration::from_secs(60)).unwrap();
//!
//! println!("Total events: {}", stats.total_events());
//! println!("Key presses: {}", stats.key_press_count);
//! println!("Most pressed key: {:?}", stats.most_frequent_key());
//! println!("Mouse moved: {:.1} pixels", stats.total_mouse_distance);
//! ```

use crate::Hook;
use crate::error::{Error, Result};
use crate::event::{Event, EventType};
use crate::keycode::Key;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Statistics collected from input events.
#[derive(Debug, Clone, Default)]
pub struct EventStatistics {
    // Event counts
    /// Total number of events recorded.
    pub total_event_count: u64,
    /// Number of key press events.
    pub key_press_count: u64,
    /// Number of key release events.
    pub key_release_count: u64,
    /// Number of mouse press events.
    pub mouse_press_count: u64,
    /// Number of mouse release events.
    pub mouse_release_count: u64,
    /// Number of mouse click events.
    pub mouse_click_count: u64,
    /// Number of mouse move events.
    pub mouse_move_count: u64,
    /// Number of mouse drag events.
    pub mouse_drag_count: u64,
    /// Number of mouse wheel events.
    pub mouse_wheel_count: u64,

    // Key statistics
    /// Count of each key pressed.
    pub key_frequency: HashMap<Key, u64>,

    // Mouse statistics
    /// Total distance the mouse has moved (in pixels).
    pub total_mouse_distance: f64,
    /// Current mouse position (last known).
    pub current_mouse_position: (f64, f64),

    // Timing statistics
    /// When statistics collection started.
    pub start_time: Option<Instant>,
    /// When statistics collection ended.
    pub end_time: Option<Instant>,
    /// Time of first key press.
    pub first_key_time: Option<Instant>,
    /// Time of last key press.
    pub last_key_time: Option<Instant>,
    /// Total time spent typing (sum of intervals between key presses < 5 seconds).
    pub active_typing_duration: Duration,
    /// Time of first mouse movement.
    pub first_mouse_time: Option<Instant>,
    /// Time of last mouse movement.
    pub last_mouse_time: Option<Instant>,

    // Click timing
    /// Average interval between mouse clicks.
    pub avg_click_interval: Option<Duration>,
    /// Last mouse click time (for interval calculation).
    last_click_time: Option<Instant>,
    /// Sum of all click intervals.
    click_interval_sum: Duration,
    /// Number of click intervals measured.
    click_interval_count: u64,

    // Button statistics
    /// Count of clicks per mouse button.
    pub button_clicks: HashMap<crate::event::Button, u64>,

    // Scroll statistics
    /// Total vertical scroll amount.
    ///
    /// Note: This is the sum of raw scroll deltas. Convention:
    /// - Positive value = scrolled up (away from user)
    /// - Negative value = scrolled down (toward user)
    pub total_vertical_scroll: f64,
    /// Total horizontal scroll amount.
    ///
    /// Note: This is the sum of raw scroll deltas. Convention:
    /// - Positive value = scrolled right
    /// - Negative value = scrolled left
    pub total_horizontal_scroll: f64,
}

impl EventStatistics {
    /// Create a new empty statistics collector.
    pub fn new() -> Self {
        Self {
            key_frequency: HashMap::new(),
            button_clicks: HashMap::new(),
            current_mouse_position: (0.0, 0.0),
            ..Default::default()
        }
    }

    /// Process an event and update statistics.
    pub fn record_event(&mut self, event: &Event) {
        self.total_event_count += 1;

        match event.event_type {
            EventType::KeyPressed => {
                self.key_press_count += 1;
                let now = Instant::now();

                if self.first_key_time.is_none() {
                    self.first_key_time = Some(now);
                }

                // Calculate active typing time (if < 5s since last key)
                if let Some(last) = self.last_key_time {
                    let interval = now.duration_since(last);
                    if interval < Duration::from_secs(5) {
                        self.active_typing_duration += interval;
                    }
                }

                self.last_key_time = Some(now);

                if let Some(ref kb) = event.keyboard {
                    *self.key_frequency.entry(kb.key).or_insert(0) += 1;
                }
            }
            EventType::KeyReleased => {
                self.key_release_count += 1;
            }
            EventType::MousePressed => {
                self.mouse_press_count += 1;

                let now = Instant::now();
                if let Some(last) = self.last_click_time {
                    let interval = now.duration_since(last);
                    self.click_interval_sum += interval;
                    self.click_interval_count += 1;
                    self.avg_click_interval =
                        Some(self.click_interval_sum / self.click_interval_count as u32);
                }
                self.last_click_time = Some(now);

                if let Some(ref mouse) = event.mouse
                    && let Some(button) = mouse.button
                {
                    *self.button_clicks.entry(button).or_insert(0) += 1;
                }
            }
            EventType::MouseReleased => {
                self.mouse_release_count += 1;
            }
            EventType::MouseClicked => {
                self.mouse_click_count += 1;
            }
            EventType::MouseMoved | EventType::MouseDragged => {
                if event.event_type == EventType::MouseMoved {
                    self.mouse_move_count += 1;
                } else {
                    self.mouse_drag_count += 1;
                }

                let now = Instant::now();
                if self.first_mouse_time.is_none() {
                    self.first_mouse_time = Some(now);
                }
                self.last_mouse_time = Some(now);

                if let Some(ref mouse) = event.mouse {
                    let dx = mouse.x - self.current_mouse_position.0;
                    let dy = mouse.y - self.current_mouse_position.1;
                    self.total_mouse_distance += (dx * dx + dy * dy).sqrt();
                    self.current_mouse_position = (mouse.x, mouse.y);
                }
            }
            EventType::MouseWheel => {
                self.mouse_wheel_count += 1;
                if let Some(ref wheel) = event.wheel {
                    match wheel.direction {
                        crate::event::ScrollDirection::Up => {
                            self.total_vertical_scroll += wheel.delta
                        }
                        crate::event::ScrollDirection::Down => {
                            self.total_vertical_scroll -= wheel.delta
                        }
                        crate::event::ScrollDirection::Left => {
                            self.total_horizontal_scroll -= wheel.delta
                        }
                        crate::event::ScrollDirection::Right => {
                            self.total_horizontal_scroll += wheel.delta
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Get total number of events.
    pub fn total_events(&self) -> u64 {
        self.total_event_count
    }

    /// Get the most frequently pressed key.
    pub fn most_frequent_key(&self) -> Option<(Key, u64)> {
        self.key_frequency
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(key, count)| (*key, *count))
    }

    /// Get the most frequently used mouse button.
    pub fn most_frequent_button(&self) -> Option<(crate::event::Button, u64)> {
        self.button_clicks
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(btn, count)| (*btn, *count))
    }

    /// Get the duration of data collection.
    pub fn collection_duration(&self) -> Duration {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => end.duration_since(start),
            (Some(start), None) => start.elapsed(),
            _ => Duration::ZERO,
        }
    }

    /// Get events per minute.
    pub fn events_per_minute(&self) -> f64 {
        let duration = self.collection_duration();
        if duration.as_secs() == 0 {
            return 0.0;
        }
        self.total_event_count as f64 / duration.as_secs_f64() * 60.0
    }

    /// Get typing speed in keys per minute.
    pub fn keys_per_minute(&self) -> f64 {
        let duration = self.collection_duration();
        if duration.as_secs() == 0 {
            return 0.0;
        }
        self.key_press_count as f64 / duration.as_secs_f64() * 60.0
    }

    /// Get mouse activity ratio (0.0 to 1.0).
    pub fn mouse_activity_ratio(&self) -> f64 {
        let total_input = self.key_press_count + self.mouse_press_count + self.mouse_move_count;
        if total_input == 0 {
            return 0.0;
        }
        (self.mouse_move_count + self.mouse_press_count) as f64 / total_input as f64
    }

    /// Check if user has been active recently (within the last `duration`).
    pub fn is_active_recently(&self, duration: Duration) -> bool {
        let now = Instant::now();

        let key_active = self
            .last_key_time
            .map(|t| now.duration_since(t) < duration)
            .unwrap_or(false);

        let mouse_active = self
            .last_mouse_time
            .map(|t| now.duration_since(t) < duration)
            .unwrap_or(false);

        key_active || mouse_active
    }

    /// Check if user has been typing continuously for too long.
    ///
    /// Returns `true` if the user has been typing for more than `threshold`
    /// without a significant break (> 60 seconds).
    pub fn needs_break(&self, threshold: Duration) -> bool {
        if self.active_typing_duration > threshold {
            // Check if there's been a recent pause
            if let Some(last) = self.last_key_time {
                let since_last = Instant::now().duration_since(last);
                if since_last > Duration::from_secs(60) {
                    return false; // They've taken a break
                }
            }
            true
        } else {
            false
        }
    }

    /// Generate a human-readable summary.
    pub fn summary(&self) -> String {
        let duration = self.collection_duration();
        let minutes = duration.as_secs() / 60;
        let seconds = duration.as_secs() % 60;

        let mut summary = format!(
            "=== Input Statistics ===\n\
             Duration: {:02}:{:02}\n\
             Total Events: {}\n\
             Events/min: {:.1}\n\n",
            minutes,
            seconds,
            self.total_event_count,
            self.events_per_minute()
        );

        // Keyboard stats
        summary.push_str(&format!(
            "Keyboard:\n\
             - Presses: {}\n\
             - Releases: {}\n\
             - Keys/min: {:.1}\n",
            self.key_press_count,
            self.key_release_count,
            self.keys_per_minute()
        ));

        if let Some((key, count)) = self.most_frequent_key() {
            summary.push_str(&format!("- Most pressed: {:?} ({} times)\n", key, count));
        }

        summary.push('\n');

        // Mouse stats
        summary.push_str(&format!(
            "Mouse:\n\
             - Clicks: {}\n\
             - Moves: {}\n\
             - Drags: {}\n\
             - Distance: {:.0} pixels\n",
            self.mouse_click_count,
            self.mouse_move_count,
            self.mouse_drag_count,
            self.total_mouse_distance
        ));

        if let Some(interval) = self.avg_click_interval {
            summary.push_str(&format!("- Avg click interval: {:?}\n", interval));
        }

        if let Some((btn, count)) = self.most_frequent_button() {
            summary.push_str(&format!("- Most clicked: {:?} ({} times)\n", btn, count));
        }

        summary
    }

    /// Merge another statistics object into this one.
    pub fn merge(&mut self, other: &EventStatistics) {
        self.total_event_count += other.total_event_count;
        self.key_press_count += other.key_press_count;
        self.key_release_count += other.key_release_count;
        self.mouse_press_count += other.mouse_press_count;
        self.mouse_release_count += other.mouse_release_count;
        self.mouse_click_count += other.mouse_click_count;
        self.mouse_move_count += other.mouse_move_count;
        self.mouse_drag_count += other.mouse_drag_count;
        self.mouse_wheel_count += other.mouse_wheel_count;

        // Merge key frequencies
        for (key, count) in &other.key_frequency {
            *self.key_frequency.entry(*key).or_insert(0) += count;
        }

        // Merge button clicks
        for (btn, count) in &other.button_clicks {
            *self.button_clicks.entry(*btn).or_insert(0) += count;
        }

        self.total_mouse_distance += other.total_mouse_distance;
        self.total_vertical_scroll += other.total_vertical_scroll;
        self.total_horizontal_scroll += other.total_horizontal_scroll;
        self.active_typing_duration += other.active_typing_duration;
    }
}

/// Collects statistics in real-time.
pub struct StatisticsCollector {
    stats: Arc<Mutex<EventStatistics>>,
    hook: Option<Hook>,
    running: Arc<AtomicBool>,
}

impl StatisticsCollector {
    /// Create a new statistics collector.
    pub fn new() -> Self {
        let mut stats = EventStatistics::new();
        stats.start_time = Some(Instant::now());

        Self {
            stats: Arc::new(Mutex::new(stats)),
            hook: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start collecting statistics in the background.
    pub fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Err(Error::AlreadyRunning);
        }

        let stats = self.stats.clone();
        let running = self.running.clone();

        let hook = Hook::new();
        hook.run_async(move |event: &Event| {
            if !running.load(Ordering::SeqCst) {
                return;
            }
            if let Ok(mut s) = stats.lock() {
                s.record_event(event);
            }
        })?;

        // Only set running flag after hook is successfully started
        self.running.store(true, Ordering::SeqCst);
        self.hook = Some(hook);
        Ok(())
    }

    /// Stop collecting and return the statistics.
    pub fn stop(&mut self) -> Result<EventStatistics> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Err(Error::NotRunning);
        }

        if let Some(hook) = self.hook.take() {
            hook.stop()?;
        }

        let mut stats = self
            .stats
            .lock()
            .map_err(|_| Error::ThreadError("statistics mutex poisoned".into()))?;
        stats.end_time = Some(Instant::now());
        Ok(stats.clone())
    }

    /// Get a snapshot of current statistics without stopping.
    pub fn snapshot(&self) -> EventStatistics {
        match self.stats.lock() {
            Ok(s) => s.clone(),
            Err(_) => EventStatistics::new(), // Return empty stats if mutex is poisoned
        }
    }

    /// Check if currently collecting.
    pub fn is_collecting(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Collect statistics for a specific duration.
    ///
    /// Convenience method that starts collecting, waits for the specified duration,
    /// then stops and returns the statistics.
    pub fn collect_for(duration: Duration) -> Result<EventStatistics> {
        let mut collector = Self::new();
        collector.start()?;
        std::thread::sleep(duration);
        collector.stop()
    }
}

impl Default for StatisticsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statistics_new() {
        let stats = EventStatistics::new();
        assert_eq!(stats.total_events(), 0);
        assert!(stats.most_frequent_key().is_none());
        assert_eq!(stats.events_per_minute(), 0.0);
    }

    #[test]
    fn test_record_key_press() {
        let mut stats = EventStatistics::new();
        let event = Event::key_pressed(Key::KeyA, 30);
        stats.record_event(&event);

        assert_eq!(stats.key_press_count, 1);
        assert_eq!(stats.key_frequency.get(&Key::KeyA), Some(&1));
    }

    #[test]
    fn test_most_frequent_key() {
        let mut stats = EventStatistics::new();

        stats.record_event(&Event::key_pressed(Key::KeyA, 30));
        stats.record_event(&Event::key_pressed(Key::KeyA, 30));
        stats.record_event(&Event::key_pressed(Key::KeyB, 48));

        let (key, count) = stats.most_frequent_key().unwrap();
        assert_eq!(key, Key::KeyA);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_mouse_distance() {
        let mut stats = EventStatistics::new();
        stats.current_mouse_position = (0.0, 0.0);

        stats.record_event(&Event::mouse_moved(3.0, 4.0)); // 5 pixels from origin
        assert!((stats.total_mouse_distance - 5.0).abs() < 0.001);

        stats.record_event(&Event::mouse_moved(6.0, 8.0)); // another 5 pixels
        assert!((stats.total_mouse_distance - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_merge() {
        let mut stats1 = EventStatistics::new();
        stats1.record_event(&Event::key_pressed(Key::KeyA, 30));

        let mut stats2 = EventStatistics::new();
        stats2.record_event(&Event::key_pressed(Key::KeyB, 48));

        stats1.merge(&stats2);

        assert_eq!(stats1.key_press_count, 2);
        assert_eq!(stats1.key_frequency.get(&Key::KeyA), Some(&1));
        assert_eq!(stats1.key_frequency.get(&Key::KeyB), Some(&1));
    }
}
