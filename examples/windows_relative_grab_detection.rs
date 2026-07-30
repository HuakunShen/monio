//! Native Windows diagnostic for physical relative pointer motion during grab.

#[cfg(target_os = "windows")]
mod windows_diagnostic {
    use monio::{Event, EventType, Hook, InjectorIdentity, InputOrigin, Key};
    use std::error::Error;
    use std::io;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    const HOOK_START_TIMEOUT: Duration = Duration::from_secs(5);
    const MANUAL_DURATION: Duration = Duration::from_secs(10);
    const POLL_INTERVAL: Duration = Duration::from_millis(10);

    #[derive(Clone, Copy, Debug, Default)]
    struct Observation {
        relative_events: usize,
        missing_relative_events: usize,
        drag_events: usize,
        positive_x: bool,
        positive_y: bool,
        negative_x: bool,
        negative_y: bool,
    }

    impl Observation {
        fn observe(&mut self, event: &Event) {
            if matches!(
                event.origin,
                InputOrigin::Injected {
                    injector: InjectorIdentity::ThisMonioSession
                }
            ) {
                return;
            }
            if !matches!(
                event.event_type,
                EventType::MouseMoved | EventType::MouseDragged
            ) {
                return;
            }

            let Some(mouse) = &event.mouse else {
                self.missing_relative_events += 1;
                return;
            };
            let Some(relative) = mouse.relative else {
                self.missing_relative_events += 1;
                return;
            };

            self.relative_events += 1;
            self.drag_events += usize::from(event.event_type == EventType::MouseDragged);
            self.positive_x |= relative.delta_x > 0.0;
            self.positive_y |= relative.delta_y > 0.0;
            self.negative_x |= relative.delta_x < 0.0;
            self.negative_y |= relative.delta_y < 0.0;

            println!(
                "{:?}: absolute=({:.0}, {:.0}) relative=({:.3}, {:.3})",
                event.event_type, mouse.x, mouse.y, relative.delta_x, relative.delta_y
            );
        }
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let pass_through = std::env::args().any(|arg| arg == "--pass-through");

        println!("monio Windows relative grab diagnostic");
        println!("======================================");
        println!("This diagnostic temporarily grabs the Windows keyboard and pointer.");
        println!("Press Escape to stop; Ctrl+C and a ten-second timeout are fallbacks.");
        println!("Move in every direction and continue pushing against a screen edge.");
        println!("Hold a mouse button while moving to verify MouseDragged.");
        println!("SendInput cannot substitute for this physical Raw Input edge test.");
        println!(
            "Pointer mode: {}.\n",
            if pass_through {
                "pass through locally"
            } else {
                "consume locally"
            }
        );

        let stop_requested = Arc::new(AtomicBool::new(false));
        let stop_for_signal = stop_requested.clone();
        ctrlc::set_handler(move || stop_for_signal.store(true, Ordering::SeqCst))?;

        let observation = Arc::new(Mutex::new(Observation::default()));
        let observation_for_hook = observation.clone();
        let stop_for_hook = stop_requested.clone();
        let (enabled_tx, enabled_rx) = mpsc::sync_channel(1);

        let hook = Hook::new();
        hook.grab_async(move |event: &Event| {
            if event.event_type == EventType::HookEnabled {
                let _ = enabled_tx.try_send(());
            }

            if event.event_type == EventType::KeyPressed
                && event.keyboard.as_ref().map(|data| data.key) == Some(Key::Escape)
            {
                stop_for_hook.store(true, Ordering::SeqCst);
                return None;
            }

            if matches!(
                event.event_type,
                EventType::MouseMoved | EventType::MouseDragged
            ) {
                if let Ok(mut observation) = observation_for_hook.lock() {
                    observation.observe(event);
                }
                return pass_through.then(|| event.clone());
            }

            if !pass_through
                && matches!(
                    event.event_type,
                    EventType::MousePressed | EventType::MouseReleased
                )
            {
                return None;
            }

            Some(event.clone())
        })?;

        enabled_rx
            .recv_timeout(HOOK_START_TIMEOUT)
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("grab hook did not report HookEnabled: {error}"),
                )
            })?;
        println!("Grab enabled.\n");

        let deadline = Instant::now() + MANUAL_DURATION;
        while Instant::now() < deadline && !stop_requested.load(Ordering::SeqCst) {
            sleep(POLL_INTERVAL);
        }

        if hook.is_running() {
            hook.stop()?;
        }

        let observed = *observation
            .lock()
            .map_err(|_| io::Error::other("observation mutex poisoned"))?;
        println!("\nResults");
        println!("=======");
        println!("Relative motion events: {}", observed.relative_events);
        println!(
            "Motion events missing relative data: {}",
            observed.missing_relative_events
        );
        println!("Relative drag events: {}", observed.drag_events);
        println!(
            "Signs: +X={} +Y={} -X={} -Y={}",
            observed.positive_x, observed.positive_y, observed.negative_x, observed.negative_y
        );
        println!("Grab released: {}", !hook.is_running());

        if observed.missing_relative_events != 0 {
            return Err(
                io::Error::other("grab delivered physical motion without relative data").into(),
            );
        }
        if observed.relative_events == 0 {
            return Err(io::Error::other("no physical relative motion was observed").into());
        }
        if observed.drag_events == 0 {
            return Err(io::Error::other("no physical MouseDragged event was observed").into());
        }
        if !observed.positive_x
            || !observed.positive_y
            || !observed.negative_x
            || !observed.negative_y
        {
            return Err(io::Error::other(
                "physical motion did not cover positive and negative X/Y directions",
            )
            .into());
        }

        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn records_relative_motion_signs_and_drag() {
            let mut observed = Observation::default();
            observed.observe(&Event::mouse_moved_relative(10.0, 20.0, 3.0, -2.0));
            observed.observe(&Event::mouse_dragged_relative(10.0, 20.0, -4.0, 5.0));

            assert_eq!(observed.relative_events, 2);
            assert_eq!(observed.drag_events, 1);
            assert!(observed.positive_x);
            assert!(observed.positive_y);
            assert!(observed.negative_x);
            assert!(observed.negative_y);
        }

        #[test]
        fn skips_this_session_injected_motion() {
            let mut event = Event::mouse_moved(10.0, 20.0);
            event.origin = InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession,
            };
            let mut observed = Observation::default();

            observed.observe(&event);

            assert_eq!(observed.missing_relative_events, 0);
        }
    }
}

#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows_diagnostic::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("windows_relative_grab_detection is only available on Windows");
}
