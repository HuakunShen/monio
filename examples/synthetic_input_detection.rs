//! Diagnostic example for observing monio's own simulated input.
//!
//! Run with: cargo run --example synthetic_input_detection

use monio::channel::listen_unbounded_channel;
use monio::{
    Event, EventType, Key, Rect, display_at_point, key_press, key_release, mouse_move,
    mouse_position,
};
use std::error::Error;
use std::io;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::sleep;
use std::time::{Duration, Instant};

const COORDINATE_TOLERANCE: f64 = 1.0;
const MOUSE_OFFSET: (f64, f64) = (32.0, 24.0);
const HOOK_START_TIMEOUT: Duration = Duration::from_secs(5);
const COLLECTION_TIMEOUT: Duration = Duration::from_secs(2);
const ACTION_PAUSE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Observation {
    control_pressed: bool,
    control_released: bool,
    reached_target: bool,
    returned_to_origin: bool,
}

impl Observation {
    fn is_complete(self) -> bool {
        self.control_pressed
            && self.control_released
            && self.reached_target
            && self.returned_to_origin
    }
}

fn observe_event(
    observed: &mut Observation,
    event: &Event,
    origin: (f64, f64),
    target: (f64, f64),
) {
    if event.event_type == EventType::KeyPressed
        && event
            .keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.key == Key::ControlLeft)
    {
        observed.control_pressed = true;
    }
    if event.event_type == EventType::KeyReleased
        && event
            .keyboard
            .as_ref()
            .is_some_and(|keyboard| keyboard.key == Key::ControlLeft)
    {
        observed.control_released = true;
    }
    if event.event_type == EventType::MouseMoved
        && let Some(mouse) = &event.mouse
    {
        if point_is_near((mouse.x, mouse.y), target) {
            observed.reached_target = true;
        }
        if observed.reached_target && point_is_near((mouse.x, mouse.y), origin) {
            observed.returned_to_origin = true;
        }
    }
}

fn point_is_near(actual: (f64, f64), expected: (f64, f64)) -> bool {
    (actual.0 - expected.0).abs() <= COORDINATE_TOLERANCE
        && (actual.1 - expected.1).abs() <= COORDINATE_TOLERANCE
}

fn choose_target(origin: (f64, f64), bounds: Rect) -> (f64, f64) {
    (
        shifted_axis(origin.0, bounds.x, bounds.x + bounds.width, MOUSE_OFFSET.0),
        shifted_axis(origin.1, bounds.y, bounds.y + bounds.height, MOUSE_OFFSET.1),
    )
}

fn shifted_axis(value: f64, minimum: f64, maximum: f64, offset: f64) -> f64 {
    if value + offset < maximum {
        value + offset
    } else {
        (value - offset).max(minimum)
    }
}

fn wait_for_hook(receiver: &Receiver<Event>) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + HOOK_START_TIMEOUT;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "listener did not report HookEnabled within 5 seconds",
            )
            .into());
        }

        match receiver.recv_timeout(remaining) {
            Ok(event) if event.event_type == EventType::HookEnabled => return Ok(()),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "listener did not report HookEnabled within 5 seconds",
                )
                .into());
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other(
                    "listener stopped before HookEnabled; check input/accessibility permissions",
                )
                .into());
            }
        }
    }
}

fn simulate_sequence(origin: (f64, f64), target: (f64, f64)) -> monio::Result<()> {
    println!("1. Pressing ControlLeft");
    key_press(Key::ControlLeft)?;
    sleep(ACTION_PAUSE);

    println!("2. Releasing ControlLeft");
    if let Err(error) = key_release(Key::ControlLeft) {
        let _ = key_release(Key::ControlLeft);
        return Err(error);
    }
    sleep(ACTION_PAUSE);

    println!("3. Moving mouse to ({:.1}, {:.1})", target.0, target.1);
    mouse_move(target.0, target.1)?;
    sleep(ACTION_PAUSE);

    println!("4. Restoring mouse to ({:.1}, {:.1})", origin.0, origin.1);
    mouse_move(origin.0, origin.1)?;

    Ok(())
}

fn collect_observations(
    receiver: &Receiver<Event>,
    origin: (f64, f64),
    target: (f64, f64),
) -> Result<Observation, Box<dyn Error>> {
    let deadline = Instant::now() + COLLECTION_TIMEOUT;
    let mut observed = Observation::default();

    while !observed.is_complete() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        match receiver.recv_timeout(remaining) {
            Ok(event) => {
                let before = observed;
                observe_event(&mut observed, &event, origin, target);
                if observed != before {
                    println!("   listener received: {event:?}");
                }
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other(
                    "listener channel disconnected while collecting events",
                )
                .into());
            }
        }
    }

    Ok(observed)
}

fn run_diagnostic(receiver: &Receiver<Event>) -> Result<Observation, Box<dyn Error>> {
    wait_for_hook(receiver)?;
    println!("Listener enabled.\n");

    let origin = mouse_position()?;
    let display = display_at_point(origin.0, origin.1)?.ok_or_else(|| {
        io::Error::other(format!(
            "mouse position ({:.1}, {:.1}) is outside known displays",
            origin.0, origin.1
        ))
    })?;
    let target = choose_target(origin, display.bounds);

    while receiver.try_recv().is_ok() {}

    simulate_sequence(origin, target)?;
    collect_observations(receiver, origin, target)
}

fn print_result(label: &str, observed: bool) {
    let result = if observed { "YES" } else { "NO" };
    println!("  {label:<34} {result}");
}

fn print_summary(observed: Observation) {
    println!("\nResults");
    println!("=======");
    print_result("ControlLeft press observed:", observed.control_pressed);
    print_result("ControlLeft release observed:", observed.control_released);
    print_result("Mouse target observed:", observed.reached_target);
    print_result("Mouse restoration observed:", observed.returned_to_origin);

    println!();
    if observed.is_complete() {
        println!("The listener received events matching every simulated action.");
    } else {
        println!("The listener did not receive every simulated action on this run.");
    }
    println!(
        "This only proves observation by timing and matching values. monio's public Event API \
         has no physical-versus-synthetic source field, so it cannot distinguish the two."
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("monio synthetic input detection example");
    println!("=======================================\n");
    println!("This will briefly press ControlLeft and move the mouse, then restore it.");
    println!("On macOS, grant Accessibility permission to the terminal first.\n");

    let (handle, receiver) = listen_unbounded_channel()?;
    let diagnostic_result = run_diagnostic(&receiver);
    let stop_result = handle.stop();

    let observed = diagnostic_result?;
    stop_result?;
    print_summary(observed);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use monio::{Event, Key, Rect};

    const ORIGIN: (f64, f64) = (100.0, 200.0);
    const TARGET: (f64, f64) = (132.0, 224.0);

    #[test]
    fn observes_simulated_control_press() {
        let mut observed = Observation::default();

        observe_event(
            &mut observed,
            &Event::key_pressed(Key::ControlLeft, 0),
            ORIGIN,
            TARGET,
        );

        assert!(observed.control_pressed);
    }

    #[test]
    fn observes_simulated_control_release() {
        let mut observed = Observation::default();

        observe_event(
            &mut observed,
            &Event::key_released(Key::ControlLeft, 0),
            ORIGIN,
            TARGET,
        );

        assert!(observed.control_released);
    }

    #[test]
    fn observes_mouse_moves_near_requested_coordinates() {
        let mut observed = Observation::default();

        observe_event(
            &mut observed,
            &Event::mouse_moved(ORIGIN.0, ORIGIN.1),
            ORIGIN,
            TARGET,
        );
        assert!(!observed.returned_to_origin);

        observe_event(
            &mut observed,
            &Event::mouse_moved(TARGET.0 + 0.5, TARGET.1 - 0.5),
            ORIGIN,
            TARGET,
        );
        observe_event(
            &mut observed,
            &Event::mouse_moved(ORIGIN.0 - 0.5, ORIGIN.1 + 0.5),
            ORIGIN,
            TARGET,
        );

        assert!(observed.reached_target);
        assert!(observed.returned_to_origin);
    }

    #[test]
    fn chooses_an_in_bounds_mouse_target() {
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        assert_eq!(choose_target((100.0, 50.0), bounds), (132.0, 74.0));
        assert_eq!(choose_target((190.0, 90.0), bounds), (158.0, 66.0));
    }

    #[test]
    fn ignores_unrelated_input() {
        let mut observed = Observation::default();

        observe_event(
            &mut observed,
            &Event::key_pressed(Key::KeyA, 0),
            ORIGIN,
            TARGET,
        );
        observe_event(
            &mut observed,
            &Event::mouse_moved(400.0, 500.0),
            ORIGIN,
            TARGET,
        );

        assert_eq!(observed, Observation::default());
    }
}
