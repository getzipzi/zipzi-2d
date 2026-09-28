use crate::EditorView;
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable};
use gpui_kit::{prelude::*, *};

pub struct StatusBar {
    owner: WeakEntity<EditorView>,
    _observe: Subscription,
}

impl StatusBar {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
        })
    }
}

impl Render for StatusBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        render_status_bar(owner.read(cx), cx).into_any_element()
    }
}

fn render_status_bar(view: &EditorView, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let error = view
        .edit_error
        .clone()
        .or_else(|| view.render_error.clone());
    let message = error.clone().unwrap_or_else(|| view.message.clone());
    let is_error = error.is_some();
    let selected_layer = view.editor.selected_layer();

    div()
        .id("app-status-bar")
        .h(px(24.))
        .w_full()
        .flex_shrink_0()
        .bg(theme.background)
        .border_t_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .text_xs()
        // Left side: status message
        .child(
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .text_color(if is_error {
                    theme.danger
                } else {
                    theme.muted_foreground
                })
                .when(is_error, |e| {
                    e.child(Icon::new(IconName::CircleAlert).small())
                })
                .child(
                    div()
                        .max_w(px(500.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(message),
                ),
        )
        // Right side: metrics
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .text_color(theme.muted_foreground)
                .when_some(selected_layer, |e, layer| {
                    e.child(div().child(format!(
                        "{}: {}v / {}t",
                        layer.name, layer.vertices, layer.triangles
                    )))
                }),
        )
}
