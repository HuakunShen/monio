//! Platform capability introspection for monio backends.

/// Scope at which a capture or interception feature operates on the active backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureScope {
    /// Not supported by the active backend.
    Unsupported,
    /// Events are observed, but delivery to the OS / other apps cannot be suppressed.
    ObserveOnly,
    /// Interception or capture is scoped to a focused application window.
    FocusedWindow,
    /// Global system-wide capture or interception.
    Global,
}

impl CaptureScope {
    /// Returns a concise human-readable label for diagnostics and UIs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "Unsupported",
            Self::ObserveOnly => "ObserveOnly",
            Self::FocusedWindow => "FocusedWindow",
            Self::Global => "Global",
        }
    }
}

/// Authorization model required before synthetic input injection takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectionAuthorization {
    /// Injection works without an explicit OS permission or user consent prompt
    /// (subject to standard OS privilege boundaries such as Windows UIPI).
    Direct,
    /// Requires an OS-level permission or device access grant (for example
    /// macOS Accessibility or Linux `/dev/uinput` permissions).
    OsPermission,
    /// Requires runtime user consent (such as HarmonyOS `OH_Input_RequestInjection`
    /// on API 20-25) or a privileged OS permission (`CONTROL_DEVICE` on API 26+).
    UserConsentOrPermission,
}

impl InjectionAuthorization {
    /// Returns a concise human-readable label for diagnostics and UIs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "Direct",
            Self::OsPermission => "OsPermission",
            Self::UserConsentOrPermission => "UserConsentOrPermission",
        }
    }

    /// Returns `true` when synthetic injection requires an interactive runtime
    /// user-consent flow unless a privileged OS permission is held.
    #[must_use]
    pub const fn requires_runtime_consent(self) -> bool {
        matches!(self, Self::UserConsentOrPermission)
    }
}

/// Compile-time capability profile for the active platform backend.
///
/// A `true` or non-`Unsupported` field indicates that the backend implements
/// the capability; runtime OS permissions (for example Accessibility on macOS
/// or `CONTROL_DEVICE` / `INPUT_MONITORING` on HarmonyOS) may still be required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputCapabilities {
    /// Global or scoped keyboard event listening (`Hook::run`).
    pub keyboard_listen: CaptureScope,
    /// Keyboard event interception / suppression (`Hook::grab`).
    pub keyboard_grab: CaptureScope,
    /// Pointer (mouse / wheel) event listening (`Hook::run`).
    pub pointer_listen: CaptureScope,
    /// Pointer event interception / suppression (`Hook::grab`).
    pub pointer_grab: CaptureScope,
    /// Process-wide cursor-disassociated relative motion capture (`RelativePointerCapture`).
    pub relative_pointer_capture: CaptureScope,
    /// Synthetic keyboard event injection (`key_press`, `key_tap`, `simulate`).
    pub keyboard_injection: bool,
    /// Synthetic mouse button / move event injection (`mouse_click`, `mouse_move`, `simulate`).
    pub pointer_injection: bool,
    /// Synthetic phased scroll injection (`scroll`, `simulate`).
    pub scroll_injection: bool,
    /// Synthetic media key injection (`media_key`).
    pub media_key_injection: bool,
    /// Global Unicode string injection (`type_text`).
    pub unicode_text_injection: bool,
    /// Synthetic trackpad pinch / rotate gesture injection (`magnify`, `rotate`, `smart_magnify`).
    pub gesture_injection: bool,
    /// Display topology enumeration (`displays`, `primary_display`, `display_at_point`).
    pub display_enumeration: bool,
    /// OS input settings query (`system_settings`).
    pub system_settings_query: bool,
    /// Whether `mouse_position()` works reliably when no app window is focused.
    pub background_pointer_query: bool,
    /// Whether listened events reliably distinguish `InputOrigin::ThisMonioSession`.
    pub injected_event_identification: bool,
    /// Authorization model required for synthetic input injection.
    pub injection_authorization: InjectionAuthorization,
}

/// Returns the capability profile for the compiled target backend.
#[must_use]
pub const fn capabilities() -> InputCapabilities {
    if cfg!(target_env = "ohos") {
        InputCapabilities {
            keyboard_listen: CaptureScope::Global,
            keyboard_grab: CaptureScope::Global,
            pointer_listen: CaptureScope::Global,
            pointer_grab: CaptureScope::ObserveOnly,
            relative_pointer_capture: CaptureScope::Unsupported,
            keyboard_injection: true,
            pointer_injection: true,
            scroll_injection: true,
            media_key_injection: true,
            unicode_text_injection: false,
            gesture_injection: false,
            display_enumeration: true,
            system_settings_query: false,
            background_pointer_query: false,
            injected_event_identification: false,
            injection_authorization: InjectionAuthorization::UserConsentOrPermission,
        }
    } else if cfg!(target_os = "macos") {
        InputCapabilities {
            keyboard_listen: CaptureScope::Global,
            keyboard_grab: CaptureScope::Global,
            pointer_listen: CaptureScope::Global,
            pointer_grab: CaptureScope::Global,
            relative_pointer_capture: CaptureScope::Global,
            keyboard_injection: true,
            pointer_injection: true,
            scroll_injection: true,
            media_key_injection: true,
            unicode_text_injection: true,
            gesture_injection: true,
            display_enumeration: true,
            system_settings_query: true,
            background_pointer_query: true,
            injected_event_identification: true,
            injection_authorization: InjectionAuthorization::OsPermission,
        }
    } else if cfg!(target_os = "windows") {
        InputCapabilities {
            keyboard_listen: CaptureScope::Global,
            keyboard_grab: CaptureScope::Global,
            pointer_listen: CaptureScope::Global,
            pointer_grab: CaptureScope::Global,
            relative_pointer_capture: CaptureScope::Unsupported,
            keyboard_injection: true,
            pointer_injection: true,
            scroll_injection: true,
            media_key_injection: true,
            unicode_text_injection: false,
            gesture_injection: false,
            display_enumeration: true,
            system_settings_query: true,
            background_pointer_query: true,
            injected_event_identification: true,
            injection_authorization: InjectionAuthorization::Direct,
        }
    } else if cfg!(all(target_os = "linux", feature = "evdev")) {
        InputCapabilities {
            keyboard_listen: CaptureScope::Global,
            keyboard_grab: CaptureScope::Global,
            pointer_listen: CaptureScope::Global,
            pointer_grab: CaptureScope::Global,
            relative_pointer_capture: CaptureScope::Global,
            keyboard_injection: true,
            pointer_injection: true,
            scroll_injection: true,
            media_key_injection: true,
            unicode_text_injection: false,
            gesture_injection: false,
            display_enumeration: true,
            system_settings_query: true,
            background_pointer_query: true,
            injected_event_identification: false,
            injection_authorization: InjectionAuthorization::OsPermission,
        }
    } else {
        // Linux X11 (default)
        InputCapabilities {
            keyboard_listen: CaptureScope::Global,
            keyboard_grab: CaptureScope::Global,
            pointer_listen: CaptureScope::Global,
            pointer_grab: CaptureScope::Global,
            relative_pointer_capture: CaptureScope::Global,
            keyboard_injection: true,
            pointer_injection: true,
            scroll_injection: true,
            media_key_injection: true,
            unicode_text_injection: false,
            gesture_injection: false,
            display_enumeration: true,
            system_settings_query: true,
            background_pointer_query: true,
            injected_event_identification: false,
            injection_authorization: InjectionAuthorization::Direct,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_matches_active_target() {
        let caps = capabilities();
        assert!(caps.keyboard_injection);
        assert!(caps.pointer_injection);
        assert!(caps.scroll_injection);
        assert!(caps.media_key_injection);
        assert!(caps.display_enumeration);
        assert_eq!(caps.keyboard_grab, CaptureScope::Global);

        if cfg!(target_env = "ohos") {
            assert_eq!(caps.pointer_grab, CaptureScope::ObserveOnly);
            assert_eq!(caps.relative_pointer_capture, CaptureScope::Unsupported);
            assert!(!caps.system_settings_query);
            assert_eq!(
                caps.injection_authorization,
                InjectionAuthorization::UserConsentOrPermission
            );
            assert!(caps.injection_authorization.requires_runtime_consent());
        } else if cfg!(target_os = "macos") {
            assert_eq!(caps.pointer_grab, CaptureScope::Global);
            assert_eq!(caps.relative_pointer_capture, CaptureScope::Global);
            assert!(caps.unicode_text_injection);
            assert!(caps.gesture_injection);
            assert_eq!(
                caps.injection_authorization,
                InjectionAuthorization::OsPermission
            );
        }
    }
}
