use crate::{InjectorIdentity, InputOrigin};

pub(crate) const LLKHF_INJECTED_BIT: u32 = 0x10;
pub(crate) const LLKHF_LOWER_IL_INJECTED_BIT: u32 = 0x02;
pub(crate) const LLMHF_INJECTED_BIT: u32 = 0x01;
pub(crate) const LLMHF_LOWER_IL_INJECTED_BIT: u32 = 0x02;
pub(crate) const NO_CONSOLE_SESSION: u32 = u32::MAX;

pub(crate) fn is_console_session(
    process_session: Option<u32>,
    active_console_session: u32,
) -> bool {
    active_console_session != NO_CONSOLE_SESSION
        && process_session == Some(active_console_session)
}

pub(crate) fn classify_windows_keyboard(
    flags: u32,
    observed_tag: usize,
    expected_tags: &[usize],
    on_console_session: bool,
) -> InputOrigin {
    let injected = flags & LLKHF_INJECTED_BIT != 0;
    let lower_il_injected = flags & LLKHF_LOWER_IL_INJECTED_BIT != 0;
    classify_windows_flags(
        injected,
        lower_il_injected,
        observed_tag,
        expected_tags,
        on_console_session,
    )
}

pub(crate) fn classify_windows_mouse(
    flags: u32,
    observed_tag: usize,
    expected_tags: &[usize],
    on_console_session: bool,
) -> InputOrigin {
    let injected = flags & LLMHF_INJECTED_BIT != 0;
    let lower_il_injected = flags & LLMHF_LOWER_IL_INJECTED_BIT != 0;
    classify_windows_flags(
        injected,
        lower_il_injected,
        observed_tag,
        expected_tags,
        on_console_session,
    )
}

fn classify_windows_flags(
    injected: bool,
    lower_il_injected: bool,
    observed_tag: usize,
    expected_tags: &[usize],
    on_console_session: bool,
) -> InputOrigin {
    if injected && observed_tag != 0 && expected_tags.contains(&observed_tag) {
        InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        }
    } else if !injected && !lower_il_injected && on_console_session {
        InputOrigin::Physical
    } else {
        InputOrigin::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TAGS: [usize; 2] = [0x1111_2222, 0x3333_4444];

    #[test]
    fn console_session_requires_matching_valid_session_id() {
        assert!(is_console_session(Some(1), 1));
        assert!(!is_console_session(Some(2), 1));
        assert!(!is_console_session(None, 1));
        assert!(!is_console_session(None, NO_CONSOLE_SESSION));
        assert!(!is_console_session(
            Some(NO_CONSOLE_SESSION),
            NO_CONSOLE_SESSION
        ));
    }

    #[test]
    fn windows_keyboard_origin_matrix() {
        let ours = InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        };

        assert_eq!(
            classify_windows_keyboard(0, 0, &TEST_TAGS, true),
            InputOrigin::Physical
        );
        assert_eq!(
            classify_windows_keyboard(0, 0, &TEST_TAGS, false),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_keyboard(LLKHF_INJECTED_BIT, TEST_TAGS[0], &TEST_TAGS, true),
            ours
        );
        assert_eq!(
            classify_windows_keyboard(LLKHF_INJECTED_BIT, TEST_TAGS[0], &TEST_TAGS, false),
            ours
        );
        assert_eq!(
            classify_windows_keyboard(LLKHF_INJECTED_BIT, 0x5852_4F53, &TEST_TAGS, true),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_keyboard(LLKHF_INJECTED_BIT, 0, &TEST_TAGS, true),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_keyboard(LLKHF_LOWER_IL_INJECTED_BIT, 0, &TEST_TAGS, true),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_keyboard(
                LLKHF_INJECTED_BIT | LLKHF_LOWER_IL_INJECTED_BIT,
                0,
                &TEST_TAGS,
                true
            ),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_keyboard(
                LLKHF_INJECTED_BIT | LLKHF_LOWER_IL_INJECTED_BIT,
                TEST_TAGS[1],
                &TEST_TAGS,
                true
            ),
            ours
        );
    }

    #[test]
    fn windows_mouse_origin_matrix() {
        let ours = InputOrigin::Injected {
            injector: InjectorIdentity::ThisMonioSession,
        };

        assert_eq!(
            classify_windows_mouse(0, 0, &TEST_TAGS, true),
            InputOrigin::Physical
        );
        assert_eq!(
            classify_windows_mouse(0, 0, &TEST_TAGS, false),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_mouse(LLMHF_INJECTED_BIT, TEST_TAGS[1], &TEST_TAGS, true),
            ours
        );
        assert_eq!(
            classify_windows_mouse(LLMHF_INJECTED_BIT, 0x5852_4F53, &TEST_TAGS, true),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_mouse(LLMHF_LOWER_IL_INJECTED_BIT, 0, &TEST_TAGS, true),
            InputOrigin::Unknown
        );
        assert_eq!(
            classify_windows_mouse(
                LLMHF_INJECTED_BIT | LLMHF_LOWER_IL_INJECTED_BIT,
                0,
                &TEST_TAGS,
                true
            ),
            InputOrigin::Unknown
        );
    }
}
