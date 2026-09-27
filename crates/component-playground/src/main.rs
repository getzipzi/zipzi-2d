mod playground;

use gpui::{prelude::*, *};

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        ui::bind_text_input_keys(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Kasane Component Playground".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1180.0), px(800.0)),
                    cx,
                ))),
                window_min_size: Some(size(px(720.0), px(480.0))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| playground::Playground::new(window, cx)),
        )
        .expect("open component playground");
        cx.activate(true);
    });
}
