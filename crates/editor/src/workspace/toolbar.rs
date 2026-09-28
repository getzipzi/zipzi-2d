use crate::{EditorView, t};
use editor_core::CommandId;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, Theme, ThemeMode, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

impl EditorView {
    pub(crate) fn set_hand_tool(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.editor.cancel_gesture();
        if let Some(dock) = &self.dock {
            dock.canvas
                .update(cx, |canvas, cx| canvas.set_hand_tool(enabled, cx));
        }
        self.redraw(cx);
    }
}

pub fn render_toolbar(
    view: &EditorView,
    modal: bool,
    _window: &mut Window,
    cx: &mut Context<EditorView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let zoom_pct = (view.editor.view().zoom * 100.0).round() as i32;
    let hand_tool = view
        .dock
        .as_ref()
        .is_some_and(|dock| dock.canvas.read(cx).hand_tool());

    div()
        .id("app-toolbar")
        .h(px(40.))
        .w_full()
        .flex_shrink_0()
        .bg(theme.background)
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .px_3()
        .gap_2()
        // App Title or Brand
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .mr_2()
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .rounded(px(5.))
                        .bg(theme.primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.primary_foreground)
                        .font_weight(FontWeight::BOLD)
                        .text_xs()
                        .child("K"),
                )
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.foreground)
                        .child(t!("app.title")),
                ),
        )
        // Divider
        .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
        // Tool group (Select, Mesh, Deformer, Hand)
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new("tool_select")
                        .small()
                        .icon(IconName::MousePointer)
                        .label(t!("editor.tool_select").into_owned())
                        .when(!hand_tool, |button| button.primary())
                        .disabled(modal)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.modal && !window.has_active_dialog(cx) {
                                this.set_hand_tool(false, cx);
                            }
                        })),
                )
                .child(
                    Button::new("tool_mesh")
                        .small()
                        .icon(IconName::Grid2x2)
                        .label(t!("editor.tool_mesh").into_owned())
                        .disabled(true),
                )
                .child(
                    Button::new("tool_deformer")
                        .small()
                        .icon(IconName::Sparkles)
                        .label(t!("editor.tool_deformer").into_owned())
                        .disabled(true),
                )
                .child(
                    Button::new("tool_hand")
                        .small()
                        .icon(IconName::Hand)
                        .label(t!("editor.tool_hand").into_owned())
                        .when(hand_tool, |button| button.primary())
                        .disabled(modal)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if !this.modal && !window.has_active_dialog(cx) {
                                this.set_hand_tool(true, cx);
                                if let Some(dock) = &this.dock {
                                    window.focus(&dock.canvas.read(cx).focus_handle(cx), cx);
                                }
                            }
                        })),
                ),
        )
        // Divider
        .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
        // View Actions (Vertices, Reset View)
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new("btn_vertices")
                        .small()
                        .icon(IconName::Eye)
                        .label(t!("menu.vertices").into_owned())
                        .disabled(
                            modal
                                || !view
                                    .editor
                                    .command_available(view.editor.command(CommandId::Vertices)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_command(CommandId::Vertices, window, cx);
                        })),
                )
                .child(
                    Button::new("btn_reset_view")
                        .small()
                        .icon(IconName::RotateCcw)
                        .label(t!("menu.reset_view").into_owned())
                        .disabled(
                            modal
                                || !view
                                    .editor
                                    .command_available(view.editor.command(CommandId::ResetView)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_command(CommandId::ResetView, window, cx);
                        })),
                ),
        )
        // Flexible spacer
        .child(div().flex_1())
        // Right side: Zoom percentage & controls, Theme Toggle
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                // Zoom Out
                .child(
                    Button::new("btn_zoom_out")
                        .small()
                        .icon(IconName::ZoomOut)
                        .disabled(
                            modal
                                || !view
                                    .editor
                                    .command_available(view.editor.command(CommandId::ZoomOut)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_command(CommandId::ZoomOut, window, cx);
                        })),
                )
                // Zoom Percentage Pill
                .child(
                    div()
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
                // Zoom In
                .child(
                    Button::new("btn_zoom_in")
                        .small()
                        .icon(IconName::ZoomIn)
                        .disabled(
                            modal
                                || !view
                                    .editor
                                    .command_available(view.editor.command(CommandId::ZoomIn)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run_command(CommandId::ZoomIn, window, cx);
                        })),
                )
                // Divider
                .child(div().w(px(1.)).h(px(16.)).bg(theme.border).mx_1())
                // Theme Toggle
                .child(
                    Button::new("btn_theme")
                        .small()
                        .icon(if theme.is_dark() {
                            IconName::Sun
                        } else {
                            IconName::Moon
                        })
                        .on_click(|_, window, cx| {
                            let mode = if cx.theme().is_dark() {
                                ThemeMode::Light
                            } else {
                                ThemeMode::Dark
                            };
                            Theme::change(mode, Some(window), cx);
                        }),
                ),
        )
}
