use gpui::{App, Context, IntoElement, div, prelude::*, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::tag::Tag;
use gpui_component::{ActiveTheme as _, Disableable as _, Sizable as _};

use crate::app::MonioAppView;
use crate::model::DisplaySnapshot;
use crate::platform_info::{capability_matrix, current_platform_name};
use crate::ui::theme::{card, section_title};

pub fn render_displays_tab(view: &MonioAppView, cx: &Context<MonioAppView>) -> impl IntoElement {
    div()
        .id("displays-scroll")
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&view.displays_scroll_handle)
        .gap_4()
        .child(render_summary_header(view, cx))
        .child(render_displays_table(view.display_snapshot.as_ref(), cx))
        .child(render_pointer_and_settings(
            view.display_snapshot.as_ref(),
            cx,
        ))
        .child(render_capability_matrix(cx))
}

fn render_summary_header(view: &MonioAppView, cx: &Context<MonioAppView>) -> impl IntoElement {
    let refreshing = view.display_refreshing;
    card(cx).child(
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
                    .child(section_title("Platform & Display Diagnostics", cx))
                    .child(Tag::info().small().child(current_platform_name())),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("btn-probe-rel-capture")
                            .small()
                            .label("Probe RelativePointerCapture")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.probe_relative_capture(cx);
                            })),
                    )
                    .child(
                        Button::new("btn-refresh-displays")
                            .primary()
                            .small()
                            .label(if refreshing {
                                "Refreshing..."
                            } else {
                                "Refresh Snapshot"
                            })
                            .disabled(refreshing)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_displays(cx);
                            })),
                    ),
            ),
    )
}

fn render_displays_table(snapshot: Option<&DisplaySnapshot>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let Some(snapshot) = snapshot else {
        return card(cx)
            .child(section_title("Connected Displays", cx))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Loading display topology..."),
            );
    };

    let primary_label = match &snapshot.primary {
        Ok(primary) => format!(
            "Primary ID {} ({:.0}x{:.0} @ {:.2}x)",
            primary.id, primary.bounds.width, primary.bounds.height, primary.scale_factor
        ),
        Err(err) => format!("Primary query failed: {err}"),
    };

    let mut container = card(cx).child(
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .child(section_title("Connected Displays", cx))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(primary_label),
            ),
    );

    match &snapshot.displays {
        Ok(list) if list.is_empty() => {
            container = container.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("No displays returned."),
            );
        }
        Ok(list) => {
            let max_w = list
                .iter()
                .map(|d| d.bounds.width.max(1.0))
                .fold(1.0_f64, f64::max);
            container = container.child(div().flex().flex_wrap().gap_3().children(
                list.iter().enumerate().map(|(idx, d)| {
                    let preview_w = ((d.bounds.width / max_w) * 180.0).clamp(96.0, 180.0);
                    let preview_h = ((preview_w * d.bounds.height) / d.bounds.width.max(1.0))
                        .clamp(58.0, 112.0);
                    div()
                        .id(("display-card", idx))
                        .flex()
                        .items_center()
                        .gap_3()
                        .p_3()
                        .rounded_md()
                        .bg(theme.secondary)
                        .border_1()
                        .border_color(if d.is_primary {
                            theme.green
                        } else {
                            theme.border
                        })
                        .child(
                            div()
                                .w(px(preview_w as f32))
                                .h(px(preview_h as f32))
                                .rounded_sm()
                                .bg(theme.background)
                                .border_1()
                                .border_color(if d.is_primary {
                                    theme.green
                                } else {
                                    theme.primary
                                })
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(theme.foreground)
                                        .child(format!(
                                            "{:.0} x {:.0}",
                                            d.bounds.width, d.bounds.height
                                        )),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(format!("{:.2}x", d.scale_factor)),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(theme.foreground)
                                                .child(format!("Display {}", d.id)),
                                        )
                                        .child(if d.is_primary {
                                            Tag::success().small().child("Primary")
                                        } else {
                                            Tag::secondary().small().child("Secondary")
                                        }),
                                )
                                .child(div().text_xs().text_color(theme.muted_foreground).child(
                                    format!(
                                        "Origin ({:.0}, {:.0}) • {}",
                                        d.bounds.x,
                                        d.bounds.y,
                                        d.refresh_rate.map_or_else(
                                            || "Unknown Hz".into(),
                                            |hz| format!("{hz} Hz")
                                        )
                                    ),
                                )),
                        )
                }),
            ));
        }
        Err(err) => {
            container = container.child(
                div()
                    .p_2()
                    .rounded_sm()
                    .bg(theme.background)
                    .text_xs()
                    .text_color(theme.yellow)
                    .child(format!("displays() error: {err}")),
            );
        }
    }

    container
}

fn render_pointer_and_settings(snapshot: Option<&DisplaySnapshot>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let Some(snapshot) = snapshot else {
        return card(cx).child(section_title("Cursor & System Settings", cx));
    };

    let mouse_text = match &snapshot.mouse_position {
        Ok((x, y)) => {
            let at_display = match &snapshot.display_at_cursor {
                Some(Ok(Some(d))) => format!("on Display {}", d.id),
                Some(Ok(None)) => String::from("outside reported display bounds"),
                Some(Err(err)) => format!("display_at_point error: {err}"),
                None => String::new(),
            };
            format!("({x:.1}, {y:.1}) — {at_display}")
        }
        Err(err) => format!("Unavailable: {err}"),
    };

    let settings_block = match &snapshot.settings {
        Ok(s) => div()
            .flex()
            .flex_wrap()
            .gap_3()
            .child(kv_pill(
                "Key Repeat Rate",
                s.keyboard_repeat_rate
                    .map_or_else(|| "-".into(), |v| v.to_string()),
                cx,
            ))
            .child(kv_pill(
                "Key Repeat Delay",
                s.keyboard_repeat_delay
                    .map_or_else(|| "-".into(), |v| format!("{v} ms")),
                cx,
            ))
            .child(kv_pill(
                "Mouse Sensitivity",
                s.mouse_sensitivity
                    .map_or_else(|| "-".into(), |v| format!("{v:.2}")),
                cx,
            ))
            .child(kv_pill(
                "Double-Click Time",
                s.double_click_time
                    .map_or_else(|| "-".into(), |v| format!("{v} ms")),
                cx,
            ))
            .child(kv_pill(
                "Keyboard Layout",
                s.keyboard_layout.clone().unwrap_or_else(|| "-".into()),
                cx,
            )),
        Err(err) => div()
            .p_2()
            .rounded_sm()
            .bg(theme.background)
            .text_xs()
            .text_color(theme.yellow)
            .child(format!("system_settings(): {err}")),
    };

    card(cx)
        .child(section_title("Cursor & System Settings", cx))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_xs()
                .child(
                    div()
                        .text_color(theme.muted_foreground)
                        .child("mouse_position():"),
                )
                .child(div().text_color(theme.foreground).child(mouse_text)),
        )
        .child(settings_block)
}

fn kv_pill(label: &'static str, value: String, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .px_3()
        .py_2()
        .rounded_md()
        .bg(theme.secondary)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(
            div()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(value),
        )
}

fn render_capability_matrix(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let rows = capability_matrix();

    card(cx)
        .child(section_title("Platform Capability Matrix", cx))
        .children(rows.into_iter().enumerate().map(|(idx, row)| {
            let status_tag = match row.support {
                "Supported" => Tag::success().small().child(row.support),
                "Observe-only" | "No-op lease" => Tag::warning().small().child(row.support),
                _ => Tag::secondary().small().child(row.support),
            };
            div()
                .id(("cap-row", idx))
                .flex()
                .flex_wrap()
                .items_center()
                .gap_3()
                .px_2p5()
                .py_1p5()
                .rounded_sm()
                .bg(theme.secondary)
                .text_xs()
                .child(
                    div()
                        .w(px(210.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(row.capability),
                )
                .child(div().w(px(115.0)).child(status_tag))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(220.0))
                        .text_color(theme.muted_foreground)
                        .child(row.notes),
                )
        }))
}
