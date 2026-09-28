use crate::{EditorView, t};
use editor_core::EditorCommand;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};

pub struct LayersPanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
}

impl LayersPanel {
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

impl EventEmitter<PanelEvent> for LayersPanel {}
impl Focusable for LayersPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for LayersPanel {
    fn panel_name(&self) -> &'static str {
        "Layers"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for LayersPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.layers")
    }
}

impl Render for LayersPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };

        let state = owner.read(cx);
        let target = state.editor.active_id();
        let selected = state.editor.selected_layer();
        let disabled = state.modal || state.editor.is_busy();
        let layers = state.editor.layers();
        let theme = cx.theme();

        div()
            .id("layers-panel")
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .track_focus(&self.focus)
            // Header stats
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
                            .child(format!("{} ({})", t!("editor.layers"), layers.len())),
                    ),
            )
            // Layer items list
            .child(
                div()
                    .id("layers-scroll-area")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_1p5()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(layers.into_iter().map(|layer| {
                        let id = layer.id.clone();
                        let is_selected = selected.as_ref().is_some_and(|s| s.id == layer.id);
                        let is_visible = layer.visible;
                        let layer_name = if layer.name.is_empty() {
                            t!("editor.untitled_layer").into_owned()
                        } else {
                            layer.name.clone()
                        };

                        let item_bg = if is_selected {
                            theme.primary.opacity(0.15)
                        } else {
                            gpui::transparent_black()
                        };

                        let text_color = if is_selected {
                            theme.primary
                        } else {
                            theme.foreground
                        };

                        let owner_for_select = self.owner.clone();
                        let owner_for_vis = self.owner.clone();
                        let id_for_vis = id.clone();

                        div()
                            .id(SharedString::from(format!("layer-item-{}", id)))
                            .test_support()
                            .flex()
                            .items_center()
                            .h(px(28.))
                            .px_2()
                            .rounded_md()
                            .bg(item_bg)
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
                                let Some(owner) = owner_for_select.upgrade() else {
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
                                    let result = owner
                                        .editor
                                        .execute_for(target, EditorCommand::Select(id.clone()));
                                    owner.complete_edit(result, cx);
                                });
                            }))
                            // Visibility Toggle Eye
                            .child(
                                div()
                                    .id(SharedString::from(format!("layer-vis-{}", id_for_vis)))
                                    .test_support()
                                    .w(px(20.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(if is_visible {
                                        theme.muted_foreground
                                    } else {
                                        theme.muted_foreground.opacity(0.3)
                                    })
                                    .hover(|s| s.text_color(theme.foreground))
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        cx.stop_propagation();
                                        if disabled {
                                            return;
                                        }
                                        let Some(owner) = owner_for_vis.upgrade() else {
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
                                            let result = owner.editor.execute_for(
                                                target,
                                                EditorCommand::SetLayerVisible {
                                                    layer_id: id_for_vis.clone(),
                                                    visible: !is_visible,
                                                },
                                            );
                                            owner.complete_edit(result, cx);
                                        });
                                    }))
                                    .child(if is_visible {
                                        Icon::new(IconName::Eye).small()
                                    } else {
                                        Icon::new(IconName::EyeOff).small()
                                    }),
                            )
                            // Layer Name
                            .child(
                                div()
                                    .flex_1()
                                    .pl_1p5()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_xs()
                                    .text_color(text_color)
                                    .font_weight(if is_selected {
                                        FontWeight::MEDIUM
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(layer_name),
                            )
                            // Small Vertex Count Badge
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground.opacity(0.7))
                                    .child(format!("{}v", layer.vertices)),
                            )
                    })),
            )
            .into_any_element()
    }
}
