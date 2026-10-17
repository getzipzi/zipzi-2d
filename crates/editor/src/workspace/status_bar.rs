use crate::{EditorView, ui::compact};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable};
use gpui_kit::{prelude::*, *};

const STATUS_LINE_HEIGHT: f32 = compact::CAPTION * 1.5;

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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        render_status_bar(owner.read(cx), cx).into_any_element()
    }
}

fn render_status_bar(view: &EditorView, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let error = view.edit_error.clone().or_else(|| view.render_error());
    let message = error.clone().unwrap_or_else(|| view.message.clone());
    let is_error = error.is_some();
    let selected_layer = view.editor.selected_layer();

    div()
        .id("app-status-bar")
        .test_support()
        .pt_0()
        .pb(compact::rem(
            (compact::STATUS_BAR - STATUS_LINE_HEIGHT) / 2.,
        ))
        .w_full()
        .flex_shrink_0()
        .bg(compact::workspace(cx))
        .flex()
        .items_center()
        .gap(crate::ui::compact::rem(compact::PADDING))
        .px(compact::chrome_inset(cx))
        .text_size(crate::ui::compact::rem(compact::CAPTION))
        .line_height(compact::rem(STATUS_LINE_HEIGHT))
        // Left side: status message
        .child(
            div()
                .id("app-status-message")
                .test_support()
                .h(compact::rem(STATUS_LINE_HEIGHT))
                .flex()
                .flex_1()
                .min_w_0()
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
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(message),
                ),
        )
        // Right side: metrics
        .child(
            div()
                .flex()
                .min_w_0()
                .items_center()
                .gap(crate::ui::compact::rem(compact::PADDING))
                .text_color(theme.muted_foreground)
                .when_some(selected_layer, |e, layer| {
                    e.child(div().overflow_hidden().text_ellipsis().child(format!(
                        "{}: {}v / {}t",
                        layer.name, layer.vertices, layer.triangles
                    )))
                }),
        )
}
