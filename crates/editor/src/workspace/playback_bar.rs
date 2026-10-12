use crate::{EditorView, t};
use editor_core::CommandId;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

/// The transport occupies a fixed, reachable row even in a small window.
/// Its command stays unavailable until the detached preview is connected.
pub fn render_playback_bar(
    view: &EditorView,
    modal: bool,
    cx: &mut Context<EditorView>,
) -> impl IntoElement {
    let command = CommandId::PlayPause;
    div()
        .id("animation-playback-bar")
        .test_support()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap_3()
        .h(px(40.))
        .px_3()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            Button::new("animation-play-pause")
                .small()
                .ghost()
                .label(t!(command.spec().label))
                .tooltip(t!("workspace.preview_pending"))
                .disabled(modal || !view.editor.command_available(view.editor.command(command)))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.run_command(command, window, cx)),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("0:00"),
        )
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(t!("workspace.preview_pending")),
        )
}
