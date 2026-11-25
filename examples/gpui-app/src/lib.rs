#![deny(clippy::all)]

pub mod app;
mod hook_runner;
mod model;
mod platform_info;
mod ui;

use app::MonioAppView;
use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;

const WINDOW_TITLE: &str = "Monio Input Studio";

#[cfg(not(target_env = "ohos"))]
pub fn run_desktop() {
    gpui_platform::application()
        .with_assets(gpui_kit_assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);

            let bounds = Bounds::centered(None, size(px(1140.0), px(780.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(WINDOW_TITLE.into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| MonioAppView::new(window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("failed to open Monio GPUI window");

            cx.activate(true);
        });
}

#[cfg(target_env = "ohos")]
thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static APPLICATION: std::cell::RefCell<Option<gpui::ApplicationHandle>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_env = "ohos")]
#[openharmony_ability_derive::ability]
pub fn openharmony_app(app: openharmony_ability::OpenHarmonyApp) {
    use gpui::Application;
    use log::LevelFilter;
    use ohos_hilog_binding::log::Config;

    ohos_hilog_binding::log::init_once(Config::default().with_max_level(LevelFilter::Info));

    let inner_app = app.clone();
    let application = Application::with_platform(gpui_ohos::current_platform(app, false))
        .with_assets(gpui_kit_assets::Assets);

    let application_handle = application.run_embedded(move |cx: &mut App| {
        gpui_component::init(cx);

        let info = inner_app.content_rect();
        let default_size = size(px(info.width as f32), px(info.height as f32));
        let bounds = Bounds::centered(None, default_size, cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(WINDOW_TITLE.into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| MonioAppView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("failed to open Monio GPUI window");

        cx.activate(true);
    });

    APPLICATION.with(|application| {
        application.replace(Some(application_handle));
    });
}
