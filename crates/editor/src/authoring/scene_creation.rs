//! Explicit scene projection creation shared by row menus, toolbars, native
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

pub(crate) const DEFORMATION_COMMANDS: [CommandId; 3] = [
    CommandId::CreateDeformationMesh,
    CommandId::CreateDeformationWarp,
    CommandId::CreateDeformationRotation,
];
pub(crate) fn all_commands() -> impl Iterator<Item = CommandId> {
    COMMANDS.into_iter().chain(DEFORMATION_COMMANDS)
}

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
    if super::error_code(error) == Some("OFFSCREEN_REQUIRES_PART") {
        return t!("authoring.offscreen_requires_part").into_owned();
    }
    super::authoring_error(error)
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
        // Capture the same scene selection as the toolbar before input
        // settlement can notify another control. The command names its projection.
        let selection = self.editor.document().unwrap().context().selections.clone();
        if !self.prepare_property_inputs(document, window, cx) {
            return;
        }
        let Some(context) = AuthoringContext::capture(&self.editor) else {
            return;
        };
        match self
            .editor
            .document()
            .unwrap()
            .plan_scene_object_creation_in(
                command.id.scene_creation_projection().unwrap(),
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
        let projection = plan.projection();
        let result = self
            .execute_authoring(context, window, cx, |editor, document| {
                editor.execute_scene_object_creation(document, plan)
            })
            .and_then(|result| {
                self.present_authoring_result(
                    document,
                    &result,
                    AuthoringPresentation::Created {
                        kind: kind.object_kind(),
                        projection: Some(projection),
                    },
                    window,
                    cx,
                )
            });
        match result {
            Ok(_) => {
                self.clear_edit_error(cx);
                self.redraw(cx);
            }
            Err(error) => self.set_edit_error(creation_error(&error), cx),
        }
    }
}
