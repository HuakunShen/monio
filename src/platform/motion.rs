use crate::event::Event;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Motion {
    Absolute { x: f64, y: f64 },
    Relative { delta_x: f64, delta_y: f64 },
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
    use super::{Motion, motion_from_event};
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
}
