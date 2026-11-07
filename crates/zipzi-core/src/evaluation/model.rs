//! Read-only inputs to the shared evaluation algorithms.
use crate::document::SceneProjection;
use crate::draw_order::DrawOrderGroup;
use crate::*;
use std::sync::Arc;
/// Geometry and availability input. Evaluation never mutates authoring content.
pub trait EvaluationModel {
    fn get_parameter(&self, id: &str) -> Option<&Parameter>;
    fn get_part(&self, id: &str) -> Option<&Part>;
    fn get_transform(&self, id: &str) -> Option<&Transform>;
    fn get_mesh(&self, id: &str) -> Option<&Mesh>;
    fn get_binding(&self, id: &str) -> Option<&MeshBinding>;
    fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding>;
    fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding>;
    fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable>;
    fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint>;
    fn get_glue(&self, id: &str) -> Option<&Glue>;
    fn get_offscreen(&self, id: &str) -> Option<&Offscreen>;
    fn asset_order(&self) -> &[String];
    fn parameter_order(&self) -> &[String];
    fn part_order(&self) -> &[String];
    fn transform_order(&self) -> &[String];
    fn mesh_order(&self) -> &[String];
    fn scene_binding_order(&self) -> &[String];
    fn blend_binding_order(&self) -> &[String];
    fn glue_order(&self) -> &[String];
    fn offscreen_order(&self) -> &[String];
    fn binding_for_mesh(&self, id: &str) -> Option<&MeshBinding>;
    fn binding_for_scene(&self, id: &str) -> Option<&SceneBinding>;
    fn offscreen_for_part(&self, id: &str) -> Option<&Offscreen>;
    fn parent_offscreen_for_part(&self, id: &str) -> Option<&Offscreen>;
    fn has_image_content(&self, id: &str) -> bool;
    fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]>;
    fn sorted_parts(&self) -> Vec<String>;
    fn sorted_transforms(&self) -> Vec<String>;
    fn prepared_evaluation(&self) -> Result<&super::PreparedEvaluation, &Status>;
    fn scene_projection(&self) -> Arc<SceneProjection>;
    /// The immutable model has no session revision. A host may stamp its sourced result.
    fn evaluation_revision(&self) -> u64 {
        0
    }
    fn initialized(&self) -> bool {
        true
    }
    fn vertex_slot(&self, mesh: &str, vertex: VertexId) -> Option<usize> {
        self.get_mesh(mesh)?
            .vertex_ids
            .iter()
            .position(|id| *id == vertex)
    }
    fn render_indices_into(&self, id: &str, out: &mut Vec<u32>) -> Result<(), Status> {
        let mesh = self
            .get_mesh(id)
            .ok_or_else(|| Status::error("MISSING_MESH", id))?;
        let slots: std::collections::HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        out.clear();
        out.reserve(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for vertex in triangle {
                out.push(
                    *slots
                        .get(vertex)
                        .ok_or_else(|| Status::error("MISSING_VERTEX", id))?
                        as u32,
                );
            }
        }
        Ok(())
    }
    fn coordinate_space(&self, parent: &str) -> Result<CoordinateSpace, Status> {
        if parent.is_empty() {
            return Ok(CoordinateSpace::Model);
        }
        let t = self
            .get_transform(parent)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", parent))?;
        Ok(if t.warp().is_some() {
            CoordinateSpace::WarpParameter
        } else {
            CoordinateSpace::RotationLocal
        })
    }
    fn is_part_ancestor(&self, ancestor: &str, part: &str) -> bool {
        if ancestor == part {
            return true;
        }
        let mut next = part;
        while let Some(p) = self.get_part(next) {
            if p.parent_id.is_empty() {
                break;
            }
            if p.parent_id == ancestor {
                return true;
            }
            next = &p.parent_id;
        }
        false
    }
    fn blend_binding_active(&self, b: &BlendShapeBinding) -> bool {
        self.get_blend_key_table(&b.key_table_id)
            .is_some_and(|t| !t.parameter_id.is_empty() && !t.keys.is_empty())
            && b.constraint_ids.iter().all(|id| {
                self.get_blend_constraint(id)
                    .is_some_and(|c| !c.parameter_id.is_empty() && !c.keys.is_empty())
            })
    }
    fn iter_active_blend_bindings_for_target<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Iterator<Item = &'a BlendShapeBinding> {
        self.blend_binding_order()
            .iter()
            .filter_map(|key| self.get_blend_binding(key))
            .filter(move |b| b.target_id == id && self.blend_binding_active(b))
    }
}
impl EvaluationModel for crate::model_validation::Model<'_> {
    fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.get_parameter(id)
    }
    fn get_part(&self, id: &str) -> Option<&Part> {
        self.get_part(id)
    }
    fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.get_transform(id)
    }
    fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.get_mesh(id)
    }
    fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.get_binding(id)
    }
    fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.get_scene_binding(id)
    }
    fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.get_blend_binding(id)
    }
    fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.get_blend_key_table(id)
    }
    fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.get_blend_constraint(id)
    }
    fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.get_glue(id)
    }
    fn get_offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.get_offscreen(id)
    }
    fn asset_order(&self) -> &[String] {
        self.asset_order()
    }
    fn parameter_order(&self) -> &[String] {
        self.parameter_order()
    }
    fn part_order(&self) -> &[String] {
        self.part_order()
    }
    fn transform_order(&self) -> &[String] {
        self.transform_order()
    }
    fn mesh_order(&self) -> &[String] {
        self.mesh_order()
    }
    fn scene_binding_order(&self) -> &[String] {
        self.scene_binding_order()
    }
    fn blend_binding_order(&self) -> &[String] {
        self.blend_binding_order()
    }
    fn glue_order(&self) -> &[String] {
        self.glue_order()
    }
    fn offscreen_order(&self) -> &[String] {
        self.offscreen_order()
    }
    fn binding_for_mesh(&self, id: &str) -> Option<&MeshBinding> {
        self.binding_for_mesh(id)
    }
    fn binding_for_scene(&self, id: &str) -> Option<&SceneBinding> {
        self.binding_for_scene(id)
    }
    fn offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.offscreen_for_part(id)
    }
    fn parent_offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.parent_offscreen_for_part(id)
    }
    fn has_image_content(&self, id: &str) -> bool {
        self.has_image_content(id)
    }
    fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.draw_order_groups()
    }
    fn sorted_parts(&self) -> Vec<String> {
        self.sorted_parts()
    }
    fn sorted_transforms(&self) -> Vec<String> {
        self.sorted_transforms()
    }
    fn prepared_evaluation(&self) -> Result<&super::PreparedEvaluation, &Status> {
        self.prepared_evaluation()
    }
    fn scene_projection(&self) -> Arc<SceneProjection> {
        self.scene_projection()
    }
}
impl EvaluationModel for crate::state::DocumentState {
    fn iter_active_blend_bindings_for_target<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Iterator<Item = &'a BlendShapeBinding> {
        self.blend_binding_ids_for_target(id)
            .iter()
            .map(|key| self.blend_binding(key).unwrap())
            .filter(|binding| self.blend_binding_active(binding))
    }
    fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.parameter(id)
    }
    fn get_part(&self, id: &str) -> Option<&Part> {
        self.part(id)
    }
    fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.transform(id)
    }
    fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.mesh(id)
    }
    fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.binding(id)
    }
    fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.scene_binding(id)
    }
    fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.blend_binding(id)
    }
    fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.blend_key_table(id)
    }
    fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.blend_constraint(id)
    }
    fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.glue(id)
    }
    fn get_offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.offscreen(id)
    }
    fn asset_order(&self) -> &[String] {
        self.asset_order()
    }
    fn parameter_order(&self) -> &[String] {
        self.parameter_order()
    }
    fn part_order(&self) -> &[String] {
        self.part_order()
    }
    fn transform_order(&self) -> &[String] {
        self.transform_order()
    }
    fn mesh_order(&self) -> &[String] {
        self.mesh_order()
    }
    fn scene_binding_order(&self) -> &[String] {
        self.scene_binding_order()
    }
    fn blend_binding_order(&self) -> &[String] {
        self.blend_binding_order()
    }
    fn glue_order(&self) -> &[String] {
        self.glue_order()
    }
    fn offscreen_order(&self) -> &[String] {
        self.offscreen_order()
    }
    fn binding_for_mesh(&self, id: &str) -> Option<&MeshBinding> {
        self.binding_for_mesh(id)
    }
    fn binding_for_scene(&self, id: &str) -> Option<&SceneBinding> {
        self.binding_for_scene(id)
    }
    fn offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.offscreen_for_part(id)
    }
    fn parent_offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.parent_offscreen_for_part(id)
    }
    fn has_image_content(&self, id: &str) -> bool {
        self.has_image_content(id)
    }
    fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.draw_order_groups().as_deref()
    }
    fn sorted_parts(&self) -> Vec<String> {
        self.sorted_parts()
    }
    fn sorted_transforms(&self) -> Vec<String> {
        self.sorted_transforms()
    }
    fn prepared_evaluation(&self) -> Result<&super::PreparedEvaluation, &Status> {
        self.prepared_evaluation()
    }
    fn scene_projection(&self) -> Arc<SceneProjection> {
        self.scene_projection()
    }
}

impl<T: EvaluationModel> EvaluationModel for Arc<T> {
    fn iter_active_blend_bindings_for_target<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Iterator<Item = &'a BlendShapeBinding> {
        self.as_ref().iter_active_blend_bindings_for_target(id)
    }
    fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.as_ref().get_parameter(id)
    }
    fn get_part(&self, id: &str) -> Option<&Part> {
        self.as_ref().get_part(id)
    }
    fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.as_ref().get_transform(id)
    }
    fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.as_ref().get_mesh(id)
    }
    fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.as_ref().get_binding(id)
    }
    fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.as_ref().get_scene_binding(id)
    }
    fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.as_ref().get_blend_binding(id)
    }
    fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.as_ref().get_blend_key_table(id)
    }
    fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.as_ref().get_blend_constraint(id)
    }
    fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.as_ref().get_glue(id)
    }
    fn get_offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.as_ref().get_offscreen(id)
    }
    fn asset_order(&self) -> &[String] {
        self.as_ref().asset_order()
    }
    fn parameter_order(&self) -> &[String] {
        self.as_ref().parameter_order()
    }
    fn part_order(&self) -> &[String] {
        self.as_ref().part_order()
    }
    fn transform_order(&self) -> &[String] {
        self.as_ref().transform_order()
    }
    fn mesh_order(&self) -> &[String] {
        self.as_ref().mesh_order()
    }
    fn scene_binding_order(&self) -> &[String] {
        self.as_ref().scene_binding_order()
    }
    fn blend_binding_order(&self) -> &[String] {
        self.as_ref().blend_binding_order()
    }
    fn glue_order(&self) -> &[String] {
        self.as_ref().glue_order()
    }
    fn offscreen_order(&self) -> &[String] {
        self.as_ref().offscreen_order()
    }
    fn binding_for_mesh(&self, id: &str) -> Option<&MeshBinding> {
        self.as_ref().binding_for_mesh(id)
    }
    fn binding_for_scene(&self, id: &str) -> Option<&SceneBinding> {
        self.as_ref().binding_for_scene(id)
    }
    fn offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.as_ref().offscreen_for_part(id)
    }
    fn parent_offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.as_ref().parent_offscreen_for_part(id)
    }
    fn has_image_content(&self, id: &str) -> bool {
        self.as_ref().has_image_content(id)
    }
    fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.as_ref().draw_order_groups()
    }
    fn sorted_parts(&self) -> Vec<String> {
        self.as_ref().sorted_parts()
    }
    fn sorted_transforms(&self) -> Vec<String> {
        self.as_ref().sorted_transforms()
    }
    fn prepared_evaluation(&self) -> Result<&super::PreparedEvaluation, &Status> {
        self.as_ref().prepared_evaluation()
    }
    fn scene_projection(&self) -> Arc<SceneProjection> {
        self.as_ref().scene_projection()
    }
    fn evaluation_revision(&self) -> u64 {
        self.as_ref().evaluation_revision()
    }
    fn initialized(&self) -> bool {
        self.as_ref().initialized()
    }
    fn vertex_slot(&self, mesh: &str, vertex: VertexId) -> Option<usize> {
        self.as_ref().vertex_slot(mesh, vertex)
    }
}

impl<T: EvaluationModel + Clone> EvaluationModel for std::borrow::Cow<'_, T> {
    fn iter_active_blend_bindings_for_target<'a>(
        &'a self,
        id: &'a str,
    ) -> impl Iterator<Item = &'a BlendShapeBinding> {
        self.as_ref().iter_active_blend_bindings_for_target(id)
    }
    fn get_parameter(&self, id: &str) -> Option<&Parameter> {
        self.as_ref().get_parameter(id)
    }
    fn get_part(&self, id: &str) -> Option<&Part> {
        self.as_ref().get_part(id)
    }
    fn get_transform(&self, id: &str) -> Option<&Transform> {
        self.as_ref().get_transform(id)
    }
    fn get_mesh(&self, id: &str) -> Option<&Mesh> {
        self.as_ref().get_mesh(id)
    }
    fn get_binding(&self, id: &str) -> Option<&MeshBinding> {
        self.as_ref().get_binding(id)
    }
    fn get_scene_binding(&self, id: &str) -> Option<&SceneBinding> {
        self.as_ref().get_scene_binding(id)
    }
    fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.as_ref().get_blend_binding(id)
    }
    fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.as_ref().get_blend_key_table(id)
    }
    fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.as_ref().get_blend_constraint(id)
    }
    fn get_glue(&self, id: &str) -> Option<&Glue> {
        self.as_ref().get_glue(id)
    }
    fn get_offscreen(&self, id: &str) -> Option<&Offscreen> {
        self.as_ref().get_offscreen(id)
    }
    fn asset_order(&self) -> &[String] {
        self.as_ref().asset_order()
    }
    fn parameter_order(&self) -> &[String] {
        self.as_ref().parameter_order()
    }
    fn part_order(&self) -> &[String] {
        self.as_ref().part_order()
    }
    fn transform_order(&self) -> &[String] {
        self.as_ref().transform_order()
    }
    fn mesh_order(&self) -> &[String] {
        self.as_ref().mesh_order()
    }
    fn scene_binding_order(&self) -> &[String] {
        self.as_ref().scene_binding_order()
    }
    fn blend_binding_order(&self) -> &[String] {
        self.as_ref().blend_binding_order()
    }
    fn glue_order(&self) -> &[String] {
        self.as_ref().glue_order()
    }
    fn offscreen_order(&self) -> &[String] {
        self.as_ref().offscreen_order()
    }
    fn binding_for_mesh(&self, id: &str) -> Option<&MeshBinding> {
        self.as_ref().binding_for_mesh(id)
    }
    fn binding_for_scene(&self, id: &str) -> Option<&SceneBinding> {
        self.as_ref().binding_for_scene(id)
    }
    fn offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.as_ref().offscreen_for_part(id)
    }
    fn parent_offscreen_for_part(&self, id: &str) -> Option<&Offscreen> {
        self.as_ref().parent_offscreen_for_part(id)
    }
    fn has_image_content(&self, id: &str) -> bool {
        self.as_ref().has_image_content(id)
    }
    fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.as_ref().draw_order_groups()
    }
    fn sorted_parts(&self) -> Vec<String> {
        self.as_ref().sorted_parts()
    }
    fn sorted_transforms(&self) -> Vec<String> {
        self.as_ref().sorted_transforms()
    }
    fn prepared_evaluation(&self) -> Result<&super::PreparedEvaluation, &Status> {
        self.as_ref().prepared_evaluation()
    }
    fn scene_projection(&self) -> Arc<SceneProjection> {
        self.as_ref().scene_projection()
    }
    fn evaluation_revision(&self) -> u64 {
        self.as_ref().evaluation_revision()
    }
    fn initialized(&self) -> bool {
        self.as_ref().initialized()
    }
    fn vertex_slot(&self, mesh: &str, vertex: VertexId) -> Option<usize> {
        self.as_ref().vertex_slot(mesh, vertex)
    }
}
