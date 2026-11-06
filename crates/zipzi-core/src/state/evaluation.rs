use super::*;
use std::sync::OnceLock;
#[derive(Debug, Default)]
pub(super) struct StateReadCache {
    prepared: OnceLock<Result<crate::evaluation::PreparedEvaluation, Status>>,
    projection: OnceLock<Arc<SceneProjection>>,
}
impl DocumentState {
    pub fn binding_for_mesh(&self, id: &str) -> Option<&MeshBinding> {
        self.content
            .bindings
            .shared_map()
            .values()
            .map(Arc::as_ref)
            .find(|b| b.mesh_id == id)
    }
    pub fn binding_for_scene(&self, id: &str) -> Option<&SceneBinding> {
        self.content
            .scene_bindings
            .shared_map()
            .values()
            .map(Arc::as_ref)
            .find(|b| b.target_id() == id)
    }
    pub fn offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.content
            .offscreens
            .shared_map()
            .values()
            .map(Arc::as_ref)
            .find(|o| o.part_id == id)
    }
    pub fn parent_offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        let mut next = id;
        while let Some(p) = self.part(next) {
            if p.parent_id.is_empty() {
                break;
            }
            if let Some(o) = self.offscreen_for_part(&p.parent_id) {
                return Some(o);
            }
            next = &p.parent_id;
        }
        None
    }
    pub fn has_image_content(&self, id: &str) -> bool {
        self.asset(id)
            .is_some_and(|a| matches!(a.content, AssetContent::Image { .. }))
    }
    pub fn sorted_parts(&self) -> Vec<String> {
        self.content.model().sorted_parts()
    }
    pub fn sorted_transforms(&self) -> Vec<String> {
        self.content.model().sorted_transforms()
    }
    pub(crate) fn prepared_evaluation(
        &self,
    ) -> Result<&crate::evaluation::PreparedEvaluation, &Status> {
        self.read_cache
            .prepared
            .get_or_init(|| crate::evaluation::PreparedEvaluation::new(self))
            .as_ref()
    }
    pub fn scene_projection(&self) -> Arc<SceneProjection> {
        self.read_cache
            .projection
            .get_or_init(|| Arc::new(crate::evaluation::build_scene_projection(self)))
            .clone()
    }
}

impl DocumentState {
    pub fn mesh_dependency_bytes(&self, id: &str) -> usize {
        super::cost::scene_payload(&self.content.model(), &[id.to_owned()].into())
    }
}

impl DocumentState {
    pub fn scene_export_omissions(&self) -> Vec<DisabledSceneItem> {
        let projection = self.scene_projection();
        // Empty geometry is representable in foreign scene formats (including
        // helper meshes used by Glue). Export readiness is stricter only for
        // missing configuration and consumers that depend on it.
        let pending: std::collections::HashSet<_> = projection
            .disabled
            .iter()
            .filter(|v| {
                matches!(
                    v.reason,
                    SceneDisabledReason::UnspecifiedReference
                        | SceneDisabledReason::MissingImageContent { .. }
                        | SceneDisabledReason::UnresolvedReference { .. }
                )
            })
            .map(|v| v.id.as_str())
            .collect();
        let mut out: Vec<_> = projection
            .disabled
            .iter()
            .filter(|v| v.chain.iter().any(|id| pending.contains(id.as_str())))
            .cloned()
            .collect();
        let omitted: std::collections::HashSet<_> = out.iter().map(|v| v.id.clone()).collect();
        for id in &projection.glues {
            let g = self.glue(id).unwrap();
            if let Some(source) = [&g.mesh_a_id, &g.mesh_b_id]
                .iter()
                .filter_map(|r| r.resolved_id())
                .find(|id| omitted.contains(*id))
            {
                out.push(DisabledSceneItem {
                    id: id.clone(),
                    usage: SceneUse::Glue,
                    reason: SceneDisabledReason::DependencyUnavailable {
                        object_id: source.into(),
                    },
                    chain: vec![id.clone(), source.into()],
                });
            }
        }
        out
    }
    pub fn binding_export_omissions(&self) -> Vec<BindingOmission> {
        self.content.model().binding_export_omissions()
    }
}

impl DocumentState {
    pub fn scene_payload_bytes(&self, ids: &[String]) -> usize {
        super::cost::scene_payload(&self.content.model(), &ids.iter().cloned().collect())
    }
    pub fn authoring_rest_projection(&self) -> Result<Self, Status> {
        let mut builder = self.builder();
        for id in self.blend_binding_order() {
            builder.remove_blend_binding(id)?;
        }
        for id in self.glue_order() {
            builder.remove_glue(id)?;
        }
        let mut hierarchy = builder.hierarchy_order().clone();
        hierarchy.glues.clear();
        builder.set_hierarchy_order(hierarchy)?;
        let mut editor = builder.editor_state().clone();
        editor
            .objects
            .retain(|id, _| builder.content.model().contains_id(id));
        builder.set_editor_state(editor)?;
        Ok(builder.freeze()?.state)
    }
}

impl DocumentState {
    pub fn available_binding_export(&self) -> Result<Self, Status> {
        let mut b = self.builder();
        for id in self.binding_order() {
            let mut value = self.binding(id).unwrap().clone();
            value.axes.retain(|a| !a.parameter_id.is_empty());
            if value.axes.is_empty() {
                b.remove_binding(id)?;
            } else {
                b.replace_binding(value)?;
            }
        }
        for id in self.scene_binding_order() {
            let mut value = self.scene_binding(id).unwrap().clone();
            value.axes.retain(|a| !a.parameter_id.is_empty());
            if value.axes.is_empty() {
                b.remove_scene_binding(id)?;
            } else {
                b.replace_scene_binding(value)?;
            }
        }
        for id in self.glue_order() {
            let mut g = self.glue(id).unwrap().clone();
            if let Some(binding) = &mut g.binding {
                binding.axes.retain(|a| !a.parameter_id.is_empty());
                if binding.axes.is_empty() {
                    g.binding = None;
                }
            }
            b.replace_glue(g)?;
        }
        let model = self.content.model();
        for id in self.blend_binding_order() {
            if !model.blend_binding_active(self.blend_binding(id).unwrap()) {
                b.remove_blend_binding(id)?;
            }
        }
        for id in self.blend_key_table_order() {
            if self.blend_key_table(id).unwrap().parameter_id.is_empty() {
                b.remove_blend_key_table(id)?;
            }
        }
        for id in self.blend_constraint_order() {
            if self.blend_constraint(id).unwrap().parameter_id.is_empty() {
                b.remove_blend_constraint(id)?;
            }
        }
        let mut editor = b.editor_state().clone();
        editor
            .objects
            .retain(|id, _| b.content.model().contains_id(id));
        b.set_editor_state(editor)?;
        Ok(b.freeze()?.state)
    }
}
