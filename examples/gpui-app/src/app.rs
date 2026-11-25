use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    Bounds, Context, Entity, FocusHandle, Focusable as _, IntoElement, KeyDownEvent, Pixels,
    Render, ScrollHandle, Window, WindowAppearance, div, prelude::*, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::InputState;
use gpui_component::tag::Tag;
use gpui_component::{ActiveTheme as _, Sizable as _, Theme, ThemeMode};
use monio::EventStatistics;

use crate::hook_runner::{
    ActiveHookSession, SimulateCommand, WorkerMessage, banner_from_action_result,
    create_worker_channel, spawn_display_refresh, spawn_probe_relative_capture, spawn_simulate,
};
use crate::model::{
    ActiveTab, BannerLevel, DisplaySnapshot, EventFilter, EventLogEntry, HookMode, MAX_LOG_ENTRIES,
    StatusBanner, push_bounded_log,
};
use crate::platform_info::current_platform_name;
use crate::ui::displays::render_displays_tab;
use crate::ui::monitor::render_monitor_tab;
use crate::ui::simulate::{SAMPLE_TEXT, render_simulate_tab};

pub struct MonioAppView {
    pub active_tab: ActiveTab,
    pub hook_mode: HookMode,
    pub event_filter: EventFilter,
    pub stats: EventStatistics,
    pub blocked_count: u64,
    pub active_mask: u32,
    pub event_log: VecDeque<EventLogEntry>,
    pub status_banner: StatusBanner,
    pub display_snapshot: Option<DisplaySnapshot>,
    pub display_refreshing: bool,
    pub sandbox_input: Entity<InputState>,
    pub zoom_scale: f32,
    pub dark_mode: bool,

    pub sim_left_clicks: u64,
    pub sim_right_clicks: u64,
    pub sim_last_click_pos: Option<(f32, f32)>,
    pub sim_last_click_button: Option<&'static str>,
    pub sim_context_menu_pos: Option<(f32, f32)>,
    pub sim_cursor_pos: Option<(f32, f32)>,
    pub sim_scroll_events: u64,
    pub sim_last_scroll_delta: Option<(f32, f32)>,
    pub mouse_pad_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    pub scroll_box_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,

    pub log_scroll_handle: ScrollHandle,
    pub simulate_scroll_handle: ScrollHandle,
    pub sim_scroll_list_handle: ScrollHandle,
    pub displays_scroll_handle: ScrollHandle,
    pub root_focus_handle: FocusHandle,

    next_seq: u64,
    block_escape: Arc<AtomicBool>,
    active_hook: Option<ActiveHookSession>,
    worker_tx: SyncSender<WorkerMessage>,
    worker_rx: Receiver<WorkerMessage>,
    poll_running: bool,
    in_flight_actions: usize,
}

impl MonioAppView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dark_mode = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        Theme::change(
            if dark_mode {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );

        let (worker_tx, worker_rx) = create_worker_channel();
        let sandbox_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Click here or press Type Text to receive simulated input...")
                .default_value(SAMPLE_TEXT)
        });
        let mut view = Self {
            active_tab: ActiveTab::Monitor,
            hook_mode: HookMode::Idle,
            event_filter: EventFilter::default(),
            stats: EventStatistics::new(),
            blocked_count: 0,
            active_mask: monio::state::get_mask(),
            event_log: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            status_banner: StatusBanner::info(
                format!("Ready on {}", current_platform_name()),
                "Start Listen or Grab to inspect live events, test Simulation, or query Displays.",
            ),
            display_snapshot: None,
            display_refreshing: false,
            sandbox_input,
            zoom_scale: 1.0,
            dark_mode,
            sim_left_clicks: 0,
            sim_right_clicks: 0,
            sim_last_click_pos: None,
            sim_last_click_button: None,
            sim_context_menu_pos: None,
            sim_cursor_pos: None,
            sim_scroll_events: 0,
            sim_last_scroll_delta: None,
            mouse_pad_bounds: Arc::new(Mutex::new(None)),
            scroll_box_bounds: Arc::new(Mutex::new(None)),
            log_scroll_handle: ScrollHandle::new(),
            simulate_scroll_handle: ScrollHandle::new(),
            sim_scroll_list_handle: ScrollHandle::new(),
            displays_scroll_handle: ScrollHandle::new(),
            root_focus_handle: cx.focus_handle(),
            next_seq: 1,
            block_escape: Arc::new(AtomicBool::new(true)),
            active_hook: None,
            worker_tx,
            worker_rx,
            poll_running: false,
            in_flight_actions: 0,
        };
        view.refresh_displays(cx);
        view
    }

    pub fn is_escape_blocked(&self) -> bool {
        self.block_escape.load(Ordering::Relaxed)
    }

    pub fn toggle_block_escape(&mut self, cx: &mut Context<Self>) {
        let prev = self.block_escape.load(Ordering::Relaxed);
        self.block_escape.store(!prev, Ordering::Relaxed);
        cx.notify();
    }

    pub fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dark_mode = !self.dark_mode;
        Theme::change(
            if self.dark_mode {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        cx.notify();
    }

    pub fn screen_center_of(
        bounds_slot: &Arc<Mutex<Option<Bounds<Pixels>>>>,
        window: &Window,
    ) -> Option<(f64, f64)> {
        let guard = bounds_slot.lock().ok()?;
        let bounds = (*guard)?;
        let win_origin = window.bounds().origin;
        let center = bounds.center();
        Some((
            f64::from(win_origin.x + center.x),
            f64::from(win_origin.y + center.y),
        ))
    }

    pub fn apply_zoom(&mut self, next_scale: f32, window: &mut Window, cx: &mut Context<Self>) {
        let clamped = (next_scale.clamp(0.75, 1.75) * 100.0).round() / 100.0;
        if (clamped - self.zoom_scale).abs() < f32::EPSILON {
            return;
        }
        self.zoom_scale = clamped;
        window.set_rem_size(px(16.0 * self.zoom_scale));
        cx.notify();
    }

    pub fn handle_root_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mods = &event.keystroke.modifiers;
        if mods.platform || mods.control {
            match event.keystroke.key.as_str() {
                "=" | "+" => {
                    self.apply_zoom(self.zoom_scale + 0.1, window, cx);
                    cx.stop_propagation();
                }
                "-" | "_" => {
                    self.apply_zoom(self.zoom_scale - 0.1, window, cx);
                    cx.stop_propagation();
                }
                "0" => {
                    self.apply_zoom(1.0, window, cx);
                    cx.stop_propagation();
                }
                _ => {}
            }
        }
    }

    pub fn start_listen(&mut self, cx: &mut Context<Self>) {
        if self.hook_mode.is_active() {
            return;
        }
        self.active_hook = Some(ActiveHookSession::start_listen(self.worker_tx.clone()));
        self.hook_mode = HookMode::Listening;
        self.status_banner = StatusBanner::info(
            "Starting Listen Hook",
            "Listening for keyboard, mouse, drag, and wheel events...",
        );
        self.ensure_poll_loop(cx);
        cx.notify();
    }

    pub fn start_grab(&mut self, cx: &mut Context<Self>) {
        if self.hook_mode.is_active() {
            return;
        }
        self.active_hook = Some(ActiveHookSession::start_grab(
            Arc::clone(&self.block_escape),
            self.worker_tx.clone(),
        ));
        self.hook_mode = HookMode::Grabbing;
        self.status_banner = StatusBanner::warning(
            "Grab Hook Active",
            if self.is_escape_blocked() {
                "Key::Escape is blocked while other events pass through."
            } else {
                "All events pass through until Block Key::Escape is enabled."
            },
        );
        self.ensure_poll_loop(cx);
        cx.notify();
    }

    pub fn stop_hook(&mut self, cx: &mut Context<Self>) {
        if let Some(mut session) = self.active_hook.take() {
            match session.stop() {
                Ok(()) => {
                    self.status_banner = StatusBanner::info(
                        "Hook Stopped",
                        "Input hook stopped cleanly and mask state was reset.",
                    );
                }
                Err(err) => {
                    self.status_banner = StatusBanner::from_monio_error("Stop Hook", &err);
                }
            }
        }
        self.hook_mode = HookMode::Idle;
        self.active_mask = monio::state::get_mask();
        self.drain_worker_messages();
        cx.notify();
    }

    pub fn clear_log(&mut self, cx: &mut Context<Self>) {
        self.event_log.clear();
        self.stats = EventStatistics::new();
        self.blocked_count = 0;
        self.next_seq = 1;
        cx.notify();
    }

    pub fn run_simulate(&mut self, cmd: SimulateCommand, cx: &mut Context<Self>) {
        self.in_flight_actions += 1;
        self.status_banner = StatusBanner::info(
            format!("Running {}", cmd.label()),
            "Dispatching simulation command off the UI thread...",
        );
        spawn_simulate(cmd, self.worker_tx.clone());
        self.ensure_poll_loop(cx);
        cx.notify();
    }

    pub fn refresh_displays(&mut self, cx: &mut Context<Self>) {
        if self.display_refreshing {
            return;
        }
        self.display_refreshing = true;
        self.in_flight_actions += 1;
        spawn_display_refresh(self.worker_tx.clone());
        self.ensure_poll_loop(cx);
        cx.notify();
    }

    pub fn probe_relative_capture(&mut self, cx: &mut Context<Self>) {
        self.in_flight_actions += 1;
        self.status_banner = StatusBanner::info(
            "Probing RelativePointerCapture",
            "Acquiring and immediately releasing relative pointer capture lease...",
        );
        spawn_probe_relative_capture(self.worker_tx.clone());
        self.ensure_poll_loop(cx);
        cx.notify();
    }

    pub fn focus_sandbox_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.sandbox_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
    }

    pub fn clear_sandbox_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.sandbox_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
    }

    fn ensure_poll_loop(&mut self, cx: &mut Context<Self>) {
        if self.poll_running {
            return;
        }
        self.poll_running = true;

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(25))
                    .await;
                let mut keep_polling = false;
                let res = this.update(cx, |view, cx| {
                    let changed = view.drain_worker_messages();
                    keep_polling = view.has_active_background_work();
                    if !keep_polling {
                        view.poll_running = false;
                    }
                    if changed || !keep_polling {
                        cx.notify();
                    }
                });
                if res.is_err() || !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn has_active_background_work(&self) -> bool {
        self.hook_mode.is_active() || self.in_flight_actions > 0
    }

    fn drain_worker_messages(&mut self) -> bool {
        let mut changed = false;
        let mut processed = 0usize;

        while processed < 128 {
            match self.worker_rx.try_recv() {
                Ok(msg) => {
                    changed = true;
                    processed += 1;
                    self.apply_worker_message(msg);
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }

        changed
    }

    fn apply_worker_message(&mut self, msg: WorkerMessage) {
        match msg {
            WorkerMessage::HookEvent { event, blocked } => {
                self.stats.record_event(&event);
                self.active_mask = event.mask;
                if blocked {
                    self.blocked_count += 1;
                }
                if self.event_filter.matches(event.event_type) {
                    let seq = self.next_seq;
                    self.next_seq += 1;
                    let entry = EventLogEntry::from_event(seq, &event, blocked);
                    push_bounded_log(&mut self.event_log, entry, MAX_LOG_ENTRIES);
                }
            }
            WorkerMessage::HookEnded { result } => {
                self.hook_mode = HookMode::Idle;
                self.active_hook = None;
                self.active_mask = monio::state::get_mask();
                if let Err(err) = result {
                    self.status_banner = StatusBanner::from_monio_error("Input Hook", &err);
                }
            }
            WorkerMessage::ActionFinished { action, result } => {
                self.in_flight_actions = self.in_flight_actions.saturating_sub(1);
                self.status_banner = banner_from_action_result(&action, &result);
            }
            WorkerMessage::DisplayRefreshed(snapshot) => {
                self.in_flight_actions = self.in_flight_actions.saturating_sub(1);
                self.display_refreshing = false;
                self.display_snapshot = Some(snapshot);
            }
        }
    }

    fn render_sidebar(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (dot_color, hook_label) = match self.hook_mode {
            HookMode::Idle => (theme.muted_foreground, "Idle"),
            HookMode::Listening => (theme.green, "Listening"),
            HookMode::Grabbing => (theme.yellow, "Grabbing"),
        };

        div()
            .flex()
            .flex_col()
            .justify_between()
            .w(px(220.0))
            .h_full()
            .p_3()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.sidebar_border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .px_2()
                            .pt_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.sidebar_foreground)
                                    .child("Monio Input Studio"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(current_platform_name()),
                            ),
                    )
                    .child(div().flex().flex_col().gap_1().children(
                        ActiveTab::ALL.into_iter().enumerate().map(|(idx, tab)| {
                            let active = self.active_tab == tab;
                            div()
                                .id(("nav-tab", idx))
                                .flex()
                                .items_center()
                                .px_2p5()
                                .py_1p5()
                                .rounded_md()
                                .text_sm()
                                .font_weight(if active {
                                    gpui::FontWeight::MEDIUM
                                } else {
                                    gpui::FontWeight::NORMAL
                                })
                                .cursor_pointer()
                                .when(active, |el| {
                                    el.bg(theme.sidebar_accent)
                                        .text_color(theme.sidebar_accent_foreground)
                                })
                                .when(!active, |el| {
                                    el.text_color(theme.muted_foreground).hover(|s| {
                                        s.bg(theme.sidebar_accent)
                                            .text_color(theme.sidebar_accent_foreground)
                                    })
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.active_tab = tab;
                                    if tab == ActiveTab::Displays && this.display_snapshot.is_none()
                                    {
                                        this.refresh_displays(cx);
                                    } else {
                                        cx.notify();
                                    }
                                }))
                                .child(tab.label())
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .p_2p5()
                    .rounded_md()
                    .bg(theme.group_box)
                    .border_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Hook Status"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(div().size(px(7.0)).rounded_full().bg(dot_color))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(theme.foreground)
                                            .child(hook_label),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Events"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.foreground)
                                    .child(self.stats.total_event_count.to_string()),
                            ),
                    ),
            )
    }

    fn render_content_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (title, subtitle) = match self.active_tab {
            ActiveTab::Monitor => (
                "Monitor & Grab",
                "Inspect live global input events and test key suppression",
            ),
            ActiveTab::Simulate => (
                "Simulate",
                "Dispatch synthetic keyboard, mouse, scroll, and media events",
            ),
            ActiveTab::Displays => (
                "Displays & Capabilities",
                "Inspect screen topology, pointer state, and OS backend support",
            ),
        };

        div()
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_base()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(subtitle),
                    ),
            )
    }

    fn render_status_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let status_tag = match self.status_banner.level {
            BannerLevel::Info => Tag::info().small().child("INFO"),
            BannerLevel::Success => Tag::success().small().child("OK"),
            BannerLevel::Warning => Tag::warning().small().child("WARN"),
            BannerLevel::Error => Tag::danger().small().child("ERROR"),
        };
        let zoom_pct = (self.zoom_scale * 100.0).round() as i32;

        div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_1p5()
            .bg(theme.sidebar)
            .border_t_1()
            .border_color(theme.sidebar_border)
            .child(status_tag)
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(self.status_banner.title.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("—"),
            )
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(self.status_banner.detail.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("btn-toggle-theme")
                            .ghost()
                            .xsmall()
                            .label(if self.dark_mode { "Dark" } else { "Light" })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_theme(window, cx);
                            })),
                    )
                    .child(div().text_xs().text_color(theme.border).child("|"))
                    .child(
                        Button::new("btn-zoom-out")
                            .ghost()
                            .xsmall()
                            .label("-")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_zoom(this.zoom_scale - 0.1, window, cx);
                            })),
                    )
                    .child(
                        Button::new("btn-zoom-reset")
                            .ghost()
                            .xsmall()
                            .label(format!("{zoom_pct}%"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_zoom(1.0, window, cx);
                            })),
                    )
                    .child(
                        Button::new("btn-zoom-in")
                            .ghost()
                            .xsmall()
                            .label("+")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_zoom(this.zoom_scale + 0.1, window, cx);
                            })),
                    ),
            )
    }
}

impl Render for MonioAppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if window.focused(cx).is_none() {
            self.root_focus_handle.focus(window, cx);
        }

        let theme = cx.theme();
        div()
            .id("monio-root")
            .track_focus(&self.root_focus_handle)
            .on_key_down(cx.listener(Self::handle_root_key_down))
            .flex()
            .flex_row()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.render_content_header(cx))
                    .child(div().flex().flex_col().flex_1().min_h_0().p_4().child(
                        match self.active_tab {
                            ActiveTab::Monitor => render_monitor_tab(self, cx).into_any_element(),
                            ActiveTab::Simulate => {
                                render_simulate_tab(self, window, cx).into_any_element()
                            }
                            ActiveTab::Displays => render_displays_tab(self, cx).into_any_element(),
                        },
                    ))
                    .child(self.render_status_bar(cx)),
            )
    }
}
