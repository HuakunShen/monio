use std::sync::{Arc, Mutex};

use gpui::{
    Bounds, Context, Focusable as _, IntoElement, MouseButton as GpuiMouseButton, Pixels,
    ScrollDelta, Window, canvas, div, prelude::*, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::tag::Tag;
use gpui_component::{ActiveTheme as _, Sizable as _};
use monio::{Button as MouseButton, Key, MediaKey};

use crate::app::MonioAppView;
use crate::hook_runner::SimulateCommand;
use crate::platform_info::select_all_shortcut_label;
use crate::ui::theme::{card, section_title};

pub const SAMPLE_TEXT: &str = "Hello from Monio! 你好";

fn bounds_tracker(slot: Arc<Mutex<Option<Bounds<Pixels>>>>) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(bounds);
            }
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
}

pub fn render_simulate_tab(
    view: &MonioAppView,
    window: &Window,
    cx: &Context<MonioAppView>,
) -> impl IntoElement {
    div()
        .id("simulate-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&view.simulate_scroll_handle)
        .gap_4()
        .child(render_text_target_card(view, window, cx))
        .child(render_keyboard_card(cx))
        .child(render_mouse_card(view, cx))
        .child(render_scroll_and_media_card(view, cx))
}

fn render_text_target_card(
    view: &MonioAppView,
    window: &Window,
    cx: &Context<MonioAppView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let focused = view
        .sandbox_input
        .read(cx)
        .focus_handle(cx)
        .is_focused(window);

    card(cx)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(section_title("Text Input & Interactive Target Area", cx))
                .child(if focused {
                    Tag::success().small().child("Target Focused")
                } else {
                    Tag::secondary().small().child("Click Target to Focus")
                }),
        )
        .child(Input::new(&view.sandbox_input).cleanable(true))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    Button::new("btn-type-text")
                        .primary()
                        .small()
                        .label(format!("type_text(\"{SAMPLE_TEXT}\")"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.focus_sandbox_input(window, cx);
                            this.run_simulate(
                                SimulateCommand::TypeText(SAMPLE_TEXT.to_string()),
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("btn-clear-sandbox")
                        .small()
                        .label("Clear Target Box")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.clear_sandbox_input(window, cx);
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Platforms without Unicode text injection surface Error::NotSupported in the status bar."),
                ),
        )
}

fn render_keyboard_card(cx: &Context<MonioAppView>) -> impl IntoElement {
    let select_label = select_all_shortcut_label();

    card(cx)
        .child(section_title("Keyboard Simulation", cx))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("btn-tap-key-a")
                        .small()
                        .label("Tap Key::KeyA")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.focus_sandbox_input(window, cx);
                            this.run_simulate(SimulateCommand::TapKey(Key::KeyA), cx);
                        })),
                )
                .child(
                    Button::new("btn-tap-space")
                        .small()
                        .label("Tap Key::Space")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.focus_sandbox_input(window, cx);
                            this.run_simulate(SimulateCommand::TapKey(Key::Space), cx);
                        })),
                )
                .child(
                    Button::new("btn-select-all")
                        .primary()
                        .small()
                        .label(format!("Select All ({select_label})"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.focus_sandbox_input(window, cx);
                            this.run_simulate(SimulateCommand::SelectAll, cx);
                        })),
                )
                .child(
                    Button::new("btn-select-all-delete")
                        .danger()
                        .small()
                        .label(format!("Select All & Delete ({select_label} + Backspace)"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.focus_sandbox_input(window, cx);
                            this.run_simulate(SimulateCommand::SelectAllAndDelete, cx);
                        })),
                ),
        )
}

fn render_mouse_card(view: &MonioAppView, cx: &Context<MonioAppView>) -> impl IntoElement {
    let theme = cx.theme();
    let pad_origin = view
        .mouse_pad_bounds
        .lock()
        .ok()
        .and_then(|g| *g)
        .map(|b| b.origin);

    let last_click_text = match (view.sim_last_click_button, view.sim_last_click_pos) {
        (Some(btn), Some((x, y))) => format!("{btn} @ ({x:.0}, {y:.0})"),
        _ => String::from("Click or run a simulation"),
    };

    card(cx)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(section_title("Mouse Move & Click Simulation", cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Tag::info().small().child(format!("Left: {}", view.sim_left_clicks)))
                        .child(Tag::warning().small().child(format!("Right: {}", view.sim_right_clicks)))
                        .child(Tag::secondary().small().child(last_click_text)),
                ),
        )
        .child(
            div()
                .id("mouse-target-pad")
                .relative()
                .h(px(124.0))
                .rounded_md()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .overflow_hidden()
                .child(bounds_tracker(Arc::clone(&view.mouse_pad_bounds)))
                .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                    this.sim_cursor_pos = Some((f32::from(event.position.x), f32::from(event.position.y)));
                    cx.notify();
                }))
                .on_mouse_down(
                    GpuiMouseButton::Left,
                    cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                        this.sim_left_clicks += 1;
                        this.sim_last_click_button = Some("Left");
                        this.sim_last_click_pos =
                            Some((f32::from(event.position.x), f32::from(event.position.y)));
                        this.sim_context_menu_pos = None;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    GpuiMouseButton::Right,
                    cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                        let pos = (f32::from(event.position.x), f32::from(event.position.y));
                        this.sim_right_clicks += 1;
                        this.sim_last_click_button = Some("Right");
                        this.sim_last_click_pos = Some(pos);
                        this.sim_context_menu_pos = Some(pos);
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .size_full()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(theme.foreground)
                                .child("Interactive Mouse & Click Target Pad"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(match view.sim_cursor_pos {
                                    Some((x, y)) => format!(
                                        "Cursor at ({x:.0}, {y:.0}) — Left click sets marker, Right click opens menu"
                                    ),
                                    None => String::from(
                                        "Use Move to Pad & Click below, or click directly inside this pad",
                                    ),
                                }),
                        ),
                )
                .when_some(
                    view.sim_last_click_pos.zip(pad_origin),
                    |el, ((abs_x, abs_y), origin)| {
                        let rel_x = (abs_x - f32::from(origin.x) - 7.0).max(2.0);
                        let rel_y = (abs_y - f32::from(origin.y) - 7.0).max(2.0);
                        el.child(
                            div()
                                .absolute()
                                .left(px(rel_x))
                                .top(px(rel_y))
                                .size(px(14.0))
                                .rounded_full()
                                .bg(if view.sim_last_click_button == Some("Right") {
                                    theme.yellow
                                } else {
                                    theme.primary
                                })
                                .border_2()
                                .border_color(theme.foreground),
                        )
                    },
                )
                .when_some(
                    view.sim_context_menu_pos.zip(pad_origin),
                    |el, ((abs_x, abs_y), origin)| {
                        let rel_x = (abs_x - f32::from(origin.x) + 10.0).clamp(8.0, 420.0);
                        let rel_y = (abs_y - f32::from(origin.y) + 6.0).clamp(8.0, 62.0);
                        el.child(
                            div()
                                .absolute()
                                .left(px(rel_x))
                                .top(px(rel_y))
                                .px_2p5()
                                .py_1p5()
                                .rounded_md()
                                .bg(theme.group_box)
                                .border_1()
                                .border_color(theme.border)
                                .text_xs()
                                .text_color(theme.foreground)
                                .child("Right-Click Menu Triggered!"),
                        )
                    },
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("btn-pad-left-click")
                        .primary()
                        .small()
                        .label("Move to Pad & Left Click")
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some((x, y)) =
                                MonioAppView::screen_center_of(&this.mouse_pad_bounds, window)
                            {
                                this.run_simulate(
                                    SimulateCommand::MoveToAndClick {
                                        x,
                                        y,
                                        button: MouseButton::Left,
                                    },
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    Button::new("btn-pad-right-click")
                        .success()
                        .small()
                        .label("Move to Pad & Right Click")
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some((x, y)) =
                                MonioAppView::screen_center_of(&this.mouse_pad_bounds, window)
                            {
                                this.run_simulate(
                                    SimulateCommand::MoveToAndClick {
                                        x,
                                        y,
                                        button: MouseButton::Right,
                                    },
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    Button::new("btn-move-rel")
                        .small()
                        .label("mouse_move_relative(+40, -25)")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_simulate(
                                SimulateCommand::MoveRelative {
                                    dx: 40.0,
                                    dy: -25.0,
                                },
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("btn-move-rel-right-click")
                        .small()
                        .label("Move Rel (+40, -25) & Right Click")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_simulate(
                                SimulateCommand::MoveRelativeAndRightClick {
                                    dx: 40.0,
                                    dy: -25.0,
                                },
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("btn-move-abs")
                        .small()
                        .label("mouse_move(320, 240)")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_simulate(
                                SimulateCommand::MoveAbsolute { x: 320.0, y: 240.0 },
                                cx,
                            );
                        })),
                ),
        )
}

fn render_scroll_and_media_card(
    view: &MonioAppView,
    cx: &Context<MonioAppView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let delta_text = match view.sim_last_scroll_delta {
        Some((dx, dy)) => format!("Delta ({dx:.1}, {dy:.1})"),
        None => String::from("Delta (0, 0)"),
    };

    card(cx)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(section_title("Phased Scroll & Media Keys", cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Tag::info()
                                .small()
                                .child(format!("Scroll Events: {}", view.sim_scroll_events)),
                        )
                        .child(Tag::secondary().small().child(delta_text)),
                ),
        )
        .child(
            div()
                .id("scroll-target-box")
                .relative()
                .h(px(124.0))
                .overflow_y_scroll()
                .track_scroll(&view.sim_scroll_list_handle)
                .rounded_md()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .p_1p5()
                .gap_1()
                .child(bounds_tracker(Arc::clone(&view.scroll_box_bounds)))
                .on_scroll(cx.listener(|this, event: &gpui::ScrollEvent, _, cx| {
                    let (dx, dy) = match event.delta {
                        ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
                        ScrollDelta::Lines(p) => (p.x, p.y),
                    };
                    this.sim_scroll_events += 1;
                    this.sim_last_scroll_delta = Some((dx, dy));
                    cx.notify();
                }))
                .children((1_usize..=30).map(|idx| {
                    div()
                        .id(("scroll-row", idx))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2p5()
                        .py_1()
                        .rounded_sm()
                        .bg(if idx % 2 == 0 {
                            theme.secondary
                        } else {
                            theme.group_box
                        })
                        .text_xs()
                        .child(
                            div()
                                .text_color(theme.foreground)
                                .child(format!("Scroll Target Row #{idx:02}")),
                        )
                        .child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child("Scroll buttons move cursor here & wheel-scroll"),
                        )
                })),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("btn-scroll-down")
                        .small()
                        .label("Phased Scroll Down")
                        .on_click(cx.listener(|this, _, window, cx| {
                            let target =
                                MonioAppView::screen_center_of(&this.scroll_box_bounds, window);
                            this.run_simulate(
                                SimulateCommand::PhasedScroll {
                                    target,
                                    delta_x: 0.0,
                                    delta_y: -18.0,
                                },
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("btn-scroll-up")
                        .small()
                        .label("Phased Scroll Up")
                        .on_click(cx.listener(|this, _, window, cx| {
                            let target =
                                MonioAppView::screen_center_of(&this.scroll_box_bounds, window);
                            this.run_simulate(
                                SimulateCommand::PhasedScroll {
                                    target,
                                    delta_x: 0.0,
                                    delta_y: 18.0,
                                },
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new("btn-scroll-loop")
                        .primary()
                        .small()
                        .label("Scroll Back & Forth (10x)")
                        .on_click(cx.listener(|this, _, window, cx| {
                            let target =
                                MonioAppView::screen_center_of(&this.scroll_box_bounds, window);
                            this.run_simulate(SimulateCommand::ScrollBackAndForth { target }, cx);
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .pt_2()
                .border_t_1()
                .border_color(theme.border)
                .child(
                    Button::new("btn-media-vol-up")
                        .small()
                        .label("VolumeUp")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_simulate(SimulateCommand::Media(MediaKey::VolumeUp), cx);
                        })),
                )
                .child(
                    Button::new("btn-media-vol-down")
                        .small()
                        .label("VolumeDown")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_simulate(SimulateCommand::Media(MediaKey::VolumeDown), cx);
                        })),
                )
                .child(
                    Button::new("btn-media-mute")
                        .small()
                        .label("Mute")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_simulate(SimulateCommand::Media(MediaKey::Mute), cx);
                        })),
                )
                .child(
                    Button::new("btn-media-play-pause")
                        .small()
                        .label("PlayPause")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run_simulate(SimulateCommand::Media(MediaKey::PlayPause), cx);
                        })),
                ),
        )
}
