use crate::{EditorView, t};
use editor_core::CommandId;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub fn render_zoom_bar(owner: &Entity<EditorView>, modal: bool, cx: &App) -> impl IntoElement {
    let state = owner.read(cx);
    let zoom_pct = (state.editor.view().zoom * 100.0).round() as i32;
    let button = |id, command: CommandId, icon| {
        let enabled = !modal
            && state
                .editor
                .command_available(state.editor.command(command));
        let owner = owner.clone();
        Button::new(id)
            .xsmall()
            .ghost()
            .icon(icon)
            .tooltip(t!(command.spec().label))
            .disabled(!enabled)
            .on_click(move |_, window, cx| {
                owner.update(cx, |owner, cx| owner.run_command(command, window, cx));
            })
    };
    div()
        .id("status-zoom")
        .test_support()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .text_color(cx.theme().muted_foreground)
        .child(button("btn_zoom_out", CommandId::ZoomOut, IconName::Minus))
        .child(
            div()
                .id("zoom-percentage")
                .test_support()
                .min_w(px(40.))
                .text_center()
                .text_xs()
                .font_family(".AppleSystemUIFontMonospaced")
                .child(format!("{}%", zoom_pct)),
        )
        .child(button("btn_zoom_in", CommandId::ZoomIn, IconName::Plus))
        .child(
            div()
                .ml_1()
                .pl_1()
                .border_l_1()
                .border_color(cx.theme().border)
                .child(button(
                    "btn_zoom_fit",
                    CommandId::FitView,
                    IconName::Maximize,
                )),
        )
}
