mod curve_axis;
mod curves_navigation;
mod events_inspector;
mod keys_marquee;
mod panel_layout;

pub(super) fn tool(window: &mut gpui_kit::Window, id: &str, cx: &mut gpui_kit::App) {
    use gpui_kit::test::TestWindowExt;
    if window.try_find(id.to_owned()).is_some() {
        window.click(id.to_owned(), cx);
    } else {
        window.press("escape", cx);
        window.render_frame(cx);
        window.click("timeline-more", cx);
        window.render_frame(cx);
        window.click(
            format!("timeline-menu-{}", id.trim_start_matches("timeline-")),
            cx,
        );
    }
    window.render_frame(cx);
}
