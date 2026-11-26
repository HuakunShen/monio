use super::hid_sender::{self, SenderKind};
use crate::{Error, InjectorIdentity, InputOrigin, Result};
use objc2_core_graphics::{CGEvent, CGEventField};
use std::sync::OnceLock;

/// Undocumented CoreGraphics integer event field carrying the IOHIDService
/// registry ID of the originating HID service. Pinned by a unit test; Apple may
/// renumber it, in which case lookups fail closed to `InputOrigin::Unknown`.
const HID_SENDER_FIELD: u32 = 87;

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
    session_tag()?;
    hid_sender::initialize();
    Ok(())
}

pub(super) fn tag_event(event: &CGEvent) -> Result<()> {
    set_event_tag(event, session_tag()?);
    Ok(())
}

pub(super) fn event_origin(event: &CGEvent) -> InputOrigin {
    let Some(Ok(expected)) = SESSION_TAG.get() else {
        return InputOrigin::Unknown;
    };
    let observed_tag = read_event_tag(event);
    let observed_pid = read_event_source_pid(event);
    let sender_id = read_event_hid_sender(event);
    let sender_kind = if observed_pid == 0 && observed_tag == 0 {
        hid_sender::lookup(sender_id)
    } else {
        None
    };
    classify_source(
        observed_tag,
        *expected,
        observed_pid,
        i64::from(std::process::id()),
        sender_id,
        sender_kind,
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

fn read_event_hid_sender(event: &CGEvent) -> u64 {
    CGEvent::integer_value_field(Some(event), CGEventField(HID_SENDER_FIELD)) as u64
}

fn classify_source(
    observed_tag: i64,
    expected_tag: i64,
    observed_pid: i64,
    expected_pid: i64,
    sender_id: u64,
    sender_kind: Option<SenderKind>,
) -> InputOrigin {
    if observed_tag != 0
        && observed_tag == expected_tag
        && observed_pid > 0
        && observed_pid == expected_pid
    {
        InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        }
    } else if observed_pid == 0
        && observed_tag == 0
        && sender_id != 0
        && sender_kind == Some(SenderKind::Physical)
    {
        InputOrigin::Physical
    } else {
        InputOrigin::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_core_graphics::{CGEvent, CGEventSource, CGEventSourceStateID};

    const PHYSICAL_SENDER_ID: u64 = 0x0001_0006_177d;
    const VIRTUAL_SENDER_ID: u64 = 0x007b_cad0_a800_0e4e;

    #[test]
    fn hid_sender_field_is_pinned_to_87() {
        assert_eq!(HID_SENDER_FIELD, 87);
    }

    #[test]
    fn exact_session_tag_is_classified_as_this_monio_session() {
        assert_eq!(
            classify_source(0x1234, 0x1234, 4000, 4000, 0, None),
            InputOrigin::Injected {
                injector: InjectorIdentity::ThisMonioSession,
            }
        );
    }

    #[test]
    fn zero_tag_is_never_classified_as_this_monio_session() {
        assert_eq!(
            classify_source(0, 0, 4000, 4000, PHYSICAL_SENDER_ID, Some(SenderKind::Physical)),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn mismatched_tag_remains_unknown() {
        assert_eq!(
            classify_source(
                0x1235,
                0x1234,
                4000,
                4000,
                PHYSICAL_SENDER_ID,
                Some(SenderKind::Physical)
            ),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn matching_tag_from_another_process_remains_unknown() {
        assert_eq!(
            classify_source(
                0x1234,
                0x1234,
                4000,
                4001,
                PHYSICAL_SENDER_ID,
                Some(SenderKind::Physical)
            ),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn zero_pid_and_zero_tag_with_physical_sender_is_classified_as_physical() {
        assert_eq!(
            classify_source(
                0,
                0x1234,
                0,
                4000,
                PHYSICAL_SENDER_ID,
                Some(SenderKind::Physical)
            ),
            InputOrigin::Physical
        );
    }

    #[test]
    fn zero_pid_and_zero_tag_with_virtual_sender_remains_unknown() {
        assert_eq!(
            classify_source(
                0,
                0x1234,
                0,
                4000,
                VIRTUAL_SENDER_ID,
                Some(SenderKind::Virtual)
            ),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn zero_pid_and_zero_tag_with_missing_sender_remains_unknown() {
        assert_eq!(
            classify_source(0, 0x1234, 0, 4000, PHYSICAL_SENDER_ID, None),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn zero_pid_and_zero_tag_with_zero_sender_id_remains_unknown() {
        assert_eq!(
            classify_source(0, 0x1234, 0, 4000, 0, Some(SenderKind::Physical)),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_source(0, 0x1234, 0, 4000, 0, None),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn foreign_process_with_zero_tag_remains_unknown() {
        assert_eq!(
            classify_source(
                0,
                0x1234,
                5000,
                4000,
                PHYSICAL_SENDER_ID,
                Some(SenderKind::Physical)
            ),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn negative_pid_with_zero_tag_remains_unknown() {
        assert_eq!(
            classify_source(
                0,
                0x1234,
                -1,
                4000,
                PHYSICAL_SENDER_ID,
                Some(SenderKind::Physical)
            ),
            InputOrigin::Unknown
        );
    }

    #[test]
    fn zero_pid_with_nonzero_tag_remains_unknown() {
        assert_eq!(
            classify_source(
                0x1234,
                0x1234,
                0,
                4000,
                PHYSICAL_SENDER_ID,
                Some(SenderKind::Physical)
            ),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_source(
                0x9999,
                0x1234,
                0,
                4000,
                PHYSICAL_SENDER_ID,
                Some(SenderKind::Physical)
            ),
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
