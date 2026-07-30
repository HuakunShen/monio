use crate::{Error, InjectorIdentity, InputOrigin, Result};
use std::sync::OnceLock;
use windows::Win32::UI::WindowsAndMessaging::{
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSLLHOOKSTRUCT,
};

static SESSION_TAG: OnceLock<std::result::Result<usize, String>> = OnceLock::new();

fn generate_session_tag() -> std::result::Result<usize, String> {
    // The low-level mouse hook can retain only the low 32 bits of dwExtraInfo,
    // so keyboard and mouse simulation share a tag that round-trips through both.
    loop {
        let tag = getrandom::u32().map_err(|error| error.to_string())? as usize;
        if tag != 0 {
            return Ok(tag);
        }
    }
}

pub(super) fn session_tag() -> Result<usize> {
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

pub(super) fn keyboard_event_origin(event: &KBDLLHOOKSTRUCT) -> InputOrigin {
    let Some(Ok(expected_tag)) = SESSION_TAG.get() else {
        return InputOrigin::Unknown;
    };
    classify_source(
        event.flags.0 & LLKHF_INJECTED.0 != 0,
        event.dwExtraInfo,
        *expected_tag,
    )
}

pub(super) fn mouse_event_origin(event: &MSLLHOOKSTRUCT) -> InputOrigin {
    let Some(Ok(expected_tag)) = SESSION_TAG.get() else {
        return InputOrigin::Unknown;
    };
    classify_source(
        event.flags & LLMHF_INJECTED != 0,
        event.dwExtraInfo,
        *expected_tag,
    )
}

fn classify_source(is_injected: bool, observed_tag: usize, expected_tag: usize) -> InputOrigin {
    if is_injected && observed_tag != 0 && observed_tag == expected_tag {
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
    use crate::InjectorIdentity;

    #[test]
    fn classifies_only_injected_input_with_the_exact_nonzero_session_tag() {
        let this_session = InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        };
        let cases = [
            (true, 0x1234, 0x1234, this_session),
            (false, 0x1234, 0x1234, InputOrigin::Unknown),
            (true, 0x1235, 0x1234, InputOrigin::Unknown),
            (true, 0, 0, InputOrigin::Unknown),
        ];

        for (is_injected, observed_tag, expected_tag, expected) in cases {
            assert_eq!(
                classify_source(is_injected, observed_tag, expected_tag),
                expected
            );
        }
    }

    #[test]
    fn process_session_tag_is_nonzero_and_stable() {
        let first = session_tag().expect("session tag should initialize");
        let second = session_tag().expect("session tag should be reusable");

        assert_ne!(first, 0);
        assert_eq!(first, second);
    }

    #[test]
    fn process_session_tag_fits_windows_mouse_extra_info_round_trip() {
        let tag = session_tag().expect("session tag should initialize");

        assert_eq!(tag & !(u32::MAX as usize), 0);
    }
}
