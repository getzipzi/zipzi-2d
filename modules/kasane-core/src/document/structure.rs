//! Read-only validation of the complete persistent document graph.
use super::*;
use crate::draw_order::validate_groups;
use crate::geometry::validate_render_mesh;
use std::collections::{HashMap, HashSet};

/// A proof is valid only for this exact snapshot. Cloning a document deliberately
/// drops it: internal planners may edit a clone's raw content before validation.
#[derive(Debug, Default)]
pub(super) struct ValidationCache(std::sync::Mutex<Option<Arc<()>>>);
impl Clone for ValidationCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureIssue {
    pub object_id: String,
    pub status: Status,
}

impl Document {
    /// Validate all persistent collections and references without reading resources
    /// or changing this document. Restored checkpoints use the same checks.
    pub fn validate_structure(&self) -> Vec<StructureIssue> {
        self.validate_structure_impl(None, false)
    }

    /// Reuse only checks whose persistent data AND cross-domain dependencies
    /// agree with a fully validated baseline. Invalid baselines remain repairable.
    pub fn validate_structure_since(&self, baseline: &Document) -> Vec<StructureIssue> {
        let valid = baseline.structure_is_validated() || baseline.validate_structure().is_empty();
        let identities = valid && self.same_authoring_identities(baseline);
        self.validate_structure_impl(
            (identities && self.same_binding_validation_dependencies(baseline)).then_some(baseline),
            identities
                && self.same_domain(baseline, super::DocumentDomain::Motions)
                && self.same_domain(baseline, super::DocumentDomain::Parameters),
        )
    }

    pub(super) fn structure_is_validated(&self) -> bool {
        self.validated_structure
            .0
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|id| Arc::ptr_eq(id, &self.snapshot_identity))
    }
    pub(super) fn remember_valid_structure(&self) {
        *self.validated_structure.0.lock().unwrap() = Some(self.snapshot_identity.clone());
    }
    pub(super) fn forget_valid_structure(&self) {
        *self.validated_structure.0.lock().unwrap() = None;
    }

    fn validate_structure_impl(
        &self,
        binding_baseline: Option<&Document>,
        reuse_motions: bool,
    ) -> Vec<StructureIssue> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.validate_structure");
        let mut issues = Vec::new();
        let mut initialized = Document::new();
        let status = initialized.initialize(self.content.id.clone(), self.model_setup());
        if !status.is_ok() {
            issues.push(StructureIssue {
                object_id: self.content.id.clone(),
                status,
            });
        }

        let mut all_ids = HashSet::new();
        all_ids.insert(self.content.id.clone());
        macro_rules! check_collection {
            ($map:ident, $order:ident) => {{
                let mut ordered = HashSet::new();
                for id in &self.content.$order {
                    if !ordered.insert(id.clone()) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("DUPLICATE_ORDER_ID", "Repeated object in order"),
                        });
                    }
                    if !self.content.$map.contains_key(id) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error(
                                "MISSING_OBJECT",
                                "Order references absent object",
                            ),
                        });
                    }
                }
                for (id, object) in &self.content.$map {
                    if !ordered.contains(id) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("UNORDERED_OBJECT", "Object missing from order"),
                        });
                    }
                    if object.id != *id || !valid_uuid(id) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("INVALID_ID", "Object ID or map key is invalid"),
                        });
                    }
                    if !all_ids.insert(id.clone()) {
                        issues.push(StructureIssue {
                            object_id: id.clone(),
                            status: Status::error("DUPLICATE_ID", "ID is used by another object"),
                        });
                    }
                }
            }};
        }
        check_collection!(assets, asset_order);
        check_collection!(parts, part_order);
        check_collection!(transforms, transform_order);
        check_collection!(meshes, mesh_order);
        check_collection!(parameters, parameter_order);
        check_collection!(bindings, binding_order);
        check_collection!(scene_bindings, scene_binding_order);
        check_collection!(blend_key_tables, blend_key_table_order);
        check_collection!(blend_constraints, blend_constraint_order);
        check_collection!(blend_bindings, blend_binding_order);
        check_collection!(glues, glue_order);
        check_collection!(offscreens, offscreen_order);
        check_collection!(expressions, expression_order);
        check_collection!(motions, motion_order);
        if let Some(pose) = &self.content.pose {
            for id in pose.entity_ids() {
                if !all_ids.insert(id.to_owned()) {
                    issues.push(StructureIssue {
                        object_id: id.to_owned(),
                        status: Status::error("DUPLICATE_ID", "Pose ID is used by another object"),
                    });
                }
            }
        }
        if let Some(physics) = &self.content.physics {
            if !all_ids.insert(physics.id.clone()) {
                issues.push(StructureIssue {
                    object_id: physics.id.clone(),
                    status: Status::error("DUPLICATE_ID", "Physics ID is used by another object"),
                });
            }
        }
        for pair in self.content.glues.values().flat_map(|g| &g.pairs) {
            if !valid_uuid(&pair.id) || !all_ids.insert(pair.id.clone()) {
                issues.push(StructureIssue {
                    object_id: pair.id.clone(),
                    status: Status::error(
                        "DUPLICATE_PAIR_ID",
                        "Missing or duplicated Glue pair identity",
                    ),
                });
            }
        }
        for child in self.binding_identities_iter() {
            if !valid_uuid(&child.id) || !all_ids.insert(child.id.clone()) {
                issues.push(StructureIssue {
                    object_id: child.id.clone(),
                    status: Status::error(
                        "INVALID_BINDING_IDENTITY",
                        "Missing or duplicated axis/sample identity",
                    ),
                });
            }
        }
        for expression in self.content.expressions.values() {
            for entry in &expression.entries {
                if !all_ids.insert(entry.id.clone()) {
                    issues.push(StructureIssue {
                        object_id: entry.id.clone(),
                        status: Status::error(
                            "DUPLICATE_ID",
                            "Expression entry ID is used by another object",
                        ),
                    });
                }
            }
        }
        for motion in self.content.motions.values() {
            for id in motion
                .tracks
                .iter()
                .map(|track| &track.id)
                .chain(motion.events.iter().map(|event| &event.id))
            {
                if !all_ids.insert(id.clone()) {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status: Status::error(
                            "DUPLICATE_ID",
                            "Motion child ID is used by another object",
                        ),
                    });
                }
            }
        }
        for id in self
            .content
            .display_info
            .parameter_groups
            .as_ref()
            .into_iter()
            .flatten()
            .map(|group| &group.id)
            .chain(
                self.content
                    .display_info
                    .combined_parameters
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .map(|set| &set.id),
            )
        {
            if !all_ids.insert(id.clone()) {
                issues.push(StructureIssue {
                    object_id: id.clone(),
                    status: Status::error("DUPLICATE_ID", "CDI identity is used by another object"),
                });
            }
        }
        for child in self
            .reference_identities()
            .iter()
            .filter(|i| i.kind != ReferenceKind::CdiCombinedSet)
        {
            if !valid_uuid(&child.id) || !all_ids.insert(child.id.clone()) {
                issues.push(StructureIssue {
                    object_id: child.id.clone(),
                    status: Status::error(
                        "INVALID_REFERENCE_IDENTITY",
                        "Missing or duplicated metadata identity",
                    ),
                });
            }
        }
        issues.extend(self.validate_display_info());
        issues.extend(self.hierarchy_order_issues(&self.content.hierarchy_order));
        issues.extend(self.editor_state_issues(&self.content.editor_state));
        issues.extend(self.validate_expressions());
        issues.extend(self.validate_model3());
        if !reuse_motions {
            issues.extend(self.validate_motions());
        }
        issues.extend(self.validate_pose_asset());
        issues.extend(self.validate_physics_asset());

        let mut add_issue = |id: &str, status: Status| {
            if !status.is_ok() {
                issues.push(StructureIssue {
                    object_id: id.into(),
                    status,
                });
            }
        };
        for (id, asset) in &self.content.assets {
            if asset
                .unresolved_source
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.contains('\0'))
                || (asset.content.is_some() && asset.unresolved_source.is_some())
            {
                add_issue(id, Status::error("INVALID_ASSET", id));
            }
            if asset
                .content
                .as_ref()
                .is_some_and(|content| !content.is_valid())
            {
                add_issue(id, Status::error("INVALID_ASSET", id));
            }
        }
        for (id, part) in &self.content.parts {
            add_issue(id, self.validate_part(part));
        }
        for (id, transform) in &self.content.transforms {
            add_issue(id, self.validate_transform(transform));
        }
        for (id, mesh) in &self.content.meshes {
            let status = self.validate_mesh_snapshot(mesh);
            add_issue(id, status);
        }
        for (id, parameter) in &self.content.parameters {
            add_issue(id, self.validate_parameter(parameter));
        }
        for (id, binding) in &self.content.bindings {
            if binding_baseline.is_some_and(|b| b.get_binding(id) == Some(binding)) {
                continue;
            }
            add_issue(id, self.canonicalize_binding(&mut binding.clone()));
        }
        for (id, binding) in &self.content.scene_bindings {
            if binding_baseline.is_some_and(|b| b.get_scene_binding(id) == Some(binding)) {
                continue;
            }
            add_issue(id, self.canonicalize_scene_binding(&mut binding.clone()));
        }
        for (id, table) in &self.content.blend_key_tables {
            if binding_baseline.is_some_and(|b| b.get_blend_key_table(id) == Some(table)) {
                continue;
            }
            add_issue(id, self.validate_blend_key_table(table));
        }
        for (id, constraint) in &self.content.blend_constraints {
            if binding_baseline.is_some_and(|b| b.get_blend_constraint(id) == Some(constraint)) {
                continue;
            }
            add_issue(id, self.validate_blend_constraint(constraint));
        }
        for (id, binding) in &self.content.blend_bindings {
            if binding_baseline.is_some_and(|b| b.get_blend_binding(id) == Some(binding)) {
                continue;
            }
            add_issue(id, self.validate_blend_binding(binding));
        }
        for (id, glue) in &self.content.glues {
            let duplicate = self
                .content
                .glues
                .iter()
                .any(|(other_id, other)| other_id != id && other.runtime_id == glue.runtime_id);
            add_issue(
                id,
                if duplicate {
                    Status::error("DUPLICATE_RUNTIME_ID", format!("{}.runtime_id", glue.id))
                } else {
                    self.validate_glue(glue)
                },
            );
        }
        for (id, offscreen) in &self.content.offscreens {
            let duplicate = self.content.offscreens.iter().any(|(other_id, other)| {
                other_id != id && other.runtime_id == offscreen.runtime_id
            });
            add_issue(
                id,
                if duplicate {
                    Status::error(
                        "DUPLICATE_RUNTIME_ID",
                        format!("{}.runtime_id", offscreen.id),
                    )
                } else {
                    self.validate_offscreen(offscreen)
                },
            );
        }
        if let Some(groups) = &self.content.draw_order_groups {
            if let Err(status) = validate_groups(self, groups) {
                issues.push(StructureIssue {
                    object_id: self.content.id.clone(),
                    status,
                });
            }
        }
        issues.sort_by(|a, b| {
            (&a.object_id, &a.status.code, &a.status.message).cmp(&(
                &b.object_id,
                &b.status.code,
                &b.status.message,
            ))
        });
        if issues.is_empty() {
            self.remember_valid_structure();
        } else {
            self.forget_valid_structure();
        }
        issues
    }

    pub(in crate::document) fn validate_mesh_snapshot(&self, mesh: &Mesh) -> Status {
        if !mesh.texture_asset_id.validate().is_ok() {
            return mesh.texture_asset_id.validate();
        }
        if mesh
            .texture_asset_id
            .resolved_id()
            .is_some_and(|id| !self.content.assets.contains_key(id))
        {
            return Status::error("MISSING_ASSET", "Texture asset does not exist.");
        }
        let status = self.validate_mesh_properties(mesh);
        if !status.is_ok() {
            return status;
        }
        if let Some(id) = self.mesh_runtime_ids.get(&mesh.runtime_id) {
            if id != &mesh.id {
                return Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", mesh.id, id),
                );
            }
        }
        if mesh.vertex_ids.len() != mesh.base_positions.len() {
            return Status::error("INVALID_LENGTH", "Vertex IDs must match positions.");
        }
        let slots: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        if slots.len() != mesh.vertex_ids.len() {
            return Status::error(
                "DUPLICATE_VERTEX",
                "Vertex IDs must be unique within a mesh.",
            );
        }
        let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for id in triangle {
                let Some(slot) = slots.get(id) else {
                    return Status::error(
                        "MISSING_VERTEX",
                        "Triangle references an unknown vertex ID.",
                    );
                };
                indices.push(*slot as u32);
            }
        }
        validate_render_mesh(&mesh.base_positions, &mesh.uvs, &indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, Vec2};

    const DOC: &str = "00000000-0000-4000-8000-000000000701";
    const PART: &str = "00000000-0000-4000-8000-000000000702";

    #[test]
    fn validation_proofs_do_not_follow_mutated_clones_or_checkpoint_branches() {
        let mut source = Document::new();
        assert!(source
            .initialize(DOC, Canvas::new(100., 100., Vec2::default(), 10.))
            .is_ok());
        assert!(source
            .create_part(Part {
                id: PART.into(),
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(source.validate_structure().is_empty());
        assert!(source.structure_is_validated());
        let mut branch = source.clone();
        assert!(!branch.structure_is_validated());
        branch.content.parts.get_mut(PART).unwrap().parent_id = PART.into();
        assert_eq!(
            branch.validate_structure_since(&source),
            branch.validate_structure()
        );
        assert!(!branch.structure_is_validated());
        let mut checkpoint = branch.checkpoint();
        source.exchange_checkpoint(&mut checkpoint).unwrap();
        assert!(!source.structure_is_validated());
        source.exchange_checkpoint(&mut checkpoint).unwrap();
        assert!(source.structure_is_validated());
        assert!(!source.fork_candidate().structure_is_validated());
        source.exchange_checkpoint(&mut checkpoint).unwrap();
        assert!(!source.structure_is_validated());
        let repaired = Document::new();
        assert_eq!(
            repaired.validate_structure_since(&source),
            repaired.validate_structure()
        );
    }

    #[test]
    fn incremental_validation_checks_dependency_and_owned_identity_changes() {
        use crate::{Appearance, BindingAxis, Mesh, MeshBinding, MeshKeyform, Parameter};
        let mut source = Document::new();
        assert!(source
            .initialize(DOC, Canvas::new(100., 100., Vec2::default(), 10.))
            .is_ok());
        let parameter = "00000000-0000-4000-8000-000000000703";
        let mesh = "00000000-0000-4000-8000-000000000704";
        let binding = "00000000-0000-4000-8000-000000000705";
        assert!(source
            .create_parameter(Parameter {
                id: parameter.into(),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(source
            .create_mesh(Mesh {
                id: mesh.into(),
                runtime_id: "Mesh".into(),
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(source
            .create_binding(MeshBinding {
                id: binding.into(),
                mesh_id: mesh.into(),
                axes: vec![BindingAxis {
                    parameter_id: parameter.into(),
                    keys: vec![0., 1.],
                    ..Default::default()
                }],
                keyforms: vec![
                    MeshKeyform {
                        keys: vec![0.],
                        positions: vec![],
                        appearance: Appearance::default(),
                        draw_order: None,
                        samples: vec![]
                    },
                    MeshKeyform {
                        keys: vec![1.],
                        positions: vec![],
                        appearance: Appearance::default(),
                        draw_order: None,
                        samples: vec![]
                    },
                ]
            })
            .status
            .is_ok());
        assert!(source.validate_structure().is_empty());
        let mut branch = source.fork_candidate();
        branch.content.bindings.get_mut(binding).unwrap().keyforms[0].positions =
            vec![Vec2::default()];
        assert!(branch.same_authoring_identities(&source));
        assert!(branch.same_binding_validation_dependencies(&source));
        let full = branch.validate_structure();
        assert!(!full.is_empty());
        assert_eq!(branch.validate_structure_since(&source), full);
        let mut branch = source.fork_candidate();
        branch
            .content
            .parameters
            .get_mut(parameter)
            .unwrap()
            .maximum = 0.5;
        let full = branch.validate_structure();
        assert!(!full.is_empty());
        assert_eq!(branch.validate_structure_since(&source), full);
        let mut branch = source.fork_candidate();
        branch.content.bindings.get_mut(binding).unwrap().axes[0].samples[0].id = mesh.into();
        let full = branch.validate_structure();
        assert!(!full.is_empty());
        assert_eq!(branch.validate_structure_since(&source), full);
    }

    #[test]
    fn restored_checkpoint_is_checked_without_mutating_the_document() {
        let mut source = Document::new();
        assert!(source
            .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
            .is_ok());
        assert!(source
            .create_part(Part {
                id: PART.into(),
                ..Default::default()
            })
            .status
            .is_ok());
        let mut restored = source.clone();
        assert!(restored.validate_structure().is_empty());

        source.content.parts.get_mut(PART).unwrap().parent_id = PART.into();
        source.content.part_order.push(PART.into());
        let mut checkpoint = source.checkpoint();
        restored.exchange_checkpoint(&mut checkpoint).unwrap();
        let revision = restored.revision();
        let issues = restored.validate_structure();
        assert!(issues
            .iter()
            .any(|issue| issue.status.code == "RELATION_CYCLE"));
        assert!(issues
            .iter()
            .any(|issue| issue.status.code == "DUPLICATE_ORDER_ID"));
        assert_eq!(restored.revision(), revision);
        assert_eq!(restored.get_part(PART).unwrap().parent_id, PART);
    }
}
