use gpui::{App, Div, Hsla, div, prelude::*, px};
use gpui_component::ActiveTheme as _;

pub fn card(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_2p5()
        .p_3p5()
        .rounded_lg()
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.border)
}

pub fn section_title(text: &'static str, cx: &App) -> Div {
    div()
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(cx.theme().foreground)
        .child(text)
}

pub fn stat_tile(
    label: &'static str,
    value: impl Into<gpui::SharedString>,
    accent: Hsla,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .flex_1()
        .min_w(px(120.0))
        .px_3()
        .py_2p5()
        .rounded_lg()
        .bg(theme.group_box)
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
                .text_base()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(accent)
                .child(value.into()),
        )
}
