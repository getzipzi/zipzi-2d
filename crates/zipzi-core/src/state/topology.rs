use super::*;
use crate::{BlendShapeTargetKind, VertexId};
use std::collections::HashSet;
impl DocumentBuilder {
    pub fn replace_mesh_topology(
        &mut self,
        mesh: Mesh,
        binding: Option<MeshBinding>,
        blend_bindings: Vec<BlendShapeBinding>,
        glues: Vec<Glue>,
        vertex_mapping: HashMap<VertexId, Option<VertexId>>,
    ) -> Result<(), Status> {
        self.write(|this| {
            let model = this.content.model();
            model.authoring_unlocked(&mesh.id)?;
            model.authoring_unlocked(&mesh.part_id)?;
            let Some(previous) = model.get_mesh(&mesh.id) else {
                return Err(Status::error("MISSING_MESH", &mesh.id));
            };
            let old_ids: HashSet<_> = previous.vertex_ids.iter().copied().collect();
            let new_ids: HashSet<_> = mesh.vertex_ids.iter().copied().collect();
            let mapped: Vec<_> = vertex_mapping.values().flatten().copied().collect();
            if vertex_mapping.keys().copied().collect::<HashSet<_>>() != old_ids
                || mapped.iter().any(|v| !new_ids.contains(v))
                || mapped.iter().copied().collect::<HashSet<_>>().len() != mapped.len()
            {
                return Err(Status::error("INVALID_VERTEX_MAPPING", &mesh.id));
            }
            if model.binding_for_mesh(&mesh.id).map(|b| b.id.as_str())
                != binding.as_ref().map(|b| b.id.as_str())
                || binding.as_ref().is_some_and(|b| b.mesh_id != mesh.id)
            {
                return Err(Status::error(
                    "INVALID_BINDING",
                    "Supply the existing ordinary binding",
                ));
            }
            let expected_blends: HashSet<_> = model
                .content
                .blend_bindings
                .values()
                .filter(|b| b.target_id == mesh.id)
                .map(|b| b.id.clone())
                .collect();
            let expected_glues: HashSet<_> = model
                .glues_for_mesh(&mesh.id)
                .iter()
                .map(|g| g.id.clone())
                .collect();
            if blend_bindings
                .iter()
                .map(|b| b.id.clone())
                .collect::<HashSet<_>>()
                != expected_blends
                || blend_bindings.len() != expected_blends.len()
                || blend_bindings
                    .iter()
                    .any(|b| b.target_id != mesh.id || b.target_kind != BlendShapeTargetKind::Mesh)
                || glues.iter().map(|g| g.id.clone()).collect::<HashSet<_>>() != expected_glues
                || glues.len() != expected_glues.len()
            {
                return Err(Status::error("INCOMPLETE_DEPENDENCIES", &mesh.id));
            }
            // Carrying an equal dependency through a complete replacement is a read,
            // not a write. Prove field equality here, never trust a caller flag.
            let changed_dependencies: Vec<_> = binding
                .iter()
                .filter(|b| model.get_binding(&b.id) != Some(*b))
                .map(|b| b.id.clone())
                .chain(
                    blend_bindings
                        .iter()
                        .filter(|b| model.get_blend_binding(&b.id) != Some(*b))
                        .map(|b| b.id.clone()),
                )
                .chain(
                    glues
                        .iter()
                        .filter(|g| model.get_glue(&g.id) != Some(*g))
                        .map(|g| g.id.clone()),
                )
                .collect();
            for id in &changed_dependencies {
                if let Err(status) = model.authoring_unlocked(id) {
                    return Err(status);
                }
            }

            this.replace_mesh(mesh)?;
            if let Some(mut binding) = binding {
                this.canonicalize_binding(&mut binding)?;
                this.replace_binding(binding)?;
            }
            for binding in blend_bindings {
                this.replace_blend_binding(binding)?;
            }
            for glue in glues {
                this.replace_glue(glue)?;
            }
            this.validate_candidate()
        })
    }
}
