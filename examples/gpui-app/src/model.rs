use std::collections::VecDeque;

use monio::state::{
    MASK_ALT, MASK_BUTTON1, MASK_BUTTON2, MASK_BUTTON3, MASK_BUTTON4, MASK_BUTTON5, MASK_CAPS_LOCK,
    MASK_CTRL, MASK_META, MASK_SHIFT,
};
use monio::{DisplayInfo, Error, Event, EventType, InputOrigin, SystemSettings};

pub const MAX_LOG_ENTRIES: usize = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Monitor,
    Simulate,
    Displays,
}

impl ActiveTab {
    pub const ALL: [Self; 3] = [Self::Monitor, Self::Simulate, Self::Displays];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Monitor => "Monitor & Grab",
            Self::Simulate => "Simulate",
            Self::Displays => "Displays & Capabilities",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    Idle,
    Listening,
    Grabbing,
}

impl HookMode {
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Listening => "Listening",
            Self::Grabbing => "Grabbing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBanner {
    pub level: BannerLevel,
    pub title: String,
    pub detail: String,
}

impl StatusBanner {
    pub fn info(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level: BannerLevel::Info,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn success(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level: BannerLevel::Success,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn warning(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level: BannerLevel::Warning,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn from_monio_error(action: &str, error: &Error) -> Self {
        match error {
            Error::PermissionDenied(msg) => Self {
                level: BannerLevel::Error,
                title: format!("{action}: Permission Denied"),
                detail: format!("{msg}. {}", crate::platform_info::permission_hint()),
            },
            Error::NotSupported(msg) => Self {
                level: BannerLevel::Warning,
                title: format!("{action}: Not Supported"),
                detail: msg.clone(),
            },
            other => Self {
                level: BannerLevel::Error,
                title: format!("{action}: Failed"),
                detail: other.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventFilter {
    pub key_pressed: bool,
    pub key_released: bool,
    pub mouse_pressed: bool,
    pub mouse_released: bool,
    pub mouse_moved: bool,
    pub mouse_dragged: bool,
    pub mouse_wheel: bool,
}

impl Default for EventFilter {
    fn default() -> Self {
        Self {
            key_pressed: true,
            key_released: true,
            mouse_pressed: true,
            mouse_released: true,
            mouse_moved: true,
            mouse_dragged: true,
            mouse_wheel: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    KeyPressed,
    KeyReleased,
    MousePressed,
    MouseReleased,
    MouseMoved,
    MouseDragged,
    MouseWheel,
}

impl FilterField {
    pub const ALL: [Self; 7] = [
        Self::KeyPressed,
        Self::KeyReleased,
        Self::MousePressed,
        Self::MouseReleased,
        Self::MouseMoved,
        Self::MouseDragged,
        Self::MouseWheel,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::KeyPressed => "KeyPressed",
            Self::KeyReleased => "KeyReleased",
            Self::MousePressed => "MousePressed",
            Self::MouseReleased => "MouseReleased",
            Self::MouseMoved => "MouseMoved",
            Self::MouseDragged => "MouseDragged",
            Self::MouseWheel => "MouseWheel",
        }
    }
}

impl EventFilter {
    pub fn get(&self, field: FilterField) -> bool {
        match field {
            FilterField::KeyPressed => self.key_pressed,
            FilterField::KeyReleased => self.key_released,
            FilterField::MousePressed => self.mouse_pressed,
            FilterField::MouseReleased => self.mouse_released,
            FilterField::MouseMoved => self.mouse_moved,
            FilterField::MouseDragged => self.mouse_dragged,
            FilterField::MouseWheel => self.mouse_wheel,
        }
    }

    pub fn toggle(&mut self, field: FilterField) {
        let slot = match field {
            FilterField::KeyPressed => &mut self.key_pressed,
            FilterField::KeyReleased => &mut self.key_released,
            FilterField::MousePressed => &mut self.mouse_pressed,
            FilterField::MouseReleased => &mut self.mouse_released,
            FilterField::MouseMoved => &mut self.mouse_moved,
            FilterField::MouseDragged => &mut self.mouse_dragged,
            FilterField::MouseWheel => &mut self.mouse_wheel,
        };
        *slot = !*slot;
    }

    pub fn set_all(&mut self, enabled: bool) {
        self.key_pressed = enabled;
        self.key_released = enabled;
        self.mouse_pressed = enabled;
        self.mouse_released = enabled;
        self.mouse_moved = enabled;
        self.mouse_dragged = enabled;
        self.mouse_wheel = enabled;
    }

    pub fn matches(&self, event_type: EventType) -> bool {
        match event_type {
            EventType::HookEnabled | EventType::HookDisabled => true,
            EventType::KeyPressed | EventType::KeyTyped => self.key_pressed,
            EventType::KeyReleased => self.key_released,
            EventType::MousePressed => self.mouse_pressed,
            EventType::MouseReleased | EventType::MouseClicked => self.mouse_released,
            EventType::MouseMoved => self.mouse_moved,
            EventType::MouseDragged => self.mouse_dragged,
            EventType::MouseWheel => self.mouse_wheel,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventLogEntry {
    pub seq: u64,
    pub event_type: EventType,
    pub event_name: &'static str,
    pub detail: String,
    pub coords: String,
    pub extra: String,
    pub origin_label: &'static str,
    pub blocked: bool,
}

impl EventLogEntry {
    pub fn from_event(seq: u64, event: &Event, blocked: bool) -> Self {
        let event_name = match event.event_type {
            EventType::HookEnabled => "HookEnabled",
            EventType::HookDisabled => "HookDisabled",
            EventType::KeyPressed => "KeyPressed",
            EventType::KeyReleased => "KeyReleased",
            EventType::KeyTyped => "KeyTyped",
            EventType::MousePressed => "MousePressed",
            EventType::MouseReleased => "MouseReleased",
            EventType::MouseClicked => "MouseClicked",
            EventType::MouseMoved => "MouseMoved",
            EventType::MouseDragged => "MouseDragged",
            EventType::MouseWheel => "MouseWheel",
        };

        let mut detail = String::from("-");
        let mut coords = String::from("-");
        let mut extra = String::from("-");

        if let Some(kb) = &event.keyboard {
            detail = if let Some(ch) = kb.char {
                format!("{:?} ('{}', raw={})", kb.key, ch, kb.raw_code)
            } else {
                format!("{:?} (raw={})", kb.key, kb.raw_code)
            };
        } else if let Some(mouse) = &event.mouse {
            detail = match mouse.button {
                Some(btn) if mouse.clicks > 0 => format!("{btn:?} (clicks={})", mouse.clicks),
                Some(btn) => format!("{btn:?}"),
                None => String::from("Pointer"),
            };
            coords = format!("({:.1}, {:.1})", mouse.x, mouse.y);
            if let Some(rel) = mouse.relative {
                extra = format!("rel=({:+.1}, {:+.1})", rel.delta_x, rel.delta_y);
            }
        } else if let Some(wheel) = &event.wheel {
            detail = format!("{:?}", wheel.direction);
            coords = format!("({:.1}, {:.1})", wheel.x, wheel.y);
            extra = format!("delta={:.2}", wheel.delta);
        }

        let origin_label = match event.origin {
            InputOrigin::Unknown => "Unknown",
            InputOrigin::Injected { .. } => "Injected(Session)",
            _ => "Injected(Other)",
        };

        Self {
            seq,
            event_type: event.event_type,
            event_name,
            detail,
            coords,
            extra,
            origin_label,
            blocked,
        }
    }
}

pub fn push_bounded_log(log: &mut VecDeque<EventLogEntry>, entry: EventLogEntry, limit: usize) {
    if log.len() >= limit {
        log.pop_back();
    }
    log.push_front(entry);
}

pub fn format_mask_flags(mask: u32) -> String {
    let mut parts = Vec::new();
    let flags = [
        (MASK_BUTTON1, "LMB"),
        (MASK_BUTTON2, "RMB"),
        (MASK_BUTTON3, "MMB"),
        (MASK_BUTTON4, "B4"),
        (MASK_BUTTON5, "B5"),
        (MASK_SHIFT, "SHIFT"),
        (MASK_CTRL, "CTRL"),
        (MASK_ALT, "ALT"),
        (MASK_META, "META"),
        (MASK_CAPS_LOCK, "CAPS"),
    ];
    for (bit, name) in flags {
        if (mask & bit) != 0 {
            parts.push(name);
        }
    }
    if parts.is_empty() {
        format!("0x{mask:04x} (none)")
    } else {
        format!("0x{mask:04x} [{}]", parts.join(" | "))
    }
}

#[derive(Debug, Clone)]
pub struct DisplaySnapshot {
    pub displays: Result<Vec<DisplayInfo>, String>,
    pub primary: Result<DisplayInfo, String>,
    pub mouse_position: Result<(f64, f64), String>,
    pub display_at_cursor: Option<Result<Option<DisplayInfo>, String>>,
    pub settings: Result<SystemSettings, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use monio::{Button, Key, ScrollDirection};

    #[test]
    fn event_filter_matches_selected_types() {
        let mut filter = EventFilter::default();
        assert!(filter.matches(EventType::MouseMoved));
        assert!(filter.matches(EventType::MouseDragged));

        filter.toggle(FilterField::MouseMoved);
        assert!(!filter.matches(EventType::MouseMoved));
        assert!(filter.matches(EventType::MouseDragged));
        assert!(filter.matches(EventType::HookEnabled));
    }

    #[test]
    fn bounded_log_caps_at_limit_newest_first() {
        let mut log = VecDeque::new();
        for seq in 1..=5 {
            let entry = EventLogEntry::from_event(seq, &Event::key_pressed(Key::KeyA, 0), false);
            push_bounded_log(&mut log, entry, 3);
        }
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].seq, 5);
        assert_eq!(log[2].seq, 3);
    }

    #[test]
    fn log_entry_captures_relative_mouse_and_wheel_fields() {
        let drag = Event::mouse_dragged_relative(120.5, 240.25, 4.5, -2.0);
        let drag_entry = EventLogEntry::from_event(1, &drag, false);
        assert_eq!(drag_entry.event_name, "MouseDragged");
        assert_eq!(drag_entry.coords, "(120.5, 240.2)");
        assert_eq!(drag_entry.extra, "rel=(+4.5, -2.0)");
        assert_eq!(drag_entry.origin_label, "Unknown");

        let wheel = Event::mouse_wheel(10.0, 20.0, ScrollDirection::Down, 3.0);
        let wheel_entry = EventLogEntry::from_event(2, &wheel, true);
        assert_eq!(wheel_entry.detail, "Down");
        assert_eq!(wheel_entry.extra, "delta=3.00");
        assert!(wheel_entry.blocked);

        let click = Event::mouse_pressed(Button::Right, 5.0, 6.0);
        let click_entry = EventLogEntry::from_event(3, &click, false);
        assert_eq!(click_entry.detail, "Right");
    }

    #[test]
    fn mask_formatting_decodes_buttons_and_modifiers() {
        assert_eq!(format_mask_flags(0), "0x0000 (none)");
        let formatted = format_mask_flags(MASK_BUTTON1 | MASK_SHIFT | MASK_META);
        assert!(formatted.contains("LMB"));
        assert!(formatted.contains("SHIFT"));
        assert!(formatted.contains("META"));
    }

    #[test]
    fn permission_and_not_supported_errors_classify_cleanly() {
        let perm = StatusBanner::from_monio_error(
            "Start Listen",
            &Error::PermissionDenied("accessibility required".into()),
        );
        assert_eq!(perm.level, BannerLevel::Error);
        assert!(perm.title.contains("Permission Denied"));

        let unsupported = StatusBanner::from_monio_error(
            "Type Text",
            &Error::NotSupported("unsupported on this target".into()),
        );
        assert_eq!(unsupported.level, BannerLevel::Warning);
        assert!(unsupported.title.contains("Not Supported"));
    }
}
