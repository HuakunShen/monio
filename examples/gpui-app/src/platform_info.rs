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
    let caps = monio::capabilities();
    if cfg!(target_env = "ohos") {
        vec![
            CapabilityRow {
                capability: "Keyboard Listen / Grab",
                support: caps.keyboard_grab.as_str(),
                notes: "Monitor (INPUT_MONITORING) & KeyHook (HOOK_KEY_EVENT)",
            },
            CapabilityRow {
                capability: "Pointer Listen",
                support: caps.pointer_listen.as_str(),
                notes: "Mouse/axis monitors (INPUT_MONITORING; OS may require screen-recording scenario)",
            },
            CapabilityRow {
                capability: "Pointer Grab (global)",
                support: caps.pointer_grab.as_str(),
                notes: "Generic grab() is observe-only for pointer; window-hit Interceptor exists in Input Kit",
            },
            CapabilityRow {
                capability: "Key, Mouse, Scroll & Media",
                support: "Supported",
                notes: "OH_Input_Inject*Global / InjectKeyEvent (PC / 2-in-1; CONTROL_DEVICE)",
            },
            CapabilityRow {
                capability: "type_text() & Gestures",
                support: "Unsupported",
                notes: "No global Unicode or pinch/rotate injection API in Input Kit",
            },
            CapabilityRow {
                capability: "displays() / mouse_position()",
                support: "Supported",
                notes: "Displays via libnative_display_manager.so; mouse_position() maps display_id to global coords",
            },
            CapabilityRow {
                capability: "RelativePointerCapture",
                support: caps.relative_pointer_capture.as_str(),
                notes: "No global cursor-lock API; Window Kit LockCursor + ArkUI RawDelta require a focused window",
            },
            CapabilityRow {
                capability: "Channels / Recorder / Stats",
                support: "Supported",
                notes: "Built on Hook; inherits underlying Listen/Grab scope & permissions",
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
