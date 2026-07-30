//! Linux evdev input listening.
//!
//! Reads input events directly from /dev/input/event* devices.
//! Works on both X11 and Wayland.

#![allow(dead_code)]

use crate::error::{Error, Result};
use crate::event::{Button, Event, InputOrigin, ScrollDirection};
use crate::hook::{EventHandler, GrabHandler};
use crate::platform::linux::evdev::provenance::InjectorDeviceIdentity;
use crate::platform::linux::evdev::simulate::{emit_event, emit_relative_motion, initialize};
use crate::platform::linux::keycodes::evdev_keycode_to_key;
use crate::platform::motion::{RelativeMotionFrame, RelativeMotionSample};
use crate::state::{
    self, MASK_ALT, MASK_BUTTON1, MASK_BUTTON2, MASK_BUTTON3, MASK_BUTTON4, MASK_BUTTON5,
    MASK_CTRL, MASK_META, MASK_SHIFT,
};
use evdev::{Device, EventType as EvdevEventType, InputEventKind, Synchronization};
use std::fs;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

struct CapturedDevice {
    device: Device,
    origin: InputOrigin,
    relative_motion: RelativeMotionFrame,
}

enum ConvertedInput {
    Immediate(Event),
    RelativePending,
    RelativeFrame {
        event: Event,
        motion: RelativeMotionSample,
    },
    Discarded,
    Ignored,
}

/// Enumerate all input devices while retaining exact injector identity.
fn enumerate_devices(
    injector: &InjectorDeviceIdentity,
    include_session_injector: bool,
) -> Result<Vec<CapturedDevice>> {
    let mut devices = Vec::new();
    let mut found_session_injector = false;

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
                        let origin =
                            injector.event_origin(device.as_raw_fd()).map_err(|error| {
                                Error::Platform(format!(
                                    "Failed to inspect input device {}: {}",
                                    path.display(),
                                    error
                                ))
                            })?;
                        let is_session_injector = origin != InputOrigin::Unknown;
                        if is_session_injector && !include_session_injector {
                            continue;
                        }

                        // Only include devices that have key or relative events
                        let supported = device.supported_events();
                        if supported.contains(EvdevEventType::KEY)
                            || supported.contains(EvdevEventType::RELATIVE)
                        {
                            found_session_injector |= is_session_injector;
                            devices.push(CapturedDevice {
                                device,
                                origin,
                                relative_motion: RelativeMotionFrame::default(),
                            });
                        }
                    }
                    Err(e) => {
                        log::debug!("Failed to open {}: {}", path.display(), e);
                    }
                }
            }
        }
    }

    if include_session_injector && !found_session_injector {
        return Err(Error::PermissionDenied(
            "Monio created its uinput injector, but its /dev/input/event* node could not be \
             opened. Make sure the current user can read input devices."
                .into(),
        ));
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
    let injector = initialize()?;

    // Store stop flag
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = Some(running.clone());
    }

    let wrapper = ListenHandler { handler };
    run_event_loop(running, &injector, |event| {
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
    let injector = initialize()?;

    // Store stop flag
    {
        let mut s = STOP_FLAG
            .lock()
            .map_err(|_| Error::ThreadError("mutex poisoned".into()))?;
        *s = Some(running.clone());
    }

    let wrapper = GrabHandlerWrapper { handler };

    // For grab mode, we need to grab the devices
    let devices = enumerate_devices(&injector, false)?;
    let mut grabbed_devices = Vec::new();

    for mut captured in devices {
        // Try to grab the device (exclusive access)
        if captured.device.grab().is_ok() {
            grabbed_devices.push(captured);
        } else {
            log::warn!(
                "Failed to grab device: {}",
                captured.device.name().unwrap_or("unknown")
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
    for mut captured in grabbed_devices {
        let _ = captured.device.ungrab();
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
fn run_event_loop<F>(
    running: &Arc<AtomicBool>,
    injector: &InjectorDeviceIdentity,
    mut callback: F,
) -> Result<()>
where
    F: FnMut(&Event) -> bool,
{
    let mut devices = enumerate_devices(injector, true)?;

    // Send hook enabled event
    callback(&Event::hook_enabled());

    // Create poll fds
    let mut poll_fds: Vec<libc::pollfd> = devices
        .iter()
        .map(|d| libc::pollfd {
            fd: d.device.as_raw_fd(),
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
            // Timeout, check stop flag
            continue;
        }

        for (i, pfd) in poll_fds.iter().enumerate() {
            if pfd.revents & libc::POLLIN != 0
                && let Some(captured) = devices.get_mut(i)
            {
                let origin = captured.origin;
                if let Ok(events) = captured.device.fetch_events() {
                    for ev in events {
                        match convert_event(&ev, origin, &mut captured.relative_motion) {
                            ConvertedInput::Immediate(event)
                            | ConvertedInput::RelativeFrame { event, .. } => {
                                callback(&event);
                            }
                            ConvertedInput::RelativePending
                            | ConvertedInput::Discarded
                            | ConvertedInput::Ignored => {}
                        }
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
    devices: &mut [CapturedDevice],
    mut callback: F,
) -> Result<()>
where
    F: FnMut(&Event) -> bool,
{
    // Create poll fds
    let mut poll_fds: Vec<libc::pollfd> = devices
        .iter()
        .map(|d| libc::pollfd {
            fd: d.device.as_raw_fd(),
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
            if pfd.revents & libc::POLLIN != 0
                && let Some(captured) = devices.get_mut(i)
            {
                let origin = captured.origin;
                if let Ok(events) = captured.device.fetch_events() {
                    for ev in events {
                        match convert_event(&ev, origin, &mut captured.relative_motion) {
                            ConvertedInput::Immediate(event) => {
                                if callback(&event)
                                    && let Err(error) = emit_event(&ev)
                                {
                                    log::debug!("Failed to re-inject event: {}", error);
                                }
                            }
                            ConvertedInput::RelativePending => {}
                            ConvertedInput::RelativeFrame { event, motion } => {
                                if callback(&event)
                                    && let Err(error) =
                                        emit_relative_motion(motion.delta_x, motion.delta_y)
                                {
                                    log::debug!(
                                        "Failed to re-inject relative motion frame: {}",
                                        error
                                    );
                                }
                            }
                            ConvertedInput::Discarded => {}
                            ConvertedInput::Ignored => {
                                // Unknown event type - pass through.
                                if let Err(error) = emit_event(&ev) {
                                    log::debug!("Failed to re-inject event: {}", error);
                                }
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
fn convert_event(
    ev: &evdev::InputEvent,
    origin: InputOrigin,
    relative_motion: &mut RelativeMotionFrame,
) -> ConvertedInput {
    let event = match ev.kind() {
        InputEventKind::Key(key) => {
            let code = key.code();
            let pressed = ev.value() == 1;

            // Check if it's a mouse button
            if (0x110..=0x117).contains(&code) {
                let Some(button) = code_to_button(code) else {
                    return ConvertedInput::Ignored;
                };
                let mask = code_to_mask(code);

                if pressed {
                    state::set_mask(mask);
                    let Ok(position) = MOUSE_POS.lock() else {
                        return ConvertedInput::Ignored;
                    };
                    let (x, y) = *position;
                    Some(Event::mouse_pressed(button, x, y))
                } else {
                    state::unset_mask(mask);
                    let Ok(position) = MOUSE_POS.lock() else {
                        return ConvertedInput::Ignored;
                    };
                    let (x, y) = *position;
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

            let Ok(mut pos) = MOUSE_POS.lock() else {
                return ConvertedInput::Ignored;
            };
            let value = ev.value();

            match axis {
                RelativeAxisType::REL_X => {
                    pos.0 += value as f64;
                    relative_motion.record(value, 0, state::is_button_held());
                    return ConvertedInput::RelativePending;
                }
                RelativeAxisType::REL_Y => {
                    pos.1 += value as f64;
                    relative_motion.record(0, value, state::is_button_held());
                    return ConvertedInput::RelativePending;
                }
                RelativeAxisType::REL_WHEEL => {
                    let direction = if value > 0 {
                        ScrollDirection::Up
                    } else {
                        ScrollDirection::Down
                    };
                    Some(Event::mouse_wheel(
                        pos.0,
                        pos.1,
                        direction,
                        value.unsigned_abs() as f64,
                    ))
                }
                RelativeAxisType::REL_HWHEEL => {
                    let direction = if value > 0 {
                        ScrollDirection::Right
                    } else {
                        ScrollDirection::Left
                    };
                    Some(Event::mouse_wheel(
                        pos.0,
                        pos.1,
                        direction,
                        value.unsigned_abs() as f64,
                    ))
                }
                _ => None,
            }
        }

        InputEventKind::AbsAxis(axis) => {
            use evdev::AbsoluteAxisType;

            let Ok(mut pos) = MOUSE_POS.lock() else {
                return ConvertedInput::Ignored;
            };
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

        InputEventKind::Synchronization(Synchronization::SYN_REPORT) => {
            let Some(motion) = relative_motion.take() else {
                return ConvertedInput::Ignored;
            };
            let Ok(position) = MOUSE_POS.lock() else {
                return ConvertedInput::Ignored;
            };
            let (x, y) = *position;

            let mut event = if motion.dragging {
                Event::mouse_dragged_relative(x, y, motion.delta_x as f64, motion.delta_y as f64)
            } else {
                Event::mouse_moved_relative(x, y, motion.delta_x as f64, motion.delta_y as f64)
            };
            event.origin = origin;

            return ConvertedInput::RelativeFrame { event, motion };
        }

        InputEventKind::Synchronization(Synchronization::SYN_DROPPED) => {
            relative_motion.clear();
            return ConvertedInput::Discarded;
        }

        _ => None,
    };

    match event {
        Some(mut event) => {
            event.origin = origin;
            ConvertedInput::Immediate(event)
        }
        None => ConvertedInput::Ignored,
    }
}

/// Stop the event hook.
pub fn stop_hook() -> Result<()> {
    // The stop is signaled via the running atomic
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{EventType, RelativeMotion};
    use evdev::{InputEvent, RelativeAxisType, Synchronization};

    #[test]
    fn relative_axes_are_coalesced_at_syn_report() {
        state::reset_mask();
        *MOUSE_POS.lock().expect("mouse position lock") = (100.0, 200.0);
        let mut relative_motion = RelativeMotionFrame::default();

        let raw_events = [
            InputEvent::new(EvdevEventType::RELATIVE, RelativeAxisType::REL_X.0, 12),
            InputEvent::new(EvdevEventType::RELATIVE, RelativeAxisType::REL_Y.0, -7),
            InputEvent::new(
                EvdevEventType::SYNCHRONIZATION,
                Synchronization::SYN_REPORT.0,
                0,
            ),
        ];

        let converted = raw_events
            .iter()
            .filter_map(|event| {
                match convert_event(event, InputOrigin::Unknown, &mut relative_motion) {
                    ConvertedInput::Immediate(event)
                    | ConvertedInput::RelativeFrame { event, .. } => Some(event),
                    ConvertedInput::RelativePending
                    | ConvertedInput::Discarded
                    | ConvertedInput::Ignored => None,
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(converted.len(), 1);
        assert!(matches!(
            converted[0].event_type,
            EventType::MouseMoved | EventType::MouseDragged
        ));

        let mouse = converted[0]
            .mouse
            .as_ref()
            .expect("motion event should contain mouse data");
        assert_eq!((mouse.x, mouse.y), (112.0, 193.0));
        assert_eq!(
            mouse.relative,
            Some(RelativeMotion {
                delta_x: 12.0,
                delta_y: -7.0,
            })
        );
    }
}
