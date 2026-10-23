//! Candidate-document edits and atomic publication.
use std::collections::HashSet;

use crate::session::{keys_in, object_exists_in, AuthoringSession};
use crate::types::*;
use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, Canvas, ChangeKind, Document,
    EditResult, Glue, ImageAsset, Mesh, MeshBinding, MeshKeyform, Offscreen, Parameter, Part,
    PartId, RotationTransform, SceneBinding, SceneKeyform, Status, Transform, TransformData,
    TransformId, Vec2, VertexId,
};

pub struct EditSession<'a> {
    pub(crate) session: Option<&'a mut AuthoringSession>,
    pub(crate) base_keys: HashSet<ObjectKey>,
    pub(crate) candidate: Option<Document>,
    pub(crate) label: String,
    pub(crate) before: Version,
    pub(crate) aborted: bool,
    pub(crate) kind: ChangeKind,
    pub(crate) categories: std::collections::BTreeSet<crate::ChangeCategory>,
    pub(crate) object_ids: Vec<String>,
    pub(crate) erased_keys: HashSet<ObjectKey>,
    pub(crate) authoring_limits: kasane_core::document::AuthoringLimits,
    pub(crate) resource_load_controls: Vec<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub(crate) binding_controls: Vec<kasane_core::document::ConversionControl>,
    pub(crate) binding_cost: Option<kasane_core::document::ConversionCost>,
}

mod cdi;
mod expression;
mod model3;
mod motion;
mod physics;
mod pose;

impl EditSession<'_> {
    /// Read the current candidate when composing multiple operations in one edit.
    pub fn candidate_document(&self) -> &Document {
        self.candidate.as_ref().expect("edit candidate exists")
    }

    pub fn base_version(&self) -> Version {
        self.before
    }

    /// Apply a scalar field patch to this candidate. Failed preview updates are
    /// recoverable and leave the edit active, unlike ordinary failed edit methods.
    pub fn preview_property(
        &mut self,
        patch: &kasane_core::document::PropertyPatch,
        preview: &mut kasane_core::preview::PreviewState,
        values: kasane_core::PreviewValues,
    ) -> Result<(), SdkError> {
        self.ensure_active("preview_property")?;
        let result = self.document().preview_property(patch, preview, values);
        if !result.status.is_ok() {
            return Err(SdkError::from_status(
                result.status,
                "preview_property",
                Vec::new(),
            ));
        }
        self.record(result, "preview_property", "")
    }

    pub fn patch_property(
        &mut self,
        patch: &kasane_core::document::PropertyPatch,
    ) -> Result<(), SdkError> {
        self.apply_authoring_change("patch_property", "", true, |document| {
            document.patch_property(patch)
        })
    }

    pub(crate) fn document(&mut self) -> &mut Document {
        self.candidate.as_mut().expect("edit candidate exists")
    }
    pub(crate) fn record(
        &mut self,
        result: EditResult,
        operation: &'static str,
        id: &str,
    ) -> Result<(), SdkError> {
        if !result.status.is_ok() {
            self.aborted = true;
            let mut error = SdkError::from_status(result.status, operation, vec![id.into()]);
            error.referrers = result.referrers.into_boxed_slice();
            return Err(error);
        }
        self.categories
            .extend(crate::changes::categories(result.changes.kind));
        self.kind = merge_kind(self.kind, result.changes.kind);
        for id in result.changes.object_ids {
            if !self.object_ids.contains(&id) {
                self.object_ids.push(id);
            }
        }
        Ok(())
    }
    pub(crate) fn track_expression_deletions(&mut self, ids: &[String]) {
        for id in ids {
            let key = ObjectKey {
                kind: ObjectKind::ExpressionEntry,
                id: id.clone(),
            };
            if self.base_keys.contains(&key)
                && self.candidate_document().get_expression_entry(id).is_none()
            {
                self.erased_keys.insert(key);
            }
        }
    }
    pub(crate) fn ensure_active(&self, operation: &'static str) -> Result<(), SdkError> {
        if self.aborted {
            Err(SdkError::new(
                "EDIT_ABORTED",
                "An earlier edit method failed",
                operation,
            ))
        } else {
            Ok(())
        }
    }
    /// A rejected plan poisons the candidate just like a failed domain edit.
    pub(crate) fn ensure_plan_version(
        &mut self,
        version: Version,
        operation: &'static str,
        replan_message: &str,
    ) -> Result<(), SdkError> {
        self.ensure_active(operation)?;
        if self.base_version() != version {
            return Err(self.abort_with(SdkError::new("STALE_PLAN", replan_message, operation)));
        }
        Ok(())
    }

    /// Apply a native edit, preserving its diagnostic ID and retiring removed identities.
    /// Identity-preserving patches avoid enumerating the whole candidate.
    pub(crate) fn apply_authoring_change(
        &mut self,
        operation: &'static str,
        id: &str,
        preserves_identities: bool,
        apply: impl FnOnce(&mut Document) -> EditResult,
    ) -> Result<(), SdkError> {
        self.ensure_active(operation)?;
        let before =
            (!preserves_identities).then(|| crate::session::keys_in(self.candidate_document()));
        let result = apply(self.document());
        self.record(result, operation, id)?;
        if let Some(before) = before {
            self.track_removed_objects(before);
        }
        Ok(())
    }

    pub(crate) fn abort_with(&mut self, error: SdkError) -> SdkError {
        self.aborted = true;
        error
    }
    pub fn replace_export_settings(
        &mut self,
        settings: kasane_core::ExportSettings,
    ) -> Result<(), SdkError> {
        let id = self.document().id().to_owned();
        self.apply_authoring_change("replace_export_settings", &id, true, |document| {
            document.replace_export_settings(settings)
        })
    }
    pub fn replace_source_canvas(&mut self, source_canvas: Option<Canvas>) -> Result<(), SdkError> {
        let id = self.document().id().to_owned();
        self.apply_authoring_change("replace_source_canvas", &id, true, |document| {
            document.replace_source_canvas(source_canvas)
        })
    }
    pub fn replace_model_setup(
        &mut self,
        canvas: impl Into<kasane_core::ModelSetup>,
    ) -> Result<(), SdkError> {
        let id = self.document().id().to_owned();
        self.apply_authoring_change("replace_model_setup", &id, true, |document| {
            document.replace_model_setup(canvas)
        })
    }
    pub fn replace_editor_state(
        &mut self,
        state: kasane_core::document::EditorState,
    ) -> Result<(), SdkError> {
        let id = self.document().id().to_owned();
        self.apply_authoring_change("replace_editor_state", &id, true, |document| {
            document.replace_editor_state(state)
        })
    }
    pub fn replace_hierarchy_order(
        &mut self,
        order: kasane_core::document::HierarchyOrder,
    ) -> Result<(), SdkError> {
        let id = self.document().id().to_owned();
        self.apply_authoring_change("replace_hierarchy_order", &id, true, |document| {
            document.replace_hierarchy_order(order)
        })
    }
    pub fn replace_draw_order_groups(
        &mut self,
        groups: Vec<DrawOrderGroup>,
    ) -> Result<(), SdkError> {
        let id = self.document().id().to_owned();
        self.apply_authoring_change("replace_draw_order_groups", &id, false, |document| {
            document.replace_draw_order_groups(groups)
        })
    }
    pub(crate) fn track_removed_objects(
        &mut self,
        before: std::collections::HashSet<crate::types::ObjectKey>,
    ) {
        let after = crate::session::keys_in(self.candidate_document());
        self.erased_keys.extend(before.difference(&after).cloned());
    }

    pub fn erase_object(&mut self, id: &str) -> Result<(), SdkError> {
        self.apply_authoring_change("erase_object", id, false, |document| {
            document.erase_object(id)
        })
    }
    pub fn create_asset(&mut self, asset: ImageAsset) -> Result<(), SdkError> {
        let id = asset.id.clone();
        self.apply_authoring_change("create_asset", &id, true, |document| {
            document.add_asset(asset)
        })
    }
    pub fn replace_asset(&mut self, asset: ImageAsset) -> Result<(), SdkError> {
        let id = asset.id.clone();
        self.apply_authoring_change("replace_asset", &id, true, |document| {
            document.replace_asset(asset)
        })
    }
    pub fn create_parameter(&mut self, parameter: Parameter) -> Result<(), SdkError> {
        let id = parameter.id.clone();
        self.apply_authoring_change("create_parameter", &id, true, |document| {
            document.create_parameter(parameter)
        })
    }
    pub fn replace_parameter(&mut self, parameter: Parameter) -> Result<(), SdkError> {
        let id = parameter.id.clone();
        self.apply_authoring_change("replace_parameter", &id, true, |document| {
            document.replace_parameter(parameter)
        })
    }
    pub fn create_part(&mut self, part: Part) -> Result<(), SdkError> {
        let id = part.id.clone();
        self.apply_authoring_change("create_part", &id, true, |document| {
            document.create_part(part)
        })
    }
    pub fn replace_part(&mut self, part: Part) -> Result<(), SdkError> {
        let id = part.id.clone();
        self.apply_authoring_change("replace_part", &id, true, |document| {
            document.replace_part(part)
        })
    }
    /// Organization hierarchy is independent of the transform hierarchy.
    pub fn set_organization_parent(
        &mut self,
        part_id: &str,
        parent_id: &str,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_organization_parent")?;
        let Some(mut part) = self.document().get_part(part_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_PART", "Part does not exist"),
                "set_organization_parent",
                vec![part_id.into()],
            )));
        };
        part.parent_id = parent_id.into();
        let result = self.document().replace_part(part);
        self.record(result, "set_organization_parent", part_id)
    }
    pub fn create_transform(&mut self, transform: Transform) -> Result<(), SdkError> {
        let id = transform.id.clone();
        self.apply_authoring_change("create_transform", &id, true, |document| {
            document.create_transform(transform)
        })
    }
    /// Replace a Warp and its ordinary keyforms in one validated operation.
    pub fn replace_warp_authoring(
        &mut self,
        transform: Transform,
        binding: Option<SceneBinding>,
    ) -> Result<(), SdkError> {
        let id = transform.id.clone();
        self.apply_authoring_change("replace_warp_authoring", &id, false, |document| {
            document.replace_warp_authoring(transform, binding)
        })
    }
    pub fn replace_transform(&mut self, transform: Transform) -> Result<(), SdkError> {
        let id = transform.id.clone();
        self.apply_authoring_change("replace_transform", &id, true, |document| {
            document.replace_transform(transform)
        })
    }
    /// Changes the transform parent while retaining the stored local coordinates.
    pub fn set_transform_parent(
        &mut self,
        transform_id: &str,
        parent_id: Option<TransformId>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_transform_parent")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "set_transform_parent",
                vec![transform_id.into()],
            )));
        };
        transform.parent_id = parent_id;
        let result = self.document().replace_transform(transform);
        self.record(result, "set_transform_parent", transform_id)
    }
    pub fn set_transform_part(
        &mut self,
        transform_id: &str,
        part_id: Option<PartId>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_transform_part")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "set_transform_part",
                vec![transform_id.into()],
            )));
        };
        transform.part_id = part_id;
        let result = self.document().replace_transform(transform);
        self.record(result, "set_transform_part", transform_id)
    }
    pub fn update_rotation(
        &mut self,
        transform_id: &str,
        rotation: RotationTransform,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_rotation")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "update_rotation",
                vec![transform_id.into()],
            )));
        };
        if transform.rotation().is_none() {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("INVALID_TRANSFORM_TYPE", "Transform is not a rotation"),
                "update_rotation",
                vec![transform_id.into()],
            )));
        }
        transform.data = TransformData::Rotation(rotation);
        let result = self.document().replace_transform(transform);
        self.record(result, "update_rotation", transform_id)
    }
    pub fn update_warp_points(
        &mut self,
        transform_id: &str,
        points: Vec<Vec2>,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_warp_points")?;
        let warp = self
            .candidate_document()
            .get_transform(transform_id)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", "Transform does not exist"))
            .and_then(|t| {
                t.warp().ok_or_else(|| {
                    Status::error("INVALID_TRANSFORM_TYPE", "Transform is not a warp")
                })
            })
            .map_err(|status| {
                SdkError::from_status(status, "update_warp_points", vec![transform_id.into()])
            });
        let warp = match warp {
            Ok(w) => w,
            Err(e) => return Err(self.abort_with(e)),
        };
        if points.len() != warp.points.len() {
            return Err(self.abort_with(SdkError::new(
                "INVALID_LENGTH",
                "Use a Warp resize plan to change topology",
                "update_warp_points",
            )));
        }
        let columns = warp.columns;
        let clear_bezier = warp.points != points;
        self.edit_scene_content(crate::SceneContentEdit::WarpPoints {
            transform_id: transform_id.into(),
            target: crate::ShapeTarget::Base,
            updates: points
                .into_iter()
                .enumerate()
                .map(|(i, point)| crate::WarpPointPatch {
                    row: i as u32 / (columns + 1),
                    column: i as u32 % (columns + 1),
                    point,
                })
                .collect(),
            clear_bezier,
        })
    }
    pub fn create_mesh(&mut self, mesh: Mesh) -> Result<(), SdkError> {
        let id = mesh.id.clone();
        self.apply_authoring_change("create_mesh", &id, true, |document| {
            document.create_mesh(mesh)
        })
    }
    pub fn replace_mesh(&mut self, mesh: Mesh) -> Result<(), SdkError> {
        let id = mesh.id.clone();
        self.apply_authoring_change("replace_mesh", &id, true, |document| {
            document.replace_mesh(mesh)
        })
    }
    /// Changes the mesh's deform parent while retaining source positions as local coordinates.
    pub fn set_deform_parent(&mut self, mesh_id: &str, transform_id: &str) -> Result<(), SdkError> {
        self.ensure_active("set_deform_parent")?;
        let Some(mut mesh) = self.document().get_mesh(mesh_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "set_deform_parent",
                vec![mesh_id.into()],
            )));
        };
        mesh.deformer_id = transform_id.into();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "set_deform_parent", mesh_id)
    }
    pub fn set_mesh_part(&mut self, mesh_id: &str, part_id: &str) -> Result<(), SdkError> {
        self.ensure_active("set_mesh_part")?;
        let Some(mut mesh) = self.document().get_mesh(mesh_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "set_mesh_part",
                vec![mesh_id.into()],
            )));
        };
        mesh.part_id = part_id.into();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "set_mesh_part", mesh_id)
    }
    pub fn rename_mesh(&mut self, id: &str, name: impl Into<String>) -> Result<(), SdkError> {
        self.apply_authoring_change("rename_mesh", id, true, |document| {
            document.rename_mesh(id, name.into())
        })
    }
    pub fn set_mesh_opacity(&mut self, id: &str, opacity: f32) -> Result<(), SdkError> {
        self.apply_authoring_change("set_mesh_opacity", id, true, |document| {
            document.set_mesh_opacity(id, opacity)
        })
    }

    pub fn update_mesh_properties(
        &mut self,
        id: &str,
        properties: MeshProperties,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_mesh_properties")?;
        let Some(mut mesh) = self.document().get_mesh(id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "update_mesh_properties",
                vec![id.into()],
            )));
        };
        mesh.texture_asset_id = properties.texture_asset_id.into();
        mesh.appearance = properties.appearance;
        mesh.draw_order = properties.draw_order;
        mesh.blend_mode = properties.blend_mode;
        mesh.enabled = properties.enabled;
        mesh.double_sided = properties.double_sided;
        mesh.inverted_mask = properties.inverted_mask;
        mesh.masks = properties.masks;
        let result = self.document().replace_mesh(mesh);
        self.record(result, "update_mesh_properties", id)
    }
    pub fn replace_topology(
        &mut self,
        source: &GeometrySnapshot,
        replacement: TopologyReplacement,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_topology")?;
        if source.version != self.before || source.mesh_id != replacement.mesh.id {
            let mut error = SdkError::new(
                "STALE_TOPOLOGY",
                "Topology snapshot is not from this edit's starting version and mesh",
                "replace_topology",
            );
            error.object_ids = vec![source.mesh_id.clone(), replacement.mesh.id.clone()];
            error.expected_version = Some(Box::new(source.version));
            error.actual_version = Some(Box::new(self.before));
            return Err(self.abort_with(error));
        }
        let id = replacement.mesh.id.clone();
        let before = crate::session::keys_in(self.candidate_document());
        let result = self.document().replace_mesh_topology(
            replacement.mesh,
            replacement.binding,
            replacement.blend_bindings,
            replacement.glues,
            replacement.vertex_mapping,
        );
        self.record(result, "replace_topology", &id)?;
        self.track_removed_objects(before);
        Ok(())
    }
    pub fn create_binding(&mut self, binding: MeshBinding) -> Result<(), SdkError> {
        let id = binding.id.clone();
        self.apply_authoring_change("create_binding", &id, true, |document| {
            document.create_binding(binding)
        })
    }
    pub fn replace_binding(&mut self, binding: MeshBinding) -> Result<(), SdkError> {
        let id = binding.id.clone();
        self.apply_authoring_change("replace_binding", &id, false, |document| {
            document.replace_binding(binding)
        })
    }
    pub fn set_mesh_keyform(
        &mut self,
        binding_id: &str,
        form: MeshKeyform,
    ) -> Result<(), SdkError> {
        self.apply_authoring_change("set_mesh_keyform", binding_id, true, |document| {
            document.set_mesh_keyform(binding_id, form)
        })
    }
    pub fn create_scene_binding(&mut self, binding: SceneBinding) -> Result<(), SdkError> {
        let id = binding.id.clone();
        self.apply_authoring_change("create_scene_binding", &id, true, |document| {
            document.create_scene_binding(binding)
        })
    }
    pub fn replace_scene_binding(&mut self, binding: SceneBinding) -> Result<(), SdkError> {
        let id = binding.id.clone();
        self.apply_authoring_change("replace_scene_binding", &id, false, |document| {
            document.replace_scene_binding(binding)
        })
    }
    pub fn set_scene_keyform(
        &mut self,
        binding_id: &str,
        form: SceneKeyform,
    ) -> Result<(), SdkError> {
        self.apply_authoring_change("set_scene_keyform", binding_id, true, |document| {
            document.set_scene_keyform(binding_id, form)
        })
    }
    pub fn create_blend_key_table(&mut self, table: BlendShapeKeyTable) -> Result<(), SdkError> {
        let id = table.id.clone();
        self.apply_authoring_change("create_blend_key_table", &id, true, |document| {
            document.create_blend_key_table(table)
        })
    }
    pub fn replace_blend_key_table(&mut self, table: BlendShapeKeyTable) -> Result<(), SdkError> {
        let id = table.id.clone();
        self.apply_authoring_change("replace_blend_key_table", &id, false, |document| {
            document.replace_blend_key_table(table)
        })
    }
    pub fn create_blend_constraint(
        &mut self,
        constraint: BlendShapeConstraint,
    ) -> Result<(), SdkError> {
        let id = constraint.id.clone();
        self.apply_authoring_change("create_blend_constraint", &id, true, |document| {
            document.create_blend_constraint(constraint)
        })
    }
    pub fn replace_blend_constraint(
        &mut self,
        constraint: BlendShapeConstraint,
    ) -> Result<(), SdkError> {
        let id = constraint.id.clone();
        self.apply_authoring_change("replace_blend_constraint", &id, false, |document| {
            document.replace_blend_constraint(constraint)
        })
    }
    pub fn create_blend_binding(&mut self, binding: BlendShapeBinding) -> Result<(), SdkError> {
        let id = binding.id.clone();
        self.apply_authoring_change("create_blend_binding", &id, true, |document| {
            document.create_blend_binding(binding)
        })
    }
    pub fn replace_blend_binding(&mut self, binding: BlendShapeBinding) -> Result<(), SdkError> {
        let id = binding.id.clone();
        self.apply_authoring_change("replace_blend_binding", &id, false, |document| {
            document.replace_blend_binding(binding)
        })
    }
    pub fn create_glue(&mut self, glue: Glue) -> Result<(), SdkError> {
        let id = glue.id.clone();
        self.apply_authoring_change("create_glue", &id, true, |document| {
            document.create_glue(glue)
        })
    }
    pub fn replace_glue(&mut self, glue: Glue) -> Result<(), SdkError> {
        let id = glue.id.clone();
        self.apply_authoring_change("replace_glue", &id, false, |document| {
            document.replace_glue(glue)
        })
    }
    pub fn create_offscreen(&mut self, offscreen: Offscreen) -> Result<(), SdkError> {
        let id = offscreen.id.clone();
        self.apply_authoring_change("create_offscreen", &id, true, |document| {
            document.create_offscreen(offscreen)
        })
    }
    pub fn replace_offscreen(&mut self, offscreen: Offscreen) -> Result<(), SdkError> {
        let id = offscreen.id.clone();
        self.apply_authoring_change("replace_offscreen", &id, true, |document| {
            document.replace_offscreen(offscreen)
        })
    }
    pub fn replace_part_binding_with_offscreen(
        &mut self,
        binding: SceneBinding,
        offscreen: Offscreen,
    ) -> Result<(), SdkError> {
        let id = binding.id.clone();
        self.apply_authoring_change(
            "replace_part_binding_with_offscreen",
            &id,
            false,
            |document| document.replace_part_binding_with_offscreen(binding, offscreen),
        )
    }
    pub fn update_positions(
        &mut self,
        mesh_id: &str,
        vertex_ids: &[VertexId],
        positions: &[Vec2],
    ) -> Result<(), SdkError> {
        self.apply_authoring_change("update_positions", mesh_id, true, |document| {
            document.set_vertex_positions(mesh_id, vertex_ids, positions)
        })
    }
    pub fn commit(mut self) -> Result<EditReceipt, SdkError> {
        let session = self.session.take().expect("attached edit has a session");
        self.commit_to(session)
    }

    /// Publish an owned edit workspace to its original session.
    pub fn commit_to(mut self, session: &mut AuthoringSession) -> Result<EditReceipt, SdkError> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("sdk.commit");
        self.ensure_active("commit")?;
        if session.version() != self.before {
            let mut error = SdkError::new("STALE_VERSION", "Document version changed", "commit");
            error.expected_version = Some(Box::new(self.before));
            error.actual_version = Some(Box::new(session.version()));
            return Err(error);
        }
        if let Some(cost) = self.binding_cost {
            crate::binding_authoring::check_cost(cost, session.authoring_limits)?;
        }
        for control in &self.resource_load_controls {
            if control.load(std::sync::atomic::Ordering::Acquire) {
                return Err(SdkError::new(
                    "CANCELLED",
                    "Resource load cancelled",
                    "commit",
                ));
            }
        }
        for control in &self.binding_controls {
            control
                .check()
                .map_err(|s| SdkError::from_status(s, "commit", vec![]))?;
        }
        let candidate = self.candidate.take().expect("edit candidate exists");
        if let Some(issue) = candidate.validate_structure().into_iter().next() {
            return Err(SdkError::from_status(
                issue.status,
                "commit",
                vec![issue.object_id],
            ));
        }
        let identity_keys: HashSet<_> = self
            .erased_keys
            .iter()
            .filter(|key| object_exists_in(&candidate, key.kind, &key.id))
            .cloned()
            .collect();
        let identity_changed = !identity_keys.is_empty();
        let changed = identity_changed || !session.project.document().same_content(&candidate);
        let before_keys = changed.then(|| session.object_keys());
        let after_keys = changed.then(|| keys_in(&candidate));
        let mut changes = if let (Some(keys), Some(after_keys)) = (&before_keys, &after_keys) {
            crate::changes::summarize(
                session.document(),
                &candidate,
                keys,
                after_keys,
                &self.object_ids,
                &self.categories,
            )
        } else {
            crate::ChangeSet::default()
        };
        for key in &identity_keys {
            let entity = crate::EntityRef {
                document_id: candidate.id().into(),
                kind: key.kind,
                id: key.id.clone(),
            };
            if !changes.created.contains(&entity) {
                changes.created.push(entity.clone());
            }
            if !changes.deleted.contains(&entity) {
                changes.deleted.push(entity);
            }
        }
        let previous = changed.then(|| session.project.document().checkpoint());
        let mut evict_count = 0usize;
        if let Some(checkpoint) = &previous {
            let limits = session.history_limits;
            let candidate_bytes = candidate.estimated_content_bytes();
            let entry_bytes = checkpoint.estimated_bytes()
                + changes.estimated_bytes()
                + self
                    .object_ids
                    .iter()
                    .map(|id| id.capacity() + std::mem::size_of::<String>())
                    .sum::<usize>()
                + self.label.capacity()
                + std::mem::size_of::<String>()
                + identity_keys.capacity() * std::mem::size_of::<ObjectKey>()
                + identity_keys
                    .iter()
                    .map(|key| key.id.capacity())
                    .sum::<usize>()
                + session.project.root().as_os_str().len();
            let redo_bytes: usize = session.redo.iter().map(HistoryEntry::estimated_bytes).sum();
            let mut projected_bytes = candidate_bytes
                .saturating_add(entry_bytes)
                .saturating_add(redo_bytes)
                .saturating_add(
                    session
                        .done
                        .iter()
                        .map(HistoryEntry::estimated_bytes)
                        .sum::<usize>(),
                );
            let mut projected_steps = session.done.len() + 1;
            while (projected_bytes > limits.max_bytes || projected_steps > limits.max_steps)
                && evict_count < session.done.len()
            {
                projected_bytes =
                    projected_bytes.saturating_sub(session.done[evict_count].estimated_bytes());
                projected_steps -= 1;
                evict_count += 1;
            }
            if projected_bytes > limits.max_bytes || projected_steps > limits.max_steps {
                return Err(SdkError::new(
                    "HISTORY_LIMIT_EXCEEDED",
                    "Edit cannot fit the history budget",
                    "commit",
                ));
            }
        }
        // Cancellation after this last check belongs to the next operation;
        // publication and its history/event changes form one atomic boundary.
        for control in &self.resource_load_controls {
            if control.load(std::sync::atomic::Ordering::Acquire) {
                return Err(SdkError::new(
                    "CANCELLED",
                    "Resource load cancelled",
                    "commit",
                ));
            }
        }
        for control in &self.binding_controls {
            control
                .check()
                .map_err(|s| SdkError::from_status(s, "commit", vec![]))?;
        }
        session
            .project
            .publish_authoring_candidate(candidate, self.kind, identity_changed)
            .map_err(|s| SdkError::from_status(s, "commit", self.object_ids.clone()))?;
        // A successfully accepted load consumes its request even for a no-op.
        // Rejected/dropped candidates leave requests available for retry.
        for control in &self.resource_load_controls {
            control.store(true, std::sync::atomic::Ordering::Release);
        }
        if changed {
            session
                .preview
                .retain_parameters(session.project.document());
        }
        if let (Some(keys), Some(after_keys)) = (&before_keys, &after_keys) {
            session.refresh_incarnations_from_keys(keys, after_keys, &identity_keys);
        }
        if let Some(checkpoint) = previous {
            for _ in 0..evict_count {
                session.done.pop_front();
            }
            session.done.push_back(HistoryEntry {
                label: self.label.clone(),
                changes: changes.clone(),
                kind: self.kind,
                object_ids: self.object_ids.clone(),
                checkpoint,
                identity_keys,
                root: session.project.root(),
            });
            session.redo.clear();
        }
        let receipt = EditReceipt {
            label: self.label.clone(),
            before: self.before,
            after: session.version(),
            kind: if changed { self.kind } else { ChangeKind::None },
            object_ids: if changed {
                std::mem::take(&mut self.object_ids)
            } else {
                Vec::new()
            },
            changed,
            changes,
        };
        if changed {
            session.publish_receipt(receipt.clone());
        }
        Ok(receipt)
    }
}

fn merge_kind(a: ChangeKind, b: ChangeKind) -> ChangeKind {
    use ChangeKind::*;
    match (a, b) {
        (Structure, _) | (_, Structure) => Structure,
        (Resources, _) | (_, Resources) => Resources,
        (Positions, _) | (_, Positions) => Positions,
        (Appearance, _) | (_, Appearance) => Appearance,
        (Metadata, _) | (_, Metadata) => Metadata,
        _ => None,
    }
}
