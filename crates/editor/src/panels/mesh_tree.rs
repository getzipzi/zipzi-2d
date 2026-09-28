use crate::{EditorView, t};
use editor_core::EditorCommand;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};

pub struct MeshTreePanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
}

impl MeshTreePanel {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            let observe = cx.observe(&owner.upgrade().unwrap(), |_, _, cx| {
                cx.notify();
            });
            Self {
                owner,
                focus,
                _observe: observe,
            }
        })
    }
}

impl EventEmitter<PanelEvent> for MeshTreePanel {}
impl Focusable for MeshTreePanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for MeshTreePanel {
    fn panel_name(&self) -> &'static str {
        "MeshTree"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for MeshTreePanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.mesh_tree")
    }
}

impl Render for MeshTreePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };

        let state = owner.read(cx);
        let layers = state.editor.layers();
        let selected = state.editor.selected_layer();
        let disabled = state.modal || state.editor.is_busy();
        let target = state.editor.active_id();
        let theme = cx.theme();

        div()
            .id("mesh-tree-panel")
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
                            .child(format!("{} ({})", t!("editor.mesh_tree"), layers.len())),
                    ),
            )
            .child(if layers.is_empty() {
                div()
                    .p_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("editor.no_meshes"))
                    .into_any_element()
            } else {
                div()
                    .id("mesh-tree-scroll-area")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_1p5()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(layers.into_iter().map(|layer| {
                        let id = layer.id.clone();
                        let is_selected = selected.as_ref().is_some_and(|s| s.id == layer.id);
                        let owner = self.owner.clone();

                        div()
                            .id(SharedString::from(format!("mesh-item-{}", id)))
                            .flex()
                            .items_center()
                            .h(px(26.))
                            .px_2()
                            .rounded_md()
                            .bg(if is_selected {
                                theme.primary.opacity(0.15)
                            } else {
                                gpui::transparent_black()
                            })
                            .border_1()
                            .border_color(if is_selected {
                                theme.primary.opacity(0.4)
                            } else {
                                gpui::transparent_black()
                            })
                            .hover(|s| {
                                if !is_selected {
                                    s.bg(theme.muted.opacity(0.5))
                                } else {
                                    s
                                }
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |_, _, window, cx| {
                                if disabled {
                                    return;
                                }
                                let Some(owner) = owner.upgrade() else {
                                    return;
                                };
                                let Some(target) = target else {
                                    return;
                                };
                                owner.update(cx, |owner, cx| {
                                    if owner.modal
                                        || window.has_active_dialog(cx)
                                        || owner.editor.is_busy()
                                        || owner.editor.active_id() != Some(target)
                                    {
                                        return;
                                    }
                                    let res = owner
                                        .editor
                                        .execute_for(target, EditorCommand::Select(id.clone()));
                                    owner.complete_edit(res, cx);
                                });
                            }))
                            .child(
                                div()
                                    .mr_1p5()
                                    .text_color(theme.muted_foreground)
                                    .child(Icon::new(gpui_kit::assets::IconName::Grid2x2).small()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .text_color(if is_selected {
                                        theme.primary
                                    } else {
                                        theme.foreground
                                    })
                                    .child(if layer.name.is_empty() {
                                        t!("editor.untitled_layer").into_owned()
                                    } else {
                                        layer.name
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground.opacity(0.7))
                                    .child(format!("{} tris", layer.triangles)),
                            )
                    }))
                    .into_any_element()
            })
            .into_any_element()
    }
}
