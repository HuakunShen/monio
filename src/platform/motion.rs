use crate::event::Event;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Motion {
    Absolute { x: f64, y: f64 },
    Relative { delta_x: f64, delta_y: f64 },
}

#[cfg(any(all(target_os = "linux", feature = "evdev"), test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RelativeMotionSample {
    pub(crate) delta_x: i32,
    pub(crate) delta_y: i32,
    pub(crate) dragging: bool,
}

#[cfg(any(all(target_os = "linux", feature = "evdev"), test))]
#[derive(Debug, Default)]
pub(crate) struct RelativeMotionFrame {
    delta_x: i32,
    delta_y: i32,
    dragging: bool,
    has_motion: bool,
}

#[cfg(any(all(target_os = "linux", feature = "evdev"), test))]
impl RelativeMotionFrame {
    pub(crate) fn record(&mut self, delta_x: i32, delta_y: i32, dragging: bool) {
        self.delta_x = self.delta_x.saturating_add(delta_x);
        self.delta_y = self.delta_y.saturating_add(delta_y);
        self.dragging |= dragging;
        self.has_motion = true;
    }

    pub(crate) fn take(&mut self) -> Option<RelativeMotionSample> {
        if !self.has_motion {
            return None;
        }

        let completed = std::mem::take(self);
        Some(RelativeMotionSample {
            delta_x: completed.delta_x,
            delta_y: completed.delta_y,
            dragging: completed.dragging,
        })
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

pub(crate) fn motion_from_event(event: &Event) -> Option<Motion> {
    let mouse = event.mouse.as_ref()?;
    match mouse.relative {
        Some(relative) => Some(Motion::Relative {
            delta_x: relative.delta_x,
            delta_y: relative.delta_y,
        }),
        None => Some(Motion::Absolute {
            x: mouse.x,
            y: mouse.y,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{Motion, RelativeMotionFrame, RelativeMotionSample, motion_from_event};
    use crate::Event;

    #[test]
    fn absolute_event_dispatches_absolute_motion() {
        assert_eq!(
            motion_from_event(&Event::mouse_moved(10.0, 20.0)),
            Some(Motion::Absolute { x: 10.0, y: 20.0 })
        );
    }

    #[test]
    fn relative_event_dispatches_relative_motion() {
        assert_eq!(
            motion_from_event(&Event::mouse_moved_relative(100.0, 200.0, -4.0, 6.0,)),
            Some(Motion::Relative {
                delta_x: -4.0,
                delta_y: 6.0,
            })
        );
    }

    #[test]
    fn relative_frame_coalesces_axes_and_keeps_drag_state() {
        let mut frame = RelativeMotionFrame::default();

        frame.record(12, 0, false);
        frame.record(0, -7, true);

        assert_eq!(
            frame.take(),
            Some(RelativeMotionSample {
                delta_x: 12,
                delta_y: -7,
                dragging: true,
            })
        );
        assert_eq!(frame.take(), None);
    }

    #[test]
    fn relative_frame_can_discard_incomplete_motion() {
        let mut frame = RelativeMotionFrame::default();
        frame.record(12, -7, false);

        frame.clear();

        assert_eq!(frame.take(), None);
    }
}
