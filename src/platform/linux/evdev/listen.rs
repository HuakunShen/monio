//! Linux evdev input listening.
//!
//! Reads input events directly from /dev/input/event* devices.
//! Works on both X11 and Wayland.

use crate::error::{Error, Result};
use crate::event::{Button, Event, ScrollDirection};
use crate::hook::{EventHandler, GrabHandler};
use crate::platform::linux::keycodes::evdev_keycode_to_key;
use crate::state::{
    self, MASK_ALT, MASK_BUTTON1, MASK_BUTTON2, MASK_BUTTON3, MASK_BUTTON4, MASK_BUTTON5,
    MASK_CTRL, MASK_META, MASK_SHIFT,
};
use evdev::{Device, EventType as EvdevEventType, InputEventKind, Key as EvdevKey};
use std::collections::HashMap;
use std::fs;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Flag to signal stopping
static STOP_FLAG: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Current mouse position (evdev gives relative motion, we need to track absolute)
static MOUSE_POS: Mutex<(f64, f64)> = Mutex::new((0.0, 0.0));

/// Update modifier mask from keycode
fn update_key_modifier(code: u16, pressed: bool) {
    let mask = match code {
        42 | 54 => MASK_SHIFT,  // KEY_LEFTSHIFT, KEY_RIGHTSHIFT
        29 | 97 => MASK_CTRL,   // KEY_LEFTCTRL, KEY_RIGHTCTRL
        56 | 100 => MASK_ALT,   // KEY_LEFTALT, KEY_RIGHTALT
        125 | 126 => MASK_META, // KEY_LEFTMETA, KEY_RIGHTMETA
        _ => return,
    };

    if pressed {
        state::set_mask(mask);
    } else {
        state::unset_mask(mask);
    }
}

/// Convert evdev button code to Button enum
fn code_to_button(code: u16) -> Option<Button> {
    match code {
        0x110 => Some(Button::Left),    // BTN_LEFT
        0x111 => Some(Button::Right),   // BTN_RIGHT
        0x112 => Some(Button::Middle),  // BTN_MIDDLE
        0x113 => Some(Button::Button4), // BTN_SIDE
        0x114 => Some(Button::Button5), // BTN_EXTRA
        _ => None,
    }
}

/// Get button mask for code
fn code_to_mask(code: u16) -> u32 {
    match code {
        0x110 => MASK_BUTTON1,
        0x111 => MASK_BUTTON2,
        0x112 => MASK_BUTTON3,
        0x113 => MASK_BUTTON4,
        0x114 => MASK_BUTTON5,
        _ => 0,
    }
}

/// Enumerate all input devices
fn enumerate_devices() -> Result<Vec<Device>> {
    let mut devices = Vec::new();

    let dir = fs::read_dir("/dev/input").map_err(|e| {
        Error::PermissionDenied(format!(
            "Cannot access /dev/input: {}. Make sure you're in the 'input' group.",
            e
        ))
    })?;

    for entry in dir.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name() {
            let name = name.to_string_lossy();
            if name.starts_with("event") {
                match Device::open(&path) {
                    Ok(device) => {
                        // Only include devices that have key or relative events
                        let supported = device.supported_events();
                        if supported.contains(EvdevEventType::KEY)
                            || supported.contains(EvdevEventType::RELATIVE)
                        {
                            devices.push(device);
                        }
                    }
                    Err(e) => {
                        log::debug!("Failed to open {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    if devices.is_empty() {
        return Err(Error::PermissionDenied(
            "No input devices accessible. Make sure you're in the 'input' group: \
             sudo usermod -aG input $USER"
                .into(),
        ));
    }

    Ok(devices)
}

/// Handler wrapper for listen mode
struct ListenHandler<H: EventHandler> {
    handler: H,
}

impl<H: EventHandler> ListenHandler<H> {
    fn handle(&self, event: &Event) {
        self.handler.handle_event(event);
    }
}

/// Handler wrapper for grab mode
struct GrabHandlerWrapper<H: GrabHandler> {
    handler: H,
}

impl<H: GrabHandler> GrabHandlerWrapper<H> {
    fn handle(&self, event: &Event) -> bool {
        // Returns true if event should be passed through
        self.handler.handle_event(event).is_some()
    }
}

/// Run the event hook (blocking).
pub fn run_hook<H: EventHandler + 'static>(running: &Arc<AtomicBool>, handler: H) -> Result<()> {
    // Store stop flag
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = Some(running.clone());
    }

    let wrapper = ListenHandler { handler };
    run_event_loop(running, |event| {
        wrapper.handle(event);
        true // Always pass through in listen mode
    })?;

    // Cleanup
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = None;
    }

    Ok(())
}

/// Run the event hook with grab capability (blocking).
pub fn run_grab_hook<H: GrabHandler + 'static>(
    running: &Arc<AtomicBool>,
    handler: H,
) -> Result<()> {
    // Store stop flag
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = Some(running.clone());
    }

    let wrapper = GrabHandlerWrapper { handler };

    // For grab mode, we need to grab the devices
    let devices = enumerate_devices()?;
    let mut grabbed_devices = Vec::new();

    for mut device in devices {
        // Try to grab the device (exclusive access)
        if device.grab().is_ok() {
            grabbed_devices.push(device);
        } else {
            log::warn!(
                "Failed to grab device: {}",
                device.name().unwrap_or("unknown")
            );
        }
    }

    if grabbed_devices.is_empty() {
        return Err(Error::PermissionDenied(
            "Could not grab any input devices. Make sure you're in the 'input' group.".into(),
        ));
    }

    // Send hook enabled event
    let _ = wrapper.handle(&Event::hook_enabled());

    // Event loop with grabbed devices
    run_grabbed_event_loop(running, &mut grabbed_devices, |event| wrapper.handle(event))?;

    // Send hook disabled event
    let _ = wrapper.handle(&Event::hook_disabled());

    // Ungrab devices
    for mut device in grabbed_devices {
        let _ = device.ungrab();
    }

    // Cleanup
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = None;
    }

    Ok(())
}

/// Main event loop for listen mode (non-grabbing)
fn run_event_loop<F>(running: &Arc<AtomicBool>, mut callback: F) -> Result<()>
where
    F: FnMut(&Event) -> bool,
{
    let devices = enumerate_devices()?;

    // Send hook enabled event
    callback(&Event::hook_enabled());

    // Create poll fds
    let mut poll_fds: Vec<libc::pollfd> = devices
        .iter()
        .map(|d| libc::pollfd {
            fd: d.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();

    // Store devices in a map for easy lookup
    let device_map: HashMap<i32, Device> =
        devices.into_iter().map(|d| (d.as_raw_fd(), d)).collect();

    while running.load(Ordering::SeqCst) {
        // Poll with timeout
        let ret = unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as _, 100) };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(Error::HookStartFailed(format!("poll error: {}", err)));
        }

        if ret == 0 {
            // Timeout, check stop flag
            continue;
        }

        // Process events from devices with data
        for pfd in &poll_fds {
            if pfd.revents & libc::POLLIN != 0 {
                if let Some(device) = device_map.get(&pfd.fd) {
                    // Note: We can't easily mutate device here due to HashMap
                    // In a real implementation, we'd use interior mutability
                    // For now, we'll use a simpler approach
                }
            }
        }

        // Simplified approach: iterate and fetch events
        for (_, device) in &device_map {
            if let Ok(events) = device.fetch_events() {
                for ev in events {
                    if let Some(event) = convert_event(&ev) {
                        callback(&event);
                    }
                }
            }
        }
    }

    // Send hook disabled event
    callback(&Event::hook_disabled());

    Ok(())
}

/// Event loop for grab mode (with device grabbing)
fn run_grabbed_event_loop<F>(
    running: &Arc<AtomicBool>,
    devices: &mut [Device],
    mut callback: F,
) -> Result<()>
where
    F: FnMut(&Event) -> bool,
{
    // Create poll fds
    let mut poll_fds: Vec<libc::pollfd> = devices
        .iter()
        .map(|d| libc::pollfd {
            fd: d.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();

    while running.load(Ordering::SeqCst) {
        // Poll with timeout
        let ret = unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as _, 100) };

        if ret < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(Error::HookStartFailed(format!("poll error: {}", err)));
        }

        if ret == 0 {
            continue;
        }

        // Process events
        for (i, pfd) in poll_fds.iter().enumerate() {
            if pfd.revents & libc::POLLIN != 0 {
                if let Some(device) = devices.get_mut(i) {
                    if let Ok(events) = device.fetch_events() {
                        for ev in events {
                            if let Some(event) = convert_event(&ev) {
                                let _pass_through = callback(&event);
                                // Note: In true grab mode, we'd need to re-inject
                                // the event if pass_through is true. This requires
                                // uinput which adds complexity. For now, we just
                                // consume all events when grabbed.
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Convert evdev InputEvent to our Event type
fn convert_event(ev: &evdev::InputEvent) -> Option<Event> {
    match ev.kind() {
        InputEventKind::Key(key) => {
            let code = key.code();
            let pressed = ev.value() == 1;

            // Check if it's a mouse button
            if code >= 0x110 && code <= 0x117 {
                let button = code_to_button(code)?;
                let mask = code_to_mask(code);

                if pressed {
                    state::set_mask(mask);
                    let (x, y) = *MOUSE_POS.lock().ok()?;
                    Some(Event::mouse_pressed(button, x, y))
                } else {
                    state::unset_mask(mask);
                    let (x, y) = *MOUSE_POS.lock().ok()?;
                    Some(Event::mouse_released(button, x, y))
                }
            } else {
                // Keyboard key
                update_key_modifier(code, pressed);
                let key = evdev_keycode_to_key(code);

                if pressed {
                    Some(Event::key_pressed(key, code as u32))
                } else {
                    Some(Event::key_released(key, code as u32))
                }
            }
        }

        InputEventKind::RelAxis(axis) => {
            use evdev::RelativeAxisType;

            let mut pos = MOUSE_POS.lock().ok()?;
            let value = ev.value() as f64;

            match axis {
                RelativeAxisType::REL_X => {
                    pos.0 += value;
                    if state::is_button_held() {
                        Some(Event::mouse_dragged(pos.0, pos.1))
                    } else {
                        Some(Event::mouse_moved(pos.0, pos.1))
                    }
                }
                RelativeAxisType::REL_Y => {
                    pos.1 += value;
                    if state::is_button_held() {
                        Some(Event::mouse_dragged(pos.0, pos.1))
                    } else {
                        Some(Event::mouse_moved(pos.0, pos.1))
                    }
                }
                RelativeAxisType::REL_WHEEL => {
                    let direction = if value > 0.0 {
                        ScrollDirection::Up
                    } else {
                        ScrollDirection::Down
                    };
                    Some(Event::mouse_wheel(pos.0, pos.1, direction, value.abs()))
                }
                RelativeAxisType::REL_HWHEEL => {
                    let direction = if value > 0.0 {
                        ScrollDirection::Right
                    } else {
                        ScrollDirection::Left
                    };
                    Some(Event::mouse_wheel(pos.0, pos.1, direction, value.abs()))
                }
                _ => None,
            }
        }

        InputEventKind::AbsAxis(axis) => {
            use evdev::AbsoluteAxisType;

            let mut pos = MOUSE_POS.lock().ok()?;
            let value = ev.value() as f64;

            match axis {
                AbsoluteAxisType::ABS_X => {
                    pos.0 = value;
                    if state::is_button_held() {
                        Some(Event::mouse_dragged(pos.0, pos.1))
                    } else {
                        Some(Event::mouse_moved(pos.0, pos.1))
                    }
                }
                AbsoluteAxisType::ABS_Y => {
                    pos.1 = value;
                    if state::is_button_held() {
                        Some(Event::mouse_dragged(pos.0, pos.1))
                    } else {
                        Some(Event::mouse_moved(pos.0, pos.1))
                    }
                }
                _ => None,
            }
        }

        _ => None,
    }
}

/// Stop the event hook.
pub fn stop_hook() -> Result<()> {
    // The stop is signaled via the running atomic
    Ok(())
}
