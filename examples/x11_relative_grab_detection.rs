//! Native X11 diagnostic for relative pointer motion during an active grab.
//!
//! Manual:
//!
//! ```bash
//! cargo run --features x11 --example x11_relative_grab_detection
//! ```
//!
//! Automated:
//!
//! ```bash
//! xvfb-run -a cargo run --features x11 \
//!   --example x11_relative_grab_detection -- --self-test
//! ```

#[cfg(target_os = "linux")]
mod linux {
    use monio::{Event, EventType, Hook, Key, mouse_move_relative, mouse_position};
    use std::error::Error;
    use std::io;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    const HOOK_START_TIMEOUT: Duration = Duration::from_secs(5);
    const MANUAL_DURATION: Duration = Duration::from_secs(10);
    const SELF_TEST_DURATION: Duration = Duration::from_secs(2);
    const POLL_INTERVAL: Duration = Duration::from_millis(10);
    const ACTION_PAUSE: Duration = Duration::from_millis(100);

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

    fn inject_self_test() -> monio::Result<(bool, bool)> {
        let origin = mouse_position()?;
        mouse_move_relative(16.0, 12.0)?;
        sleep(ACTION_PAUSE);
        let target = mouse_position()?;
        mouse_move_relative(-16.0, -12.0)?;
        sleep(ACTION_PAUSE);
        let restored = mouse_position()?;

        let moved = target.0 > origin.0 && target.1 > origin.1;
        let returned = (restored.0 - origin.0).abs() <= 1.0 && (restored.1 - origin.1).abs() <= 1.0;
        Ok((moved, returned))
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let self_test = std::env::args().any(|arg| arg == "--self-test");
        let pass_through = std::env::args().any(|arg| arg == "--pass-through");

        println!("monio X11 relative grab diagnostic");
        println!("==================================");
        println!("This diagnostic temporarily grabs the X11 keyboard and pointer.");
        println!("Press Escape to stop; Ctrl+C and a ten-second timeout are fallbacks.");
        if self_test {
            println!("Self-test will inject right/down and left/up relative motion.");
            println!("XTest does not generate XI2 RawMotion; hardware capture is not asserted.");
        } else {
            println!("Move in every direction and continue pushing against a screen edge.");
            println!("Hold a mouse button while moving to verify MouseDragged.");
        }
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

        let self_test_motion = self_test.then(inject_self_test).transpose()?;

        let duration = if self_test {
            SELF_TEST_DURATION
        } else {
            MANUAL_DURATION
        };
        let deadline = Instant::now() + duration;
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
        if let Some((moved, returned)) = self_test_motion {
            println!("Relative injection moved right/down: {moved}");
            println!("Inverse relative injection restored origin: {returned}");
        }

        if observed.missing_relative_events != 0 {
            return Err(io::Error::other("grab delivered motion without XI2 relative data").into());
        }
        if let Some((moved, returned)) = self_test_motion
            && (!moved || !returned)
        {
            return Err(io::Error::other(
                "relative injection did not move and restore the X11 pointer",
            )
            .into());
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    linux::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("x11_relative_grab_detection is only available on Linux/X11");
}
