//! Physics groups are navigated independently of animation playback.
use super::*;
use crate::panels::{properties::physics::group_name, toolbar::PanelToolbar};
use crate::ui::compact;
use editor_core::{DocumentId, ObjectKind, ObjectRef};
use gpui_kit::assets::IconName;
use kasane_core::document::{PhysicsAsset, PhysicsEdit};

fn selected(editor: &editor_core::Editor, asset: &PhysicsAsset) -> Option<usize> {
    let object = editor.active_inspection()?.object()?;
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
        window.focus(&self.focus, cx);
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() || owner.editor.active_id() != Some(document) {
                return;
            }
            let result = owner.editor.inspect_animation_resource(document, object);
            if result.is_ok() {
                owner
                    .dock
                    .as_ref()
                    .unwrap()
                    .show(PanelKind::Properties, None, window, cx);
            }
            owner.complete_edit(result, cx);
        });
    }
    fn physics_list_action(
        &mut self,
        document: DocumentId,
        action: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() || owner.editor.active_id() != Some(document) {
                return;
            }
            // An unfinished value belongs to the previous inspector selection.
            let _ = owner.editor.execute(editor_core::EditorCommand::CancelEdit);
            let doc = owner.editor.document().unwrap();
            let asset = doc.physics_definition();
            let position = asset.and_then(|a| selected(&owner.editor, a));
            let group = position.and_then(|i| asset.map(|a| a.data.settings[i].id.clone()));
            let intent = match action {
                0 => PhysicsEdit::CreateGroup {
                    asset_id: asset
                        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |a| a.id.clone()),
                    id: uuid::Uuid::new_v4().to_string(),
                    name: t!("physics.new_group").into_owned(),
                    before: None,
                },
                1 => {
                    let Some(id) = group.clone() else {
                        return;
                    };
                    PhysicsEdit::Duplicate {
                        ids: vec![id],
                        group_id: None,
                        before: None,
                    }
                }
                2 => {
                    let Some(id) = group.clone() else {
                        return;
                    };
                    PhysicsEdit::Delete {
                        ids: vec![id],
                        node_replacements: Default::default(),
                    }
                }
                3 | 4 => {
                    let (Some(id), Some(i), Some(a)) = (group.clone(), position, asset) else {
                        return;
                    };
                    let before = if action == 3 {
                        a.data.settings.get(i.saturating_sub(1))
                    } else {
                        a.data.settings.get(i + 2)
                    }
                    .map(|g| g.id.clone());
                    if before.as_ref() == Some(&id) {
                        return;
                    }
                    PhysicsEdit::ReorderGroups {
                        ids: vec![id],
                        before,
                    }
                }
                _ => return,
            };
            let mut object = group.map(|id| ObjectRef::new(ObjectKind::PhysicsGroup, id));
            let result = (|| {
                let plan = owner.editor.plan_physics_authoring(document, intent)?;
                let result = owner.editor.execute_physics_authoring(document, &plan)?;
                if let Some(created) = result
                    .suggested_selection
                    .iter()
                    .find(|r| r.kind == ObjectKind::PhysicsGroup)
                {
                    object = Some(created.clone());
                }
                if object.as_ref().is_some_and(|r| {
                    !owner
                        .editor
                        .document()
                        .unwrap()
                        .object_index()
                        .get(r)
                        .is_some()
                }) {
                    object = owner
                        .editor
                        .document()
                        .unwrap()
                        .physics_definition()
                        .and_then(|a| {
                            let next = position
                                .unwrap_or(0)
                                .min(a.data.settings.len().saturating_sub(1));
                            a.data
                                .settings
                                .get(next)
                                .map(|g| ObjectRef::new(ObjectKind::PhysicsGroup, &g.id))
                        });
                }
                Ok(())
            })();
            if result.is_ok()
                && let Some(object) = object
            {
                let _ = owner.editor.inspect_animation_resource(document, object);
                owner
                    .dock
                    .as_ref()
                    .unwrap()
                    .show(PanelKind::Properties, None, window, cx);
            }
            owner.animation_anchor = None;
            owner.complete_edit(result, cx);
        });
    }
    pub(super) fn physics_list_toolbar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let Some(document) = state.editor.active_id() else {
            return Empty.into_any_element();
        };
        let asset = state
            .editor
            .document()
            .and_then(|doc| doc.physics_definition());
        let index = asset.and_then(|asset| selected(&state.editor, asset));
        let count = asset.map_or(0, |a| a.data.settings.len());
        let disabled = state.modal || state.editor.is_busy() || dialog;
        let mut toolbar = PanelToolbar::new("physics-list-toolbar");
        for (action, id, icon, label, off) in [
            (
                0,
                "physics-add-group",
                IconName::Plus,
                "physics.add_group",
                false,
            ),
            (
                1,
                "physics-copy-group",
                IconName::Copy,
                "physics.copy_group",
                index.is_none(),
            ),
            (
                2,
                "physics-delete-group",
                IconName::Trash,
                "physics.delete_group",
                index.is_none(),
            ),
            (
                3,
                "physics-up-group",
                IconName::ArrowUp,
                "physics.up",
                index.is_none_or(|i| i == 0),
            ),
            (
                4,
                "physics-down-group",
                IconName::ArrowDown,
                "physics.down",
                index.is_none_or(|i| i + 1 >= count),
            ),
        ] {
            let listener = cx.listener(move |this, _, window, cx| {
                this.physics_list_action(document, action, window, cx)
            });
            toolbar = toolbar.action(move |_| {
                Button::new(id)
                    .icon(icon)
                    .ghost()
                    .tooltip(t!(label))
                    .accessibility_label(t!(label))
                    .disabled(disabled || off)
                    .on_click(listener)
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
            .track_focus(&self.focus)
            .key_context("KasaneDocument")
            .text_size(compact::rem(compact::BODY));
        let Some(owner) = self.owner.upgrade() else {
            return root.into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let Some(document) = state.editor.active_id() else {
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
        let disabled = state.modal || state.editor.is_busy() || dialog;
        let choices: Vec<_> = asset
            .data
            .settings
            .iter()
            .map(|rig| reference(&rig.id))
            .collect();
        for (i, rig) in asset.data.settings.iter().enumerate() {
            let object = reference(&rig.id);
            root = root.child(
                self.resource_row(
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
                        .flex_shrink_0()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{}", i + 1)),
                )
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
                ),
            );
        }
        root.on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
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
        }))
        .into_any_element()
    }
}
