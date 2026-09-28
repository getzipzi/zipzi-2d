use crate::t;
use gpui_kit::component::{
    ActiveTheme,
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};

pub struct ParametersPanel {
    focus: FocusHandle,
}

impl ParametersPanel {
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            focus: cx.focus_handle(),
        })
    }
}

impl EventEmitter<PanelEvent> for ParametersPanel {}
impl Focusable for ParametersPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for ParametersPanel {
    fn panel_name(&self) -> &'static str {
        "Parameters"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for ParametersPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.parameters")
    }
}

impl Render for ParametersPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id("parameters-panel")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .track_focus(&self.focus)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child(t!("editor.parameters")),
            )
            .child(
                div()
                    .p_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("editor.parameter_edit_unavailable")),
            )
            .into_any_element()
    }
}
