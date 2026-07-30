//! Bounded CrossFlow-style relative pointer capture diagnostic.
//!
//! The example owns local input for five seconds, prints relative pointer
//! deltas that a real source would send remotely, then restores the cursor and
//! stops the hook. On Windows it exits with `Error::NotSupported`.

use monio::{Event, EventType, Hook, RelativePointerCapture};
use std::thread;
use std::time::Duration;

fn main() -> monio::Result<()> {
    println!("Capturing local input for five seconds; restoration is automatic.");

    let capture = RelativePointerCapture::acquire()?;
    let hook = Hook::new();
    hook.grab_async(|event: &Event| {
        if matches!(
            event.event_type,
            EventType::MouseMoved | EventType::MouseDragged
        ) && let Some(relative) = event.mouse.as_ref().and_then(|mouse| mouse.relative)
        {
            println!(
                "route relative delta: ({:.3}, {:.3})",
                relative.delta_x, relative.delta_y
            );
        }

        None
    })?;

    thread::sleep(Duration::from_secs(5));

    capture.release()?;
    hook.stop()?;
    println!("Local cursor and input restored.");
    Ok(())
}
