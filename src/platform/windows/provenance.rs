use crate::{Error, InjectorIdentity, InputOrigin, Result};
use std::sync::OnceLock;
use windows::Win32::UI::WindowsAndMessaging::{
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSLLHOOKSTRUCT,
};

#[derive(Clone, Copy)]
struct SessionTags {
    injection: usize,
    grab_replay: usize,
}

static SESSION_TAGS: OnceLock<std::result::Result<SessionTags, String>> = OnceLock::new();

fn generate_nonzero_u32_tag() -> std::result::Result<usize, String> {
    // The low-level mouse hook can retain only the low 32 bits of dwExtraInfo,
    // so keyboard and mouse simulation share a tag that round-trips through both.
    loop {
        let tag = getrandom::u32().map_err(|error| error.to_string())? as usize;
        if tag != 0 {
            return Ok(tag);
        }
    }
}

fn generate_session_tags() -> std::result::Result<SessionTags, String> {
    let injection = generate_nonzero_u32_tag()?;
    let grab_replay = loop {
        let candidate = generate_nonzero_u32_tag()?;
        if candidate != injection {
            break candidate;
        }
    };
    Ok(SessionTags {
        injection,
        grab_replay,
    })
}

fn tags() -> Result<SessionTags> {
    match SESSION_TAGS.get_or_init(generate_session_tags) {
        Ok(tags) => Ok(*tags),
        Err(message) => Err(Error::Platform(format!(
            "failed to initialize input injection tags: {message}"
        ))),
    }
}

pub(super) fn session_tag() -> Result<usize> {
    Ok(tags()?.injection)
}

pub(super) fn grab_replay_tag() -> Result<usize> {
    Ok(tags()?.grab_replay)
}

fn recognized_tags() -> Result<[usize; 2]> {
    let tags = tags()?;
    Ok([tags.injection, tags.grab_replay])
}

pub(super) fn initialize() -> Result<()> {
    tags().map(|_| ())
}

pub(super) fn keyboard_event_origin(event: &KBDLLHOOKSTRUCT) -> InputOrigin {
    let Some(Ok(tags)) = SESSION_TAGS.get() else {
        return InputOrigin::Unknown;
    };
    classify_source(
        event.flags.0 & LLKHF_INJECTED.0 != 0,
        event.dwExtraInfo,
        &[tags.injection, tags.grab_replay],
    )
}

pub(super) fn mouse_event_origin(event: &MSLLHOOKSTRUCT) -> InputOrigin {
    let Some(Ok(tags)) = SESSION_TAGS.get() else {
        return InputOrigin::Unknown;
    };
    classify_source(
        event.flags & LLMHF_INJECTED != 0,
        event.dwExtraInfo,
        &[tags.injection, tags.grab_replay],
    )
}

fn classify_source(is_injected: bool, observed_tag: usize, expected_tags: &[usize]) -> InputOrigin {
    if is_injected && observed_tag != 0 && expected_tags.contains(&observed_tag) {
        InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        }
    } else {
        InputOrigin::Unknown
    }
}

pub(super) fn is_grab_replay(event: &MSLLHOOKSTRUCT) -> bool {
    event.flags & LLMHF_INJECTED != 0
        && grab_replay_tag().is_ok_and(|expected| event.dwExtraInfo == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InjectorIdentity;

    #[test]
    fn injection_and_grab_replay_tags_are_distinct_nonzero_u32_values() {
        let injection = session_tag().expect("injection tag should initialize");
        let replay = grab_replay_tag().expect("replay tag should initialize");

        assert_ne!(injection, 0);
        assert_ne!(replay, 0);
        assert_ne!(injection, replay);
        assert_eq!(injection & !(u32::MAX as usize), 0);
        assert_eq!(replay & !(u32::MAX as usize), 0);
    }

    #[test]
    fn injected_input_from_either_process_tag_is_this_session() {
        let expected = InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        };

        assert_eq!(
            classify_source(true, session_tag().unwrap(), &recognized_tags().unwrap()),
            expected
        );
        assert_eq!(
            classify_source(
                true,
                grab_replay_tag().unwrap(),
                &recognized_tags().unwrap()
            ),
            expected
        );
    }

    #[test]
    fn only_the_private_replay_tag_bypasses_grab_dispatch() {
        let replay = MSLLHOOKSTRUCT {
            flags: LLMHF_INJECTED,
            dwExtraInfo: grab_replay_tag().unwrap(),
            ..Default::default()
        };
        let ordinary = MSLLHOOKSTRUCT {
            flags: LLMHF_INJECTED,
            dwExtraInfo: session_tag().unwrap(),
            ..Default::default()
        };

        assert!(is_grab_replay(&replay));
        assert!(!is_grab_replay(&ordinary));
    }
}
