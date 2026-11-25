use monio::Key;

#[derive(Debug, Clone, Copy)]
pub struct CapabilityRow {
    pub capability: &'static str,
    pub support: &'static str,
    pub notes: &'static str,
}

pub const fn current_platform_name() -> &'static str {
    if cfg!(target_env = "ohos") {
        "HarmonyOS (OHOS)"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "linux") {
        "Linux"
    } else {
        "Unknown"
    }
}

pub const fn select_all_modifier_key() -> Key {
    if cfg!(target_os = "macos") {
        Key::MetaLeft
    } else {
        Key::ControlLeft
    }
}

pub const fn select_all_shortcut_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd + A"
    } else {
        "Ctrl + A"
    }
}

pub const fn permission_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "Enable Accessibility permission in System Settings -> Privacy & Security -> Accessibility."
    } else if cfg!(target_env = "ohos") {
        "Ensure INPUT_MONITORING, HOOK_KEY_EVENT, and CONTROL_DEVICE permissions are granted in HarmonyOS Settings."
    } else if cfg!(target_os = "linux") {
        "On Wayland/evdev, join the 'input' group and verify /dev/uinput access; on X11 ensure XRecord/XI2 extensions are enabled."
    } else {
        "Verify OS input hook permissions or target window elevation (UIPI on Windows)."
    }
}

pub fn capability_matrix() -> Vec<CapabilityRow> {
    if cfg!(target_env = "ohos") {
        vec![
            CapabilityRow {
                capability: "Listen (Hook)",
                support: "Supported",
                notes: "Key, mouse, and axis monitors (requires INPUT_MONITORING)",
            },
            CapabilityRow {
                capability: "Keyboard Grab",
                support: "Supported",
                notes: "Global key hook via OH_Input_AddKeyEventHook (HOOK_KEY_EVENT)",
            },
            CapabilityRow {
                capability: "Pointer Grab",
                support: "Observe-only",
                notes: "Mouse/axis monitors cannot suppress delivery in generic grab()",
            },
            CapabilityRow {
                capability: "Key & Mouse Simulate",
                support: "Supported",
                notes: "Requires API 26+ CONTROL_DEVICE permission",
            },
            CapabilityRow {
                capability: "Phased Scroll & Media Keys",
                support: "Supported",
                notes: "OH_Input_InjectMouseEventGlobal / OH_Input_InjectKeyEvent",
            },
            CapabilityRow {
                capability: "type_text()",
                support: "NotSupported",
                notes: "Input Kit has no global Unicode text-injection API",
            },
            CapabilityRow {
                capability: "displays() / primary_display()",
                support: "Supported",
                notes: "Via libnative_display_manager.so; system_settings() NotSupported",
            },
            CapabilityRow {
                capability: "RelativePointerCapture",
                support: "NotSupported",
                notes: "Returns Error::NotSupported on HarmonyOS",
            },
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            CapabilityRow {
                capability: "Listen & Full Grab",
                support: "Supported",
                notes: "CGEventTap for keyboard, mouse, wheel (requires Accessibility)",
            },
            CapabilityRow {
                capability: "Drag Detection",
                support: "Supported",
                notes: "Global atomic button mask separates MouseDragged vs MouseMoved",
            },
            CapabilityRow {
                capability: "type_text() / Phased Scroll / Media Keys",
                support: "Supported",
                notes: "CGEvent Unicode strings, scroll phases, and system-defined media keys",
            },
            CapabilityRow {
                capability: "displays() / system_settings()",
                support: "Supported",
                notes: "CoreGraphics displays, DPI scale, refresh rate, and input settings",
            },
            CapabilityRow {
                capability: "RelativePointerCapture",
                support: "Supported",
                notes: "Process-exclusive lease decoupling cursor from physical pointer",
            },
            CapabilityRow {
                capability: "InputOrigin Provenance",
                support: "Supported",
                notes: "Self-injected events tagged with ThisMonioSession",
            },
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            CapabilityRow {
                capability: "Listen & Full Grab",
                support: "Supported",
                notes: "WH_KEYBOARD_LL / WH_MOUSE_LL + WM_INPUT raw deltas during grab",
            },
            CapabilityRow {
                capability: "Key & Mouse Simulate",
                support: "Supported",
                notes: "SendInput (respects UIPI integrity levels)",
            },
            CapabilityRow {
                capability: "type_text() / Phased Scroll / Media Keys",
                support: "NotSupported",
                notes: "Returns Error::NotSupported on Windows",
            },
            CapabilityRow {
                capability: "displays() / system_settings()",
                support: "Supported",
                notes: "EnumDisplayMonitors + SystemParametersInfo",
            },
            CapabilityRow {
                capability: "RelativePointerCapture",
                support: "NotSupported",
                notes: "Returns Error::NotSupported on Windows",
            },
            CapabilityRow {
                capability: "InputOrigin Provenance",
                support: "Supported",
                notes: "Private SendInput extra-info tag identifies ThisMonioSession",
            },
        ]
    } else {
        vec![
            CapabilityRow {
                capability: "Listen & Grab",
                support: "Supported",
                notes: "X11 (XRecord + XI2) or evdev (/dev/input + uinput)",
            },
            CapabilityRow {
                capability: "Key & Mouse Simulate",
                support: "Supported",
                notes: "XTest (X11) or uinput (evdev)",
            },
            CapabilityRow {
                capability: "type_text() / Phased Scroll / Media Keys",
                support: "NotSupported",
                notes: "Returns Error::NotSupported on Linux",
            },
            CapabilityRow {
                capability: "displays() / system_settings()",
                support: "Supported",
                notes: "X11 display/settings queries",
            },
            CapabilityRow {
                capability: "RelativePointerCapture",
                support: "No-op lease",
                notes: "X11/evdev grab paths already expose relative deltas",
            },
            CapabilityRow {
                capability: "InputOrigin Provenance",
                support: "Unknown",
                notes: "Reports InputOrigin::Unknown on Linux backends",
            },
        ]
    }
}
