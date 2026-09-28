use crate::{EditorView, t};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::Button,
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};

pub struct DeformerTreePanel {
    _owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
}

impl DeformerTreePanel {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            let observe = cx.observe(&owner.upgrade().unwrap(), |_, _, cx| {
                cx.notify();
            });
            Self {
                _owner: owner,
                focus,
                _observe: observe,
            }
        })
    }
}

impl EventEmitter<PanelEvent> for DeformerTreePanel {}
impl Focusable for DeformerTreePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for DeformerTreePanel {
    fn panel_name(&self) -> &'static str {
        "Deformers"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for DeformerTreePanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.deformer_tree")
    }
}

impl Render for DeformerTreePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .id("deformer-tree-panel")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .track_focus(&self.focus)
            // Header
            .child(
                div()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.muted_foreground)
                            .child(t!("editor.deformer_tree")),
                    ),
            )
            // Content
            .child(
                div().flex_1().p_3().flex().flex_col().gap_3().child(
                    div()
                        .p_3()
                        .rounded_md()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.background)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(t!("editor.deformer_edit_unavailable")),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1p5()
                                .child(
                                    Button::new("add_warp")
                                        .small()
                                        .label(t!("editor.add_warp_deformer").into_owned())
                                        .disabled(true),
                                )
                                .child(
                                    Button::new("add_rot")
                                        .small()
                                        .label(t!("editor.add_rotation_deformer").into_owned())
                                        .disabled(true),
                                ),
                        ),
                ),
            )
            .into_any_element()
    }
}
