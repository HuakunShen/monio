use crate::{Error, InjectorIdentity, InputOrigin, Result};
use objc2_core_graphics::{CGEvent, CGEventField};
use std::sync::OnceLock;

static SESSION_TAG: OnceLock<std::result::Result<i64, String>> = OnceLock::new();

fn generate_session_tag() -> std::result::Result<i64, String> {
    loop {
        let tag = (getrandom::u64().map_err(|error| error.to_string())? & i64::MAX as u64) as i64;
        if tag != 0 {
            return Ok(tag);
        }
    }
}

fn session_tag() -> Result<i64> {
    match SESSION_TAG.get_or_init(generate_session_tag) {
        Ok(tag) => Ok(*tag),
        Err(message) => Err(Error::Platform(format!(
            "failed to initialize input injection tag: {message}"
        ))),
    }
}

pub(super) fn initialize() -> Result<()> {
    session_tag().map(|_| ())
}

pub(super) fn tag_event(event: &CGEvent) -> Result<()> {
    set_event_tag(event, session_tag()?);
    Ok(())
}

pub(super) fn event_origin(event: &CGEvent) -> InputOrigin {
    let Some(Ok(expected)) = SESSION_TAG.get() else {
        return InputOrigin::Unknown;
    };
    classify_source(
        read_event_tag(event),
        *expected,
        read_event_source_pid(event),
        i64::from(std::process::id()),
    )
}

fn set_event_tag(event: &CGEvent, tag: i64) {
    CGEvent::set_integer_value_field(Some(event), CGEventField::EventSourceUserData, tag);
}

fn read_event_tag(event: &CGEvent) -> i64 {
    CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData)
}

fn read_event_source_pid(event: &CGEvent) -> i64 {
    CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUnixProcessID)
}

fn classify_source(
    observed_tag: i64,
    expected_tag: i64,
    observed_pid: i64,
    expected_pid: i64,
) -> InputOrigin {
    if observed_tag != 0
        && observed_tag == expected_tag
        && observed_pid > 0
        && observed_pid == expected_pid
    {
        InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        }
    } else {
        InputOrigin::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_graphics::{CGEvent, CGEventSource, CGEventSourceStateID};

    #[test]
    fn exact_session_tag_is_classified_as_this_monio_session() {
        assert_eq!(
            classify_source(0x1234, 0x1234, 4000, 4000),
            InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession,
            }
        );
    }

    #[test]
    fn zero_tag_is_never_classified_as_this_monio_session() {
        assert_eq!(classify_source(0, 0, 4000, 4000), InputOrigin::Unknown);
    }

    #[test]
    fn mismatched_tag_remains_unknown() {
        assert_eq!(
            classify_source(0x1235, 0x1234, 4000, 4000),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn matching_tag_from_another_process_remains_unknown() {
        assert_eq!(
            classify_source(0x1234, 0x1234, 4000, 4001),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn cg_event_round_trips_session_tag() {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .expect("CGEventSource should be available");
        let event = CGEvent::new(Some(&source)).expect("CGEvent should be available");
        let tag = 0x1234_5678_9abc;

        set_event_tag(&event, tag);

        assert_eq!(read_event_tag(&event), tag);
    }

    #[test]
    fn process_session_tag_is_nonzero_and_stable() {
        let first = session_tag().expect("session tag should initialize");
        let second = session_tag().expect("session tag should be reusable");

        assert_ne!(first, 0);
        assert_eq!(first, second);
    }

    #[test]
    fn tagged_cg_event_is_classified_as_this_monio_session() {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .expect("CGEventSource should be available");
        let event = CGEvent::new(Some(&source)).expect("CGEvent should be available");

        tag_event(&event).expect("event should be tagged");

        assert_eq!(
            event_origin(&event),
            InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession,
            }
        );
    }
}
