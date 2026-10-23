//! Borrowed comparisons used by publication. These describe persistent data,
//! never the operation name or a caller's declared change category.
use super::*;
use crate::BindingAxis;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentDomain {
    Document,
    Assets,
    Scene,
    Bindings,
    Parameters,
    Organization,
    Expressions,
    Motions,
    Pose,
    Physics,
    References,
    EditorState,
}

impl Document {
    /// Whether all persistent inputs consumed by a domain's change summary agree.
    /// Cross-domain summaries deliberately include their secondary inputs here.
    pub fn same_domain(&self, other: &Self, domain: DocumentDomain) -> bool {
        let a = &self.content;
        let b = &other.content;
        match domain {
            DocumentDomain::Document => {
                a.id == b.id
                    && a.export_settings == b.export_settings
                    && a.source_canvas == b.source_canvas
                    && a.missing_attachments == b.missing_attachments
            }
            DocumentDomain::Assets => a.assets == b.assets && a.asset_order == b.asset_order,
            DocumentDomain::Scene => {
                a.parts == b.parts
                    && a.part_order == b.part_order
                    && a.transforms == b.transforms
                    && a.transform_order == b.transform_order
                    && a.meshes == b.meshes
                    && a.mesh_order == b.mesh_order
                    && a.glues == b.glues
                    && a.glue_order == b.glue_order
                    && a.offscreens == b.offscreens
                    && a.offscreen_order == b.offscreen_order
                    && a.draw_order_groups == b.draw_order_groups
                    && a.hierarchy_order.deformation == b.hierarchy_order.deformation
            }
            DocumentDomain::Bindings => {
                a.bindings == b.bindings
                    && a.binding_order == b.binding_order
                    && a.scene_bindings == b.scene_bindings
                    && a.scene_binding_order == b.scene_binding_order
                    && a.blend_key_tables == b.blend_key_tables
                    && a.blend_key_table_order == b.blend_key_table_order
                    && a.blend_constraints == b.blend_constraints
                    && a.blend_constraint_order == b.blend_constraint_order
                    && a.blend_bindings == b.blend_bindings
                    && a.blend_binding_order == b.blend_binding_order
                    && a.glues == b.glues
            }
            DocumentDomain::Parameters => {
                a.parameters == b.parameters && a.parameter_order == b.parameter_order
            }
            DocumentDomain::Organization => {
                a.parameters == b.parameters
                    && a.parameter_order == b.parameter_order
                    && a.parts == b.parts
                    && a.part_order == b.part_order
                    && a.display_info == b.display_info
                    && a.hierarchy_order.organization == b.hierarchy_order.organization
                    && a.draw_order_groups == b.draw_order_groups
                    && a.transform_order == b.transform_order
                    && a.mesh_order == b.mesh_order
                    && a.offscreen_order == b.offscreen_order
                    && same_records(&a.transforms, &b.transforms, |a, b| a.part() == b.part())
                    && same_records(&a.meshes, &b.meshes, |a, b| a.part_id == b.part_id)
                    && same_records(&a.offscreens, &b.offscreens, |a, b| a.part_id == b.part_id)
            }
            DocumentDomain::Expressions => {
                a.expressions == b.expressions && a.expression_order == b.expression_order
            }
            DocumentDomain::Motions => {
                a.motions == b.motions
                    && a.motion_order == b.motion_order
                    && a.motion_groups == b.motion_groups
                    && a.motion_roots == b.motion_roots
                    && a.package_attachments == b.package_attachments
                    && a.model3_settings.user_data == b.model3_settings.user_data
            }
            DocumentDomain::Pose => a.pose == b.pose,
            DocumentDomain::Physics => a.physics == b.physics,
            DocumentDomain::References => {
                a.model3_settings == b.model3_settings
                    && a.display_info == b.display_info
                    && a.mesh_order == b.mesh_order
                    && a.offscreen_order == b.offscreen_order
                    && a.blend_binding_order == b.blend_binding_order
                    && same_records(&a.meshes, &b.meshes, |a, b| a.masks == b.masks)
                    && same_records(&a.offscreens, &b.offscreens, |a, b| a.masks == b.masks)
                    && same_records(&a.blend_bindings, &b.blend_bindings, |a, b| {
                        a.constraint_ids == b.constraint_ids
                    })
            }
            DocumentDomain::EditorState => a.editor_state == b.editor_state,
        }
    }

    /// A conservative, allocation-free proof that the global authoring key set
    /// is unchanged. Changes inside an owned collection may fall back to a full
    /// enumeration even if their particular IDs happen to agree.
    pub fn same_authoring_identities(&self, other: &Self) -> bool {
        let a = &self.content;
        let b = &other.content;
        a.id == b.id
            && same_keys(&a.assets, &b.assets)
            && same_keys(&a.parts, &b.parts)
            && same_keys(&a.transforms, &b.transforms)
            && same_keys(&a.meshes, &b.meshes)
            && same_keys(&a.parameters, &b.parameters)
            && same_records(&a.bindings, &b.bindings, |a, b| {
                same_axis_ids(&a.axes, &b.axes)
            })
            && same_records(&a.scene_bindings, &b.scene_bindings, |a, b| {
                same_axis_ids(&a.axes, &b.axes)
            })
            && same_records(&a.blend_key_tables, &b.blend_key_tables, |a, b| {
                a.samples
                    .iter()
                    .map(|s| &s.id)
                    .eq(b.samples.iter().map(|s| &s.id))
            })
            && same_records(&a.blend_constraints, &b.blend_constraints, |a, b| {
                a.samples
                    .iter()
                    .map(|s| &s.id)
                    .eq(b.samples.iter().map(|s| &s.id))
            })
            && same_keys(&a.blend_bindings, &b.blend_bindings)
            && same_records(&a.glues, &b.glues, |a, b| {
                a.pairs
                    .iter()
                    .map(|p| &p.id)
                    .eq(b.pairs.iter().map(|p| &p.id))
                    && match (&a.binding, &b.binding) {
                        (Some(a), Some(b)) => same_axis_ids(&a.axes, &b.axes),
                        (None, None) => true,
                        _ => false,
                    }
            })
            && a.offscreens == b.offscreens
            && self.same_domain(other, DocumentDomain::Expressions)
            && self.same_domain(other, DocumentDomain::Motions)
            && self.same_domain(other, DocumentDomain::Pose)
            && self.same_domain(other, DocumentDomain::Physics)
            && self.same_domain(other, DocumentDomain::References)
            && a.draw_order_groups == b.draw_order_groups
    }

    pub(super) fn same_binding_validation_dependencies(&self, other: &Self) -> bool {
        // Stable ownership prevents a changed binding from introducing conflicts
        // into an unchanged binding. Key tables/constraints are shared inputs.
        same_records(&self.content.bindings, &other.content.bindings, |a, b| {
            a.mesh_id == b.mesh_id
        }) && same_records(
            &self.content.scene_bindings,
            &other.content.scene_bindings,
            |a, b| a.target_id() == b.target_id(),
        ) && self.content.blend_key_tables == other.content.blend_key_tables
            && self.content.blend_constraints == other.content.blend_constraints
            && self.content.glues == other.content.glues
            && self.same_domain(other, DocumentDomain::Parameters)
            && self.content.parts == other.content.parts
            && self.content.transforms == other.content.transforms
            && self.content.offscreens == other.content.offscreens
            && same_records(&self.content.meshes, &other.content.meshes, |a, b| {
                a.part_id == b.part_id
                    && a.deformer_id == b.deformer_id
                    && a.vertex_ids == b.vertex_ids
                    && a.base_positions == b.base_positions
                    && a.triangles == b.triangles
                    && a.uvs == b.uvs
            })
    }
}

fn same_axis_ids(a: &[BindingAxis], b: &[BindingAxis]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.id == b.id
                && a.samples
                    .iter()
                    .map(|s| &s.id)
                    .eq(b.samples.iter().map(|s| &s.id))
        })
}

fn same_keys<T>(a: &HashMap<String, T>, b: &HashMap<String, T>) -> bool {
    a.len() == b.len() && a.keys().all(|id| b.contains_key(id))
}
fn same_records<T>(
    a: &HashMap<String, T>,
    b: &HashMap<String, T>,
    same: impl Fn(&T, &T) -> bool,
) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(id, a)| b.get(id).is_some_and(|b| same(a, b)))
}
