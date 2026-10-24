//! Physics groups are navigated independently of animation playback.
use super::*;
use crate::authoring::{AuthoringContext, business};
use crate::panels::sorting::{self, SortItem};
use crate::panels::{properties::physics::group_name, toolbar::PanelToolbar};
use crate::ui::compact;
use gpui_kit::assets::IconName;
use gpui_kit::component::menu::ContextMenuExt;
use zipzi_core::document::PhysicsAsset;
use zipzi_editor_core::BusinessAction as A;
use zipzi_editor_core::{DocumentId, ObjectKind, ObjectRef};

fn selected(editor: &zipzi_editor_core::Editor, asset: &PhysicsAsset) -> Option<usize> {
    let mut object = editor.active_inspection()?.object()?;
    if matches!(
        object.kind,
        ObjectKind::PhysicsInput | ObjectKind::PhysicsOutput | ObjectKind::PhysicsNode
    ) {
        object = editor.document()?.business_owner(&object)?;
    }
    asset
        .data
        .settings
        .iter()
        .position(|rig| object == reference(&rig.id))
}
fn reference(group: &str) -> ObjectRef {
    ObjectRef::new(ObjectKind::PhysicsGroup, group)
}

impl AnimationPanel {
    fn choose_physics(
        &mut self,
        document: DocumentId,
        object: ObjectRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let focus = self.focus.clone();
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() || owner.editor.active_id() != Some(document) {
                return;
            }
            if !owner.prepare_property_inputs(document, window, cx) {
                return;
            }
            window.focus(&focus, cx);
            let result = owner.inspect_authoring_object(document, object, false, window, cx);
            owner.complete_edit(result, cx);
        });
    }
    pub(super) fn physics_list_toolbar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut toolbar = PanelToolbar::new("physics-list-toolbar");
        let Some(owner) = self.owner.upgrade() else {
            return toolbar.into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let Some(doc) = state.editor.document() else {
            return toolbar.into_any_element();
        };
        let document = state.editor.active_id().unwrap();
        let context = AuthoringContext::capture(&state.editor);
        let group = doc
            .physics_definition()
            .and_then(|a| selected(&state.editor, a).map(|i| reference(&a.data.settings[i].id)));
        let action = A::Create(ObjectKind::PhysicsGroup, group);
        let available = business::availability(&state.editor, &action);
        let off = state.modal || dialog || available.as_ref().is_none_or(|a| !a.enabled);
        let hint = available
            .and_then(|a| a.reason)
            .unwrap_or_else(|| t!("physics.add_group").into());
        let create_owner = self.owner.clone();
        toolbar = toolbar.action(move |_| {
            compact::new_group_button(Button::new("physics-add-group"))
                .accessibility_label(t!("physics.add_group"))
                .tooltip(hint)
                .disabled(off)
                .on_click(move |_, window, cx| {
                    if let Some(context) = context {
                        let _ = create_owner.update(cx, |v, cx| {
                            v.run_business_action(context, action.clone(), window, cx)
                        });
                    }
                })
        });
        if let Some(asset) = doc.physics_definition() {
            let object = ObjectRef::new(ObjectKind::Physics, &asset.id);
            let owner = self.owner.clone();
            let off = state.modal || dialog;
            toolbar = toolbar.action(move |_| {
                Button::new("physics-inspect-root")
                    .icon(IconName::Settings)
                    .ghost()
                    .tooltip(t!("authoring.physics_settings"))
                    .accessibility_label(t!("authoring.physics_settings"))
                    .disabled(off)
                    .on_click(move |_, window, cx| {
                        if let Some(context) = context {
                            let _ = owner.update(cx, |v, cx| {
                                if AuthoringContext::capture(&v.editor) == Some(context)
                                    && v.prepare_property_inputs(document, window, cx)
                                {
                                    let result = v.inspect_authoring_object(
                                        document,
                                        object.clone(),
                                        false,
                                        window,
                                        cx,
                                    );
                                    v.complete_edit(result, cx);
                                }
                            });
                        }
                    })
            });
        }
        toolbar.into_any_element()
    }
    pub(super) fn render_physics_list(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut root = div()
            .id("AnimationPhysics")
            .test_support()
            .size_full()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.resources.view.scroll)
            .track_focus(&self.focus)
            .key_context("ZipziDocument ZipziPalette")
            .text_size(compact::rem(compact::BODY));
        let Some(owner) = self.owner.upgrade() else {
            return root.into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let disabled = state.modal || state.editor.is_busy() || dialog;
        let document = state.editor.active_id();
        self.sorting.update(cx, |sorting, cx| {
            sorting.reconcile(document, !disabled, window, cx);
        });
        let state = owner.read(cx);
        let Some(document) = document else {
            return root
                .child(div().p_2().child(t!("editor.welcome")))
                .into_any_element();
        };
        let Some(asset) = state
            .editor
            .document()
            .and_then(|doc| doc.physics_definition())
        else {
            return root
                .child(
                    div()
                        .id("animation-physics-empty")
                        .test_support()
                        .p_2()
                        .child(t!("workspace.no_physics")),
                )
                .into_any_element();
        };
        let index = selected(&state.editor, asset);
        root = sorting::scroll_div(root, &self.sorting, &self.resources.view.scroll);
        let choices: Vec<_> = asset
            .data
            .settings
            .iter()
            .map(|rig| reference(&rig.id))
            .collect();
        let location = self
            .location
            .take()
            .filter(|(id, _)| *id == document)
            .map(|(_, r)| r);
        for (i, rig) in asset.data.settings.iter().enumerate() {
            if location.as_ref().is_some_and(|r| {
                r.id == rig.id
                    || rig.inputs.iter().any(|i| i.id == r.id)
                    || rig.outputs.iter().any(|i| i.id == r.id)
                    || rig.vertices.iter().any(|i| i.id == r.id)
            }) {
                self.resources.view.scroll.scroll_to_top_of_item(i);
            }
            let object = reference(&rig.id);
            let row = self
                .resource_row(
                    ("physics-group-row", i),
                    index == Some(i),
                    i,
                    disabled,
                    window,
                    cx,
                )
                .test_support()
                .aria_label(group_name(asset, &rig.id).to_owned())
                .aria_selected(index == Some(i))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(group_name(asset, &rig.id).to_owned()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(compact::rem(compact::CAPTION))
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{} → {}", rig.inputs.len(), rig.outputs.len())),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if !disabled {
                            this.choose_physics(document, object.clone(), window, cx);
                        }
                    }),
                );
            let item = SortItem::PhysicsGroup(reference(&rig.id));
            let row = sorting::drop_target(row, &self.sorting, item.clone(), None, cx);
            root = root.child(
                sorting::drag_source(
                    row,
                    &self.sorting,
                    document,
                    item,
                    group_name(asset, &rig.id).to_owned(),
                    !disabled,
                )
                .context_menu(business::menu(self.owner.clone(), reference(&rig.id))),
            );
        }
        crate::ui::compact::scroll_frame(
            sorting::panel_actions(root, &self.sorting).on_key_down(cx.listener(
                move |this, event: &KeyDownEvent, window, cx| {
                    if disabled || !this.focus.is_focused(window) || choices.is_empty() {
                        return;
                    }
                    let next = match event.keystroke.key.as_str() {
                        "down" => index.map_or(0, |i| (i + 1).min(choices.len() - 1)),
                        "up" => index.map_or(choices.len() - 1, |i| i.saturating_sub(1)),
                        "enter" => index.unwrap_or(0),
                        _ => return,
                    };
                    this.choose_physics(document, choices[next].clone(), window, cx);
                    cx.stop_propagation();
                },
            )),
            &self.resources.view.scroll,
        )
        .into_any_element()
    }
}
