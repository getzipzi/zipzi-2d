//! Organization projection creation shared by row menus, toolbars, native
//! menus and configured shortcuts. The core owns defaults and placement.
use super::*;
use editor_core::{Command, CommandId, SceneObjectCreationPlan, SceneObjectType};

pub(crate) const COMMANDS: [CommandId; 5] = [
    CommandId::CreatePart,
    CommandId::CreateMesh,
    CommandId::CreateWarp,
    CommandId::CreateRotation,
    CommandId::CreateOffscreen,
];

pub(crate) fn default_name(kind: SceneObjectType) -> String {
    t!(match kind {
        SceneObjectType::Part => "editor.tree_type_part",
        SceneObjectType::Mesh => "editor.tree_type_mesh",
        SceneObjectType::Warp => "editor.tree_type_warp",
        SceneObjectType::Rotation => "editor.tree_type_rotation",
        SceneObjectType::Offscreen => "editor.tree_type_offscreen",
    })
    .into_owned()
}

pub(crate) fn creation_error(error: &anyhow::Error) -> String {
    let message = error.to_string();
    if message.contains("OBJECT_LOCKED") {
        t!("authoring.locked").into_owned()
    } else if message.contains("OFFSCREEN_REQUIRES_PART") {
        t!("authoring.offscreen_requires_part").into_owned()
    } else if message.contains("DUPLICATE_OWNER") {
        t!("authoring.move_offscreen_owner").into_owned()
    } else {
        message
    }
}

impl EditorView {
    pub(crate) fn handle_scene_creation_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(kind), Some(document)) = (command.id.scene_creation_type(), command.document())
        else {
            return;
        };
        if self.editor.active_id() != Some(document) {
            return;
        }
        // Capture the same organization selection as the toolbar before input
        // settlement can notify another control. Deformation placement is separate.
        let selection = self.editor.document().unwrap().context().selections.clone();
        if !self.prepare_property_inputs(document, window, cx) {
            return;
        }
        let Some(context) = AuthoringContext::capture(&self.editor) else {
            return;
        };
        match self.editor.document().unwrap().plan_scene_object_creation(
            kind,
            default_name(kind),
            &selection,
        ) {
            Ok(plan) => self.apply_scene_creation(context, &plan, kind, window, cx),
            Err(error) => self.set_edit_error(creation_error(&error), cx),
        }
    }

    pub(crate) fn apply_scene_creation(
        &mut self,
        context: AuthoringContext,
        plan: &SceneObjectCreationPlan,
        kind: SceneObjectType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let document = context.editing.document;
        let result = self
            .execute_authoring(context, window, cx, |editor, document| {
                editor.execute_scene_object_creation(document, plan)
            })
            .and_then(|result| {
                if let Some(dock) = &self.dock {
                    if !dock.displayed(editor_core::PanelKind::MeshTree, cx) {
                        dock.show(editor_core::PanelKind::MeshTree, None, window, cx);
                    }
                }
                // Properties is shown last so its name field stays available
                // even when both panels share a user-configured tab group.
                self.present_created_object(document, &result, kind.object_kind(), window, cx)
            });
        match result {
            Ok(object) => {
                self.clear_edit_error(cx);
                self.redraw(cx);
                if let (Some(object), Some(dock)) = (object, &self.dock) {
                    let panel = dock.mesh_tree.downgrade();
                    let owner = cx.weak_entity();
                    let after = AuthoringContext::capture(&self.editor);
                    window.defer(cx, move |window, cx| {
                        if owner.upgrade().is_some_and(|owner| {
                            let state = owner.read(cx);
                            AuthoringContext::capture(&state.editor) == after
                                && state.editor.document().is_some_and(|doc| {
                                    doc.context().inspected.as_ref() == Some(&object)
                                })
                        }) {
                            _ = panel.update(cx, |panel, cx| {
                                panel.locate_created_object(document, object, window, cx);
                            });
                        }
                    });
                }
            }
            Err(error) => self.set_edit_error(creation_error(&error), cx),
        }
    }
}
