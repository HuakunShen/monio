use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureKind {
    PermissionDenied,
    Unsupported,
    InvalidParameter,
    Service,
    Conflict,
    Other(u32),
}

pub(crate) fn classify_code(code: u32) -> FailureKind {
    match code {
        201 => FailureKind::PermissionDenied,
        401 | 26_500_001 => FailureKind::InvalidParameter,
        801 | 3_900_002 | 3_900_010 => FailureKind::Unsupported,
        3_800_001 => FailureKind::Service,
        4_200_001..=4_200_003 => FailureKind::Conflict,
        other => FailureKind::Other(other),
    }
}

fn details(operation: &str, code: u32) -> String {
    format!("{operation} failed with HarmonyOS Input Kit result {code}")
}

pub(crate) fn hook_start_error(operation: &str, code: u32, permission: &str) -> Error {
    let details = details(operation, code);
    match classify_code(code) {
        FailureKind::PermissionDenied => {
            Error::PermissionDenied(format!("{details}; required permission: {permission}"))
        }
        FailureKind::Unsupported => Error::NotSupported(details),
        _ => Error::HookStartFailed(details),
    }
}

pub(crate) fn hook_stop_error(operation: &str, code: u32) -> Error {
    Error::HookStopFailed(details(operation, code))
}

pub(crate) fn simulate_error(operation: &str, code: u32) -> Error {
    let details = details(operation, code);
    match classify_code(code) {
        FailureKind::PermissionDenied => Error::PermissionDenied(format!(
            "{details}; required permission: CONTROL_DEVICE"
        )),
        FailureKind::Unsupported => Error::NotSupported(details),
        _ => Error::SimulateFailed(details),
    }
}

pub(crate) fn platform_error(operation: &str, code: u32) -> Error {
    let details = details(operation, code);
    match classify_code(code) {
        FailureKind::PermissionDenied => Error::PermissionDenied(details),
        FailureKind::Unsupported => Error::NotSupported(details),
        _ => Error::Platform(details),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;

    #[test]
    fn classifies_documented_input_kit_codes() {
        let cases = [
            (201, FailureKind::PermissionDenied),
            (801, FailureKind::Unsupported),
            (401, FailureKind::InvalidParameter),
            (3_800_001, FailureKind::Service),
            (4_200_001, FailureKind::Conflict),
            (123_456, FailureKind::Other(123_456)),
        ];

        for (code, expected) in cases {
            assert_eq!(classify_code(code), expected);
        }
    }

    #[test]
    fn maps_hook_start_errors_with_operation_permission_and_code() {
        let permission =
            hook_start_error("OH_Input_AddKeyEventMonitor", 201, "INPUT_MONITORING");
        assert!(matches!(permission, Error::PermissionDenied(_)));
        let message = permission.to_string();
        assert!(message.contains("OH_Input_AddKeyEventMonitor"));
        assert!(message.contains("INPUT_MONITORING"));
        assert!(message.contains("201"));

        let unsupported =
            hook_start_error("OH_Input_AddKeyEventHook", 801, "HOOK_KEY_EVENT");
        assert!(matches!(unsupported, Error::NotSupported(_)));
        assert!(unsupported.to_string().contains("801"));

        let service =
            hook_start_error("OH_Input_AddMouseEventMonitor", 3_800_001, "INPUT_MONITORING");
        assert!(matches!(service, Error::HookStartFailed(_)));
        assert!(service.to_string().contains("3800001"));
    }

    #[test]
    fn maps_simulation_permission_to_control_device() {
        let error = simulate_error("OH_Input_InjectKeyEvent", 201);

        assert!(matches!(error, Error::PermissionDenied(_)));
        assert!(error.to_string().contains("CONTROL_DEVICE"));
        assert!(error.to_string().contains("OH_Input_InjectKeyEvent"));
        assert!(error.to_string().contains("201"));
    }

    #[test]
    fn operation_specific_mappers_keep_the_native_code() {
        let stop = hook_stop_error("OH_Input_RemoveKeyEventHook", 401);
        assert!(matches!(stop, Error::HookStopFailed(_)));
        assert!(stop.to_string().contains("401"));

        let platform = platform_error("OH_Input_GetPointerLocation", 3_800_001);
        assert!(matches!(platform, Error::Platform(_)));
        assert!(platform.to_string().contains("3800001"));
    }
}
