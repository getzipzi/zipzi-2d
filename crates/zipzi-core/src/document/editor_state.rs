//! Persistent authoring preferences. Never read by runtime evaluation or export.
use super::*;

pub use zipzi_contract::document::{EditorState, ObjectEditorState};

impl Document {
    pub fn editor_state(&self) -> &EditorState {
        &self.content.editor_state
    }

    pub fn replace_editor_state(&mut self, mut state: EditorState) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Some(issue) = self.editor_state_issues(&state).into_iter().next() {
            return self.failed(issue.status);
        }
        state
            .objects
            .retain(|_, flags| flags.hidden_in_editor || flags.locked);
        if self.content.editor_state == state {
            return self.failed(Status::ok());
        }
        let mut changed: Vec<_> = self
            .content
            .editor_state
            .objects
            .keys()
            .chain(state.objects.keys())
            .cloned()
            .collect();
        changed.sort();
        changed.dedup();
        self.content.editor_state = state;
        self.changed(ChangeKind::Metadata, Vec::new(), changed)
    }

    pub(super) fn editor_state_issues(&self, state: &EditorState) -> Vec<StructureIssue> {
        state
            .objects
            .keys()
            .filter_map(|id| {
                let valid = self.reference_identity(id).is_some()
                    || self.binding_identity(id).is_some()
                    || self.get_glue_pair(id).is_some()
                    || self.content.assets.contains_key(id)
                    || self.content.meshes.contains_key(id)
                    || self.content.transforms.contains_key(id)
                    || self.content.parts.contains_key(id)
                    || self.content.offscreens.contains_key(id)
                    || self.content.glues.contains_key(id)
                    || self.content.parameters.contains_key(id)
                    || self.get_parameter_group(id).is_some()
                    || self.content.bindings.contains_key(id)
                    || self.content.scene_bindings.contains_key(id)
                    || self.content.blend_key_tables.contains_key(id)
                    || self.content.blend_constraints.contains_key(id)
                    || self.content.blend_bindings.contains_key(id)
                    || self.content.expressions.contains_key(id)
                    || self.get_expression_entry(id).is_some()
                    || self.motion_identity_exists(id)
                    || self.get_attachment(id).is_some()
                    || self.physics_owner(id).is_some()
                    || self.physics().is_some_and(|a| a.id == *id)
                    || self.pose().is_some_and(|a| a.entity_ids().any(|v| v == id));
                (!valid).then(|| StructureIssue {
                    object_id: id.clone(),
                    status: Status::error(
                        "INVALID_EDITOR_STATE",
                        "Editor state references a missing or incompatible object",
                    ),
                })
            })
            .collect()
    }
}
