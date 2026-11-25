use gpui::{App, Context, IntoElement, div, prelude::*, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::tag::Tag;
use gpui_component::{ActiveTheme as _, Disableable as _, Sizable as _};

use crate::app::MonioAppView;
use crate::model::{FilterField, HookMode, MAX_LOG_ENTRIES, format_mask_flags};
use crate::ui::theme::{card, section_title, stat_tile};

pub fn render_monitor_tab(view: &MonioAppView, cx: &Context<MonioAppView>) -> impl IntoElement {
    let mode = view.hook_mode;
    let is_running = mode.is_active();
    let stats = &view.stats;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap_4()
        .child(render_controls_card(view, mode, is_running, cx))
        .child(render_counters_row(
            stats,
            view.blocked_count,
            view.active_mask,
            cx,
        ))
        .child(render_log_card(view, cx))
}

fn render_controls_card(
    view: &MonioAppView,
    mode: HookMode,
    is_running: bool,
    cx: &Context<MonioAppView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let block_escape = view.is_escape_blocked();

    card(cx)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(section_title("Hook Controls", cx))
                        .child(match mode {
                            HookMode::Idle => Tag::secondary().small().child("Idle"),
                            HookMode::Listening => Tag::success().small().child("Listening"),
                            HookMode::Grabbing => Tag::warning().small().child("Grabbing"),
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("btn-start-listen")
                                .primary()
                                .small()
                                .label("Start Listen")
                                .disabled(is_running)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.start_listen(cx);
                                })),
                        )
                        .child(
                            Button::new("btn-start-grab")
                                .success()
                                .small()
                                .label("Start Grab")
                                .disabled(is_running)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.start_grab(cx);
                                })),
                        )
                        .child(
                            Button::new("btn-stop-hook")
                                .danger()
                                .small()
                                .label("Stop Hook")
                                .disabled(!is_running)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.stop_hook(cx);
                                })),
                        ),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_3()
                .pt_1()
                .child(
                    Checkbox::new("chk-block-escape")
                        .checked(block_escape)
                        .label("Block Key::Escape during Grab (passes all other events through)")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.toggle_block_escape(cx);
                        })),
                )
                .child(div().text_xs().text_color(theme.muted_foreground).child(
                    "HarmonyOS supports keyboard-only grab; pointer events remain observe-only.",
                )),
        )
        .child(render_filters_row(view, cx))
}

fn render_filters_row(view: &MonioAppView, cx: &Context<MonioAppView>) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_2p5()
        .pt_2p5()
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.muted_foreground)
                        .child("Event Type Filters"),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("btn-filter-all")
                                .xsmall()
                                .label("All")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.event_filter.set_all(true);
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("btn-filter-no-moves")
                                .xsmall()
                                .label("Hide MouseMoved")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.event_filter.set_all(true);
                                    this.event_filter.mouse_moved = false;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("btn-filter-none")
                                .xsmall()
                                .label("None")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.event_filter.set_all(false);
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .child(
            div().flex().flex_wrap().gap_4().children(
                FilterField::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let checked = view.event_filter.get(field);
                        Checkbox::new(("filter-chk", idx))
                            .checked(checked)
                            .label(field.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.event_filter.toggle(field);
                                cx.notify();
                            }))
                    }),
            ),
        )
}

fn render_counters_row(
    stats: &monio::EventStatistics,
    blocked_count: u64,
    active_mask: u32,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .flex_wrap()
        .gap_3()
        .child(stat_tile(
            "Total Events",
            stats.total_event_count.to_string(),
            theme.foreground,
            cx,
        ))
        .child(stat_tile(
            "MouseMoved",
            stats.mouse_move_count.to_string(),
            theme.blue,
            cx,
        ))
        .child(stat_tile(
            "MouseDragged",
            stats.mouse_drag_count.to_string(),
            theme.green,
            cx,
        ))
        .child(stat_tile(
            "KeyPressed",
            format!("{} (blocked: {blocked_count})", stats.key_press_count),
            theme.yellow,
            cx,
        ))
        .child(stat_tile(
            "Active Mask",
            format_mask_flags(active_mask),
            theme.muted_foreground,
            cx,
        ))
}

fn render_log_card(view: &MonioAppView, cx: &Context<MonioAppView>) -> impl IntoElement {
    let theme = cx.theme();
    let total_rows = view.event_log.len();

    card(cx)
        .flex_1()
        .min_h(px(220.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(section_title("Recent Events", cx))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("{total_rows} / {MAX_LOG_ENTRIES} shown (newest first)")),
                        ),
                )
                .child(
                    Button::new("btn-clear-log")
                        .xsmall()
                        .label("Clear")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_log(cx);
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(theme.secondary)
                .text_xs()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(div().w(px(56.0)).child("#"))
                .child(div().w(px(130.0)).child("EventType"))
                .child(div().flex_1().min_w(px(140.0)).child("Key / Button"))
                .child(div().w(px(130.0)).child("Coords"))
                .child(div().w(px(140.0)).child("Relative / Wheel"))
                .child(div().w(px(130.0)).child("Origin")),
        )
        .child(
            div()
                .id("event-log-scroll")
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(150.0))
                .overflow_y_scroll()
                .track_scroll(&view.log_scroll_handle)
                .rounded_md()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .p_1()
                .gap_0p5()
                .when(view.event_log.is_empty(), |el| {
                    el.child(
                        div()
                            .p_4()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("No events recorded yet. Start Listen or Start Grab above, then move the mouse or press keys."),
                    )
                })
                .children(view.event_log.iter().map(|entry| {
                    let event_tag = if entry.blocked {
                        Tag::danger()
                            .small()
                            .child(format!("{} [BLK]", entry.event_name))
                    } else {
                        match entry.event_type {
                            monio::EventType::KeyPressed
                            | monio::EventType::KeyReleased
                            | monio::EventType::KeyTyped => Tag::info().small().child(entry.event_name),
                            monio::EventType::MousePressed
                            | monio::EventType::MouseReleased
                            | monio::EventType::MouseClicked => Tag::warning().small().child(entry.event_name),
                            monio::EventType::MouseDragged => Tag::success().small().child(entry.event_name),
                            monio::EventType::MouseWheel => Tag::primary().small().child(entry.event_name),
                            monio::EventType::MouseMoved
                            | monio::EventType::HookEnabled
                            | monio::EventType::HookDisabled => Tag::secondary().small().child(entry.event_name),
                        }
                    };
                    div()
                        .id(("log-row", entry.seq as usize))
                        .flex()
                        .items_center()
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .bg(if entry.blocked {
                            theme.danger.opacity(0.15)
                        } else {
                            theme.group_box
                        })
                        .text_xs()
                        .child(
                            div()
                                .w(px(56.0))
                                .text_color(theme.muted_foreground)
                                .child(format!("#{}", entry.seq)),
                        )
                        .child(div().w(px(130.0)).flex().items_center().child(event_tag))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(140.0))
                                .text_color(theme.foreground)
                                .child(entry.detail.clone()),
                        )
                        .child(
                            div()
                                .w(px(130.0))
                                .text_color(theme.muted_foreground)
                                .child(entry.coords.clone()),
                        )
                        .child(
                            div()
                                .w(px(140.0))
                                .text_color(theme.muted_foreground)
                                .child(entry.extra.clone()),
                        )
                        .child(
                            div()
                                .w(px(130.0))
                                .flex()
                                .items_center()
                                .child(Tag::secondary().small().child(entry.origin_label)),
                        )
                })),
        )
}
