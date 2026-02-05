//! TUI key displayer example - like keycastr but for the terminal.
//!
//! Run with: cargo run --example tui_key_displayer
//!
//! Note: On macOS, you need to grant Accessibility permissions to the terminal.
//! Press 'q' or Ctrl+C to exit.

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use monio::{Button, Event, EventType, Key as HookKey, ScrollDirection, listen};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use std::{
    collections::VecDeque,
    io,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

/// Maximum number of events to keep in history
const MAX_HISTORY: usize = 50;
/// Maximum number of recent keys to display
const MAX_RECENT_KEYS: usize = 10;
/// How long to show a key press highlight (milliseconds)
const KEY_HIGHLIGHT_DURATION: Duration = Duration::from_millis(300);

/// Application state
struct App {
    /// Recent input events (newest first)
    event_history: VecDeque<InputEvent>,
    /// Recent keys pressed (for the key display bar)
    recent_keys: VecDeque<KeyEntry>,
    /// Current mouse position
    mouse_position: (f64, f64),
    /// Mouse button states
    mouse_buttons: [bool; 5], // Left, Right, Middle, Button4, Button5
    /// Scroll wheel info (for animation)
    last_scroll: Option<(ScrollDirection, Instant)>,
    /// Whether the hook is active
    hook_active: bool,
    /// Exit flag
    should_exit: bool,
    /// Start time for uptime calculation
    start_time: Instant,
}

/// A single input event entry
#[derive(Clone)]
struct InputEvent {
    timestamp: Instant,
    event_type: String,
    details: String,
}

/// A key entry for the recent keys display
#[derive(Clone)]
struct KeyEntry {
    key: String,
    timestamp: Instant,
    is_pressed: bool,
}

impl App {
    fn new() -> Self {
        Self {
            event_history: VecDeque::with_capacity(MAX_HISTORY),
            recent_keys: VecDeque::with_capacity(MAX_RECENT_KEYS),
            mouse_position: (0.0, 0.0),
            mouse_buttons: [false; 5],
            last_scroll: None,
            hook_active: false,
            should_exit: false,
            start_time: Instant::now(),
        }
    }

    fn add_event(&mut self, event_type: &str, details: String) {
        let entry = InputEvent {
            timestamp: Instant::now(),
            event_type: event_type.to_string(),
            details,
        };
        if self.event_history.len() >= MAX_HISTORY {
            self.event_history.pop_back();
        }
        self.event_history.push_front(entry);
    }

    fn add_key(&mut self, key: &str, is_pressed: bool) {
        let entry = KeyEntry {
            key: key.to_string(),
            timestamp: Instant::now(),
            is_pressed,
        };
        if self.recent_keys.len() >= MAX_RECENT_KEYS {
            self.recent_keys.pop_back();
        }
        self.recent_keys.push_front(entry);
    }

    fn format_key(key: &HookKey) -> String {
        match key {
            HookKey::KeyA => "A".to_string(),
            HookKey::KeyB => "B".to_string(),
            HookKey::KeyC => "C".to_string(),
            HookKey::KeyD => "D".to_string(),
            HookKey::KeyE => "E".to_string(),
            HookKey::KeyF => "F".to_string(),
            HookKey::KeyG => "G".to_string(),
            HookKey::KeyH => "H".to_string(),
            HookKey::KeyI => "I".to_string(),
            HookKey::KeyJ => "J".to_string(),
            HookKey::KeyK => "K".to_string(),
            HookKey::KeyL => "L".to_string(),
            HookKey::KeyM => "M".to_string(),
            HookKey::KeyN => "N".to_string(),
            HookKey::KeyO => "O".to_string(),
            HookKey::KeyP => "P".to_string(),
            HookKey::KeyQ => "Q".to_string(),
            HookKey::KeyR => "R".to_string(),
            HookKey::KeyS => "S".to_string(),
            HookKey::KeyT => "T".to_string(),
            HookKey::KeyU => "U".to_string(),
            HookKey::KeyV => "V".to_string(),
            HookKey::KeyW => "W".to_string(),
            HookKey::KeyX => "X".to_string(),
            HookKey::KeyY => "Y".to_string(),
            HookKey::KeyZ => "Z".to_string(),
            HookKey::Num0 => "0".to_string(),
            HookKey::Num1 => "1".to_string(),
            HookKey::Num2 => "2".to_string(),
            HookKey::Num3 => "3".to_string(),
            HookKey::Num4 => "4".to_string(),
            HookKey::Num5 => "5".to_string(),
            HookKey::Num6 => "6".to_string(),
            HookKey::Num7 => "7".to_string(),
            HookKey::Num8 => "8".to_string(),
            HookKey::Num9 => "9".to_string(),
            HookKey::F1 => "F1".to_string(),
            HookKey::F2 => "F2".to_string(),
            HookKey::F3 => "F3".to_string(),
            HookKey::F4 => "F4".to_string(),
            HookKey::F5 => "F5".to_string(),
            HookKey::F6 => "F6".to_string(),
            HookKey::F7 => "F7".to_string(),
            HookKey::F8 => "F8".to_string(),
            HookKey::F9 => "F9".to_string(),
            HookKey::F10 => "F10".to_string(),
            HookKey::F11 => "F11".to_string(),
            HookKey::F12 => "F12".to_string(),
            HookKey::ShiftLeft => "Shift".to_string(),
            HookKey::ShiftRight => "Shift".to_string(),
            HookKey::ControlLeft => "Ctrl".to_string(),
            HookKey::ControlRight => "Ctrl".to_string(),
            HookKey::AltLeft => "Alt".to_string(),
            HookKey::AltRight => "Alt".to_string(),
            HookKey::MetaLeft => "Cmd".to_string(),
            HookKey::MetaRight => "Cmd".to_string(),
            HookKey::Escape => "Esc".to_string(),
            HookKey::Tab => "Tab".to_string(),
            HookKey::Space => "Space".to_string(),
            HookKey::Enter => "Enter".to_string(),
            HookKey::Backspace => "Backspace".to_string(),
            HookKey::Delete => "Delete".to_string(),
            HookKey::Home => "Home".to_string(),
            HookKey::End => "End".to_string(),
            HookKey::PageUp => "PgUp".to_string(),
            HookKey::PageDown => "PgDn".to_string(),
            HookKey::ArrowUp => "Up".to_string(),
            HookKey::ArrowDown => "Down".to_string(),
            HookKey::ArrowLeft => "Left".to_string(),
            HookKey::ArrowRight => "Right".to_string(),
            HookKey::Grave => "`".to_string(),
            HookKey::Minus => "-".to_string(),
            HookKey::Equal => "=".to_string(),
            HookKey::BracketLeft => "[".to_string(),
            HookKey::BracketRight => "]".to_string(),
            HookKey::Backslash => "\\".to_string(),
            HookKey::Semicolon => ";".to_string(),
            HookKey::Quote => "'".to_string(),
            HookKey::Comma => ",".to_string(),
            HookKey::Period => ".".to_string(),
            HookKey::Slash => "/".to_string(),
            HookKey::CapsLock => "CapsLock".to_string(),
            HookKey::Insert => "Insert".to_string(),
            HookKey::NumLock => "NumLock".to_string(),
            HookKey::ScrollLock => "ScrollLock".to_string(),
            HookKey::PrintScreen => "PrtScn".to_string(),
            HookKey::Pause => "Pause".to_string(),
            HookKey::Numpad0 => "Numpad0".to_string(),
            HookKey::Numpad1 => "Numpad1".to_string(),
            HookKey::Numpad2 => "Numpad2".to_string(),
            HookKey::Numpad3 => "Numpad3".to_string(),
            HookKey::Numpad4 => "Numpad4".to_string(),
            HookKey::Numpad5 => "Numpad5".to_string(),
            HookKey::Numpad6 => "Numpad6".to_string(),
            HookKey::Numpad7 => "Numpad7".to_string(),
            HookKey::Numpad8 => "Numpad8".to_string(),
            HookKey::Numpad9 => "Numpad9".to_string(),
            HookKey::NumpadAdd => "Numpad+".to_string(),
            HookKey::NumpadSubtract => "Numpad-".to_string(),
            HookKey::NumpadMultiply => "Numpad*".to_string(),
            HookKey::NumpadDivide => "Numpad/".to_string(),
            HookKey::NumpadDecimal => "Numpad.".to_string(),
            HookKey::NumpadEnter => "NumpadEnter".to_string(),
            HookKey::NumpadEqual => "Numpad=".to_string(),
            HookKey::VolumeUp => "VolUp".to_string(),
            HookKey::VolumeDown => "VolDown".to_string(),
            HookKey::VolumeMute => "Mute".to_string(),
            HookKey::MediaPlayPause => "Play/Pause".to_string(),
            HookKey::MediaStop => "Stop".to_string(),
            HookKey::MediaNext => "Next".to_string(),
            HookKey::MediaPrevious => "Prev".to_string(),
            HookKey::BrowserBack => "BrowserBack".to_string(),
            HookKey::BrowserForward => "BrowserForward".to_string(),
            HookKey::BrowserRefresh => "BrowserRefresh".to_string(),
            HookKey::BrowserStop => "BrowserStop".to_string(),
            HookKey::BrowserSearch => "BrowserSearch".to_string(),
            HookKey::BrowserFavorites => "BrowserFav".to_string(),
            HookKey::BrowserHome => "BrowserHome".to_string(),
            HookKey::LaunchMail => "LaunchMail".to_string(),
            HookKey::LaunchApp1 => "LaunchApp1".to_string(),
            HookKey::LaunchApp2 => "LaunchApp2".to_string(),
            HookKey::ContextMenu => "Menu".to_string(),
            HookKey::Unknown(code) => format!("Unknown({})", code),
            _ => format!("{:?}", key),
        }
    }

    fn format_button(button: &Button) -> String {
        match button {
            Button::Left => "Left".to_string(),
            Button::Right => "Right".to_string(),
            Button::Middle => "Middle".to_string(),
            Button::Button4 => "Back".to_string(),
            Button::Button5 => "Forward".to_string(),
            Button::Unknown(n) => format!("Btn{}", n),
        }
    }

    fn button_index(button: &Button) -> usize {
        match button {
            Button::Left => 0,
            Button::Right => 1,
            Button::Middle => 2,
            Button::Button4 => 3,
            Button::Button5 => 4,
            Button::Unknown(n) => (*n as usize).saturating_sub(1).min(4),
        }
    }

    fn handle_monio_event(&mut self, event: &Event) {
        match event.event_type {
            EventType::HookEnabled => {
                self.hook_active = true;
                self.add_event("Hook", "Hook enabled".to_string());
            }
            EventType::HookDisabled => {
                self.hook_active = false;
                self.add_event("Hook", "Hook disabled".to_string());
            }
            EventType::KeyPressed => {
                if let Some(kb) = &event.keyboard {
                    let key_str = Self::format_key(&kb.key);
                    self.add_key(&key_str, true);
                    self.add_event("KeyPress", format!("{} (raw: {})", key_str, kb.raw_code));
                }
            }
            EventType::KeyReleased => {
                if let Some(kb) = &event.keyboard {
                    let key_str = Self::format_key(&kb.key);
                    self.add_key(&key_str, false);
                    self.add_event("KeyRelease", format!("{}", key_str));
                }
            }
            EventType::MousePressed => {
                if let Some(mouse) = &event.mouse {
                    if let Some(button) = mouse.button {
                        let btn_idx = Self::button_index(&button);
                        self.mouse_buttons[btn_idx] = true;
                        let btn_str = Self::format_button(&button);
                        self.add_event(
                            "MousePress",
                            format!("{} at ({:.0}, {:.0})", btn_str, mouse.x, mouse.y),
                        );
                    }
                }
            }
            EventType::MouseReleased => {
                if let Some(mouse) = &event.mouse {
                    if let Some(button) = mouse.button {
                        let btn_idx = Self::button_index(&button);
                        self.mouse_buttons[btn_idx] = false;
                        let btn_str = Self::format_button(&button);
                        self.add_event(
                            "MouseRelease",
                            format!("{} at ({:.0}, {:.0})", btn_str, mouse.x, mouse.y),
                        );
                    }
                }
            }
            EventType::MouseClicked => {
                if let Some(mouse) = &event.mouse {
                    if let Some(button) = mouse.button {
                        let btn_str = Self::format_button(&button);
                        self.add_event(
                            "MouseClick",
                            format!(
                                "{} clicks={} at ({:.0}, {:.0})",
                                btn_str, mouse.clicks, mouse.x, mouse.y
                            ),
                        );
                    }
                }
            }
            EventType::MouseMoved => {
                if let Some(mouse) = &event.mouse {
                    self.mouse_position = (mouse.x, mouse.y);
                    // Don't log every move to avoid flooding
                }
            }
            EventType::MouseDragged => {
                if let Some(mouse) = &event.mouse {
                    self.mouse_position = (mouse.x, mouse.y);
                    // Don't log every drag to avoid flooding
                }
            }
            EventType::MouseWheel => {
                if let Some(wheel) = &event.wheel {
                    self.mouse_position = (wheel.x, wheel.y);
                    let dir_str = match wheel.direction {
                        ScrollDirection::Up => "Up",
                        ScrollDirection::Down => "Down",
                        ScrollDirection::Left => "Left",
                        ScrollDirection::Right => "Right",
                    };
                    self.last_scroll = Some((wheel.direction, Instant::now()));
                    self.add_event(
                        "Scroll",
                        format!(
                            "{} delta={:.1} at ({:.0}, {:.0})",
                            dir_str, wheel.delta, wheel.x, wheel.y
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    fn uptime(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let secs = elapsed.as_secs();
        let mins = secs / 60;
        let hours = mins / 60;
        format!("{:02}:{:02}:{:02}", hours, mins % 60, secs % 60)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create channel for monio events
    let (tx, rx): (Sender<Event>, Receiver<Event>) = mpsc::channel();

    // Start monio in a separate thread
    std::thread::spawn(move || {
        if let Err(e) = listen(move |event: &Event| {
            let _ = tx.send(event.clone());
        }) {
            eprintln!("monio error: {}", e);
        }
    });

    // Create app state
    let mut app = App::new();
    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();

    // Main loop
    loop {
        // Draw UI
        terminal.draw(|f| draw(f, &mut app))?;

        // Handle timeout
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());

        // Check for crossterm events (for exit)
        if crossterm::event::poll(timeout)? {
            if let CEvent::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        app.should_exit = true;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        app.should_exit = true;
                    }
                    _ => {}
                }
            }
        }

        // Process monio events
        while let Ok(event) = rx.try_recv() {
            app.handle_monio_event(&event);
        }

        // Tick for animations
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }

        if app.should_exit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Length(5), // Recent keys display
            Constraint::Length(6), // Mouse status
            Constraint::Min(10),   // Event history
            Constraint::Length(1), // Help text
        ])
        .split(f.area());

    draw_title_bar(f, app, chunks[0]);
    draw_recent_keys(f, app, chunks[1]);
    draw_mouse_status(f, app, chunks[2]);
    draw_event_history(f, app, chunks[3]);
    draw_help(f, chunks[4]);
}

fn draw_title_bar(f: &mut Frame, app: &App, area: Rect) {
    let status_color = if app.hook_active {
        Color::Green
    } else {
        Color::Red
    };
    let status_text = if app.hook_active {
        "ACTIVE"
    } else {
        "INACTIVE"
    };

    let title = Line::from(vec![
        Span::styled(
            " monio TUI Key Displayer ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Hook: "),
        Span::styled(
            status_text,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | Uptime: "),
        Span::styled(app.uptime(), Style::default().fg(Color::Yellow)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));

    let paragraph = Paragraph::new(title)
        .block(block)
        .alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

fn draw_recent_keys(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Recent Keys ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.recent_keys.is_empty() {
        let empty = Paragraph::new("Press some keys...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(empty, inner);
        return;
    }

    // Build spans for recent keys
    let mut spans = vec![];
    let now = Instant::now();

    for (i, entry) in app.recent_keys.iter().enumerate() {
        let age = now.duration_since(entry.timestamp);
        let is_highlighted = entry.is_pressed && age < KEY_HIGHLIGHT_DURATION;

        let bg_color = if is_highlighted {
            Color::Yellow
        } else {
            Color::DarkGray
        };
        let fg_color = if is_highlighted {
            Color::Black
        } else {
            Color::White
        };

        spans.push(Span::styled(
            format!(" {} ", entry.key),
            Style::default()
                .bg(bg_color)
                .fg(fg_color)
                .add_modifier(Modifier::BOLD),
        ));

        if i < app.recent_keys.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).alignment(Alignment::Center);
    f.render_widget(paragraph, inner);
}

fn draw_mouse_status(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Mouse Status ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split into sections
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    // Left: Position
    let pos_text = format!(
        "Position: ({:.0}, {:.0})",
        app.mouse_position.0, app.mouse_position.1
    );
    let pos_para = Paragraph::new(pos_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);
    f.render_widget(pos_para, sections[0]);

    // Right: Button states
    let button_names = ["Left", "Right", "Middle", "Back", "Forward"];
    let mut button_lines = vec![];

    for (i, name) in button_names.iter().enumerate() {
        let is_pressed = app.mouse_buttons[i];
        let color = if is_pressed {
            Color::Green
        } else {
            Color::DarkGray
        };
        let symbol = if is_pressed { "●" } else { "○" };
        button_lines.push(Line::from(vec![
            Span::styled(symbol, Style::default().fg(color)),
            Span::raw(format!(" {}", name)),
        ]));
    }

    // Add scroll indicator
    let scroll_text = if let Some((dir, time)) = &app.last_scroll {
        let age = time.elapsed();
        if age < Duration::from_millis(500) {
            let arrow = match dir {
                ScrollDirection::Up => "↑",
                ScrollDirection::Down => "↓",
                ScrollDirection::Left => "←",
                ScrollDirection::Right => "→",
            };
            format!(" Scroll: {} ", arrow)
        } else {
            " Scroll: - ".to_string()
        }
    } else {
        " Scroll: - ".to_string()
    };
    button_lines.push(Line::from(""));
    button_lines.push(Line::from(Span::styled(
        scroll_text,
        Style::default().fg(Color::Yellow),
    )));

    let buttons_para = Paragraph::new(button_lines).alignment(Alignment::Center);
    f.render_widget(buttons_para, sections[1]);
}

fn draw_event_history(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Event History ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.event_history.is_empty() {
        let empty = Paragraph::new("No events yet...")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(empty, inner);
        return;
    }

    // Create table rows
    let rows: Vec<Row> = app
        .event_history
        .iter()
        .take(inner.height as usize)
        .map(|event| {
            let elapsed = event.timestamp.elapsed();
            let time_str = format!("{:02}.{:03}s", elapsed.as_secs(), elapsed.subsec_millis());

            let type_color = match event.event_type.as_str() {
                "KeyPress" => Color::Yellow,
                "KeyRelease" => Color::Rgb(180, 160, 0),
                "MousePress" => Color::Cyan,
                "MouseRelease" => Color::Rgb(0, 160, 160),
                "MouseClick" => Color::Blue,
                "Scroll" => Color::Magenta,
                "Hook" => Color::Green,
                _ => Color::White,
            };

            Row::new(vec![
                Cell::from(time_str).style(Style::default().fg(Color::DarkGray)),
                Cell::from(event.event_type.clone()).style(Style::default().fg(type_color)),
                Cell::from(event.details.clone()).style(Style::default().fg(Color::White)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(vec!["Time", "Type", "Details"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    );

    f.render_widget(table, inner);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let help = Paragraph::new("Press 'q' or Ctrl+C to exit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(help, area);
}
