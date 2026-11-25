//! HarmonyOS scroll injection through Input Kit mouse axis events.

use super::display::scroll_pointer_position;
use super::simulate::NativeMouseEvent;
use super::translate::plan_scroll_steps;
use crate::error::Result;
use crate::scroll::ScrollPhase;

/// Inject scroll events at the current global pointer location.
pub fn scroll(delta_x: f64, delta_y: f64, phase: ScrollPhase) -> Result<()> {
    let steps = plan_scroll_steps(delta_x, delta_y, phase)?;
    if steps.is_empty() {
        return Ok(());
    }

    let (x, y) = scroll_pointer_position();
    let global_x = x.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
    let global_y = y.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
    let event = NativeMouseEvent::new()?;

    for step in steps {
        event.inject_axis(
            step.action,
            step.axis_type,
            step.axis_value,
            global_x,
            global_y,
        )?;
    }

    Ok(())
}
