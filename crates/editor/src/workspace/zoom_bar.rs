use crate::{EditorView, canvas::CanvasView, t};
use editor_core::CommandId;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub fn render_floating_zoom_bar(
    owner: &Entity<EditorView>,
    modal: bool,
    cx: &mut Context<CanvasView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let state = owner.read(cx);
    let zoom_pct = (state.editor.view().zoom * 100.0).round() as i32;
    let can_zoom_out = !modal
        && state
            .editor
            .command_available(state.editor.command(CommandId::ZoomOut));
    let can_zoom_in = !modal
        && state
            .editor
            .command_available(state.editor.command(CommandId::ZoomIn));

    div()
        .id("canvas-floating-zoom")
        .test_support()
        .absolute()
        .bottom_2()
        .right_2()
        .flex()
        .items_center()
        .gap_1()
        .p_1()
        .rounded_md()
        .bg(theme.background.opacity(0.85))
        .border_1()
        .border_color(theme.border)
        .shadow_md()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .child(
            Button::new("btn_zoom_out")
                .small()
                .ghost()
                .icon(IconName::ZoomOut)
                .tooltip(t!("menu.zoom_out"))
                .disabled(!can_zoom_out)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(owner) = this.owner.upgrade() {
                        owner.update(cx, |owner, cx| {
                            owner.run_command(CommandId::ZoomOut, window, cx);
                        });
                    }
                })),
        )
        .child(
            div()
                .id("zoom-percentage")
                .test_support()
                .px_2()
                .py_0p5()
                .rounded(px(4.))
                .bg(theme.muted)
                .text_xs()
                .font_family(".AppleSystemUIFontMonospaced")
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(format!("{}%", zoom_pct)),
        )
        .child(
            Button::new("btn_zoom_in")
                .small()
                .ghost()
                .icon(IconName::ZoomIn)
                .tooltip(t!("menu.zoom_in"))
                .disabled(!can_zoom_in)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(owner) = this.owner.upgrade() {
                        owner.update(cx, |owner, cx| {
                            owner.run_command(CommandId::ZoomIn, window, cx);
                        });
                    }
                })),
        )
}
