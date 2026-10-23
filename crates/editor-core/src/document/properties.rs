//! Atomic property edits and owned inspector candidates.
use super::*;
use crate::{ObjectKind, ObjectRef, PropertyPatch, PropertyScope, PropertyValues};
use anyhow::bail;

enum PropertyPayload {
    Object(serde_json::Value),
    Physics(kasane_core::document::PhysicsPatch),
    Motion(kasane_core::document::MotionEdit),
    Pose(kasane_core::document::PoseEdit),
    Expression(kasane_core::document::ExpressionEdit),
}
impl PropertyPayload {
    fn json(self) -> Result<serde_json::Value> {
        if let Self::Object(value) = self {
            Ok(value)
        } else {
            bail!("Expected object payload")
        }
    }
    fn json_ref(&self) -> Result<&serde_json::Value> {
        if let Self::Object(value) = self {
            Ok(value)
        } else {
            bail!("Expected object payload")
        }
    }
}

impl Document {
    /// Committed values remain stable while an inspector candidate is displayed.
    pub fn property_values(&self, object: &ObjectRef) -> Result<PropertyValues> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.property_values");
        Self::property_values_in(
            self.session.document(),
            object,
            &self.property_scope(object),
        )
    }

    pub fn property_scope(&self, object: &ObjectRef) -> PropertyScope {
        if matches!(
            object.kind,
            ObjectKind::MotionRegistration
                | ObjectKind::MotionGroup
                | ObjectKind::ExpressionEntry
                | ObjectKind::Physics
                | ObjectKind::Pose
                | ObjectKind::PoseGroup
                | ObjectKind::PoseEntry
                | ObjectKind::PoseLink
        ) {
            return PropertyScope::Resource {
                revision: self.model_generation(),
            };
        }
        if object.kind != ObjectKind::Mesh {
            return PropertyScope::Object;
        }
        let model = self.session.document();
        let Some(binding) = model
            .binding_for_mesh(&object.id)
            .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
        else {
            return PropertyScope::Object;
        };
        let form = binding.keyforms.iter().find(|form| {
            binding
                .axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty())
                .count()
                == form.keys.len()
                && binding
                    .axes
                    .iter()
                    .filter(|a| !a.parameter_id.is_empty())
                    .zip(&form.keys)
                    .all(|(axis, key)| {
                        model
                            .get_parameter(&axis.parameter_id)
                            .is_some_and(|parameter| {
                                let value = self
                                    .session
                                    .preview_values()
                                    .get(&axis.parameter_id)
                                    .copied()
                                    .unwrap_or(parameter.default_value)
                                    .clamp(parameter.minimum, parameter.maximum);
                                // Modeling preview inputs are constrained to the parameter
                                // range, but repeating parameters still wrap its upper end.
                                // Match runtime evaluation when choosing the authoring target.
                                let value = if parameter.repeat {
                                    let range = parameter.maximum - parameter.minimum;
                                    let normalized = (value - parameter.minimum) / range;
                                    let wrapped = (normalized - normalized.floor()) * range
                                        + parameter.minimum;
                                    if wrapped < parameter.minimum || wrapped >= parameter.maximum {
                                        parameter.minimum
                                    } else {
                                        wrapped
                                    }
                                } else {
                                    value
                                };
                                (value - key).abs() < 1e-6
                            })
                    })
        });
        form.map_or(PropertyScope::InterpolatedMesh, |f| {
            PropertyScope::MeshKeyform {
                binding: binding.id.clone(),
                samples: f.samples.clone(),
            }
        })
    }

    pub(crate) fn property_values_in(
        model: &kasane_core::Document,
        object: &ObjectRef,
        scope: &PropertyScope,
    ) -> Result<PropertyValues> {
        if object.kind != ObjectKind::Mesh {
            return crate::property::values_from(model, object);
        }
        let payload = crate::property::mesh::inspector_payload(model, &object.id, scope)?;
        crate::property_specs(object.kind)
            .iter()
            .map(|spec| Ok((spec.key, spec.read(&payload)?)))
            .collect()
    }

    pub fn set_object_property(
        &mut self,
        object: &ObjectRef,
        field: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        let patch = self.object_property_patch(object, field, &value)?;
        self.set_object_properties(object, &patch)
    }

    pub(crate) fn object_property_patch(
        &self,
        object: &ObjectRef,
        field: &str,
        value: &serde_json::Value,
    ) -> Result<PropertyPatch> {
        let spec = crate::property_specs(object.kind)
            .iter()
            .find(|s| s.path == field)
            .ok_or_else(|| anyhow!("This property is read only"))?;
        let current = self.property_values(object)?;
        ensure!(
            current.contains_key(&spec.key),
            "This property requires an element index"
        );
        let expected = crate::property_specs(object.kind)
            .iter()
            .filter(|s| s.same_group(spec))
            .map(|s| (s.key, current[&s.key].clone()))
            .collect();
        Ok(PropertyPatch {
            scope: if spec.keyframed {
                self.property_scope(object)
            } else {
                PropertyScope::Object
            },
            expected,
            changes: [(spec.key, spec.decode(value)?)].into(),
        })
    }

    fn validate_property_baseline(&self, object: &ObjectRef, patch: &PropertyPatch) -> Result<()> {
        self.ensure_editable(std::slice::from_ref(object))?;
        if let PropertyScope::Resource { revision } = patch.scope {
            ensure!(
                revision == self.model_generation(),
                "Resource field address expired"
            );
        }
        if matches!(
            object.kind,
            ObjectKind::MotionRegistration | ObjectKind::MotionGroup | ObjectKind::ExpressionEntry
        ) {
            ensure!(
                patch.scope == self.property_scope(object),
                "Resource field address expired"
            );
        }
        let specs = crate::property_specs(object.kind);
        let current = self.property_values(object)?;
        for (id, value) in &patch.expected {
            ensure!(
                current.get(id) == Some(value),
                "Property changed outside this form: {}. Reload the group before applying.",
                id.name()
            );
        }
        for (id, value) in &patch.changes {
            let spec = crate::property_spec(*id)
                .filter(|spec| spec.key.object_kind() == object.kind)
                .ok_or_else(|| anyhow!("This property is read only"))?;
            ensure!(patch.expected.contains_key(id), "Missing property baseline");
            spec.validate(value)?;
            ensure!(!spec.readonly, "This property is read only");
            if spec.keyframed {
                ensure!(
                    patch.scope != PropertyScope::InterpolatedMesh,
                    "Select a stored keyform before editing this property"
                );
                ensure!(
                    patch.scope == self.property_scope(object),
                    "Keyform target changed"
                );
            }
            ensure!(
                specs
                    .iter()
                    .filter(|s| s.same_group(&spec))
                    .all(|s| patch.expected.contains_key(&s.key)),
                "Missing property baseline"
            );
        }
        Ok(())
    }

    fn checked_payload(
        &self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<PropertyPayload> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.checked_payload");
        self.validate_property_baseline(object, patch)?;
        let specs = crate::property_specs(object.kind);
        if object.kind == ObjectKind::PhysicsGroup {
            return crate::property::physics::patch(
                self.session.document(),
                object,
                &patch.changes,
            )
            .map(PropertyPayload::Physics);
        }
        let mut payload = if object.kind == ObjectKind::Mesh {
            if patch.mesh_presentation_only(object.kind) || patch.uses_scalar_patch(object.kind) {
                crate::property::mesh::inspector_payload(
                    self.session.document(),
                    &object.id,
                    &patch.scope,
                )?
            } else {
                crate::property::mesh::payload(self.session.document(), &object.id, &patch.scope)?
            }
        } else if patch.uses_scalar_patch(object.kind) {
            crate::property::inspector_payload(self.session.document(), object)?
        } else {
            crate::property::object_payload(self.session.document(), object)?
        };
        for (id, value) in &patch.changes {
            let spec = specs
                .iter()
                .find(|spec| spec.key == *id)
                .expect("validated property key");
            spec.write(&mut payload, value)?;
        }
        match object.kind {
            ObjectKind::MotionGroup | ObjectKind::MotionRegistration => {
                crate::property::animation_entry::motion_patch(
                    self.session.document(),
                    object,
                    payload,
                )
                .map(PropertyPayload::Motion)
            }
            ObjectKind::ExpressionEntry => crate::property::animation_entry::expression_patch(
                self.session.document(),
                object,
                payload,
            )
            .map(PropertyPayload::Expression),
            ObjectKind::Pose
            | ObjectKind::PoseGroup
            | ObjectKind::PoseEntry
            | ObjectKind::PoseLink => {
                crate::property::business::pose_patch(self.session.document(), object, payload)
                    .map(PropertyPayload::Pose)
            }
            ObjectKind::Physics => crate::property::business::physics_patch(object, payload)
                .map(PropertyPayload::Physics),
            _ => Ok(PropertyPayload::Object(payload)),
        }
    }

    fn apply_properties(
        &self,
        edit: &mut EditSession<'_>,
        object: &ObjectRef,
        patch: &PropertyPatch,
        payload: PropertyPayload,
    ) -> Result<()> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.apply_properties");
        if let PropertyPayload::Physics(patch) = payload {
            return Ok(edit.patch_physics(&patch)?);
        }
        let payload = match payload {
            PropertyPayload::Motion(intent) => return Ok(edit.edit_motion(intent)?),
            PropertyPayload::Pose(intent) => return Ok(edit.edit_pose(intent)?),
            PropertyPayload::Expression(intent) => return Ok(edit.edit_expression(intent)?),
            payload => payload.json()?,
        };
        if patch.uses_scalar_patch(object.kind) {
            return Ok(edit.patch_property(&self.scalar_patch(object, patch, &payload)?)?);
        }
        if matches!(
            object.kind,
            ObjectKind::Mesh | ObjectKind::Deformer | ObjectKind::Glue | ObjectKind::Offscreen
        ) && patch.changes.len() == 1
            && patch
                .changes
                .keys()
                .all(|key| crate::property_spec(*key).is_some_and(|s| s.path == "name"))
        {
            let expected = edit
                .candidate_document()
                .scene_field(&object.id, kasane_sdk::SceneField::Label)
                .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
            return Ok(edit.edit_scene(kasane_sdk::SceneEdit::Patch {
                id: object.id.clone(),
                expected,
                value: kasane_sdk::SceneFieldValue::Label(payload["name"].as_str().unwrap().into()),
                clear_glue_pairs: false,
            })?);
        }
        if object.kind != ObjectKind::Mesh {
            return replace(edit, object.kind, payload);
        }
        use crate::property::mesh;
        let original = self
            .session
            .document()
            .get_mesh(&object.id)
            .ok_or_else(|| anyhow!("Mesh no longer exists"))?;
        let mut next: kasane_core::Mesh = if patch.mesh_presentation_only(object.kind) {
            let mut next = original.clone();
            next.name = serde_json::from_value(payload["name"].clone())?;
            next.enabled = serde_json::from_value(payload["enabled"].clone())?;
            next.appearance = serde_json::from_value(payload["appearance"].clone())?;
            next.draw_order = serde_json::from_value(payload["draw_order"].clone())?;
            next
        } else {
            serde_json::from_value(payload.clone())?
        };
        if patch.changes.contains_key(&mesh::COLOR_BLEND.key)
            || patch.changes.contains_key(&mesh::ALPHA_BLEND.key)
        {
            mesh::normalize_blend(&mut next, &payload)?;
        }
        next.double_sided = !payload["culling"].as_bool().unwrap();
        let pose_changed = patch.changes.keys().any(|key| {
            crate::property_specs(ObjectKind::Mesh)
                .iter()
                .any(|spec| spec.key == *key && spec.keyframed)
        });
        if let Some((binding, form)) =
            mesh::keyform(self.session.document(), &object.id, &patch.scope)?
        {
            if pose_changed {
                let mut form = form.clone();
                form.appearance = next.appearance;
                form.draw_order = next.draw_order;
                edit.set_mesh_keyform(binding, form)?;
            }
            next.appearance = original.appearance;
            next.draw_order = original.draw_order;
        }
        // Prepare against the same policy as tree moves, and include ordering
        // and legacy drawing-group protection in this single history step.
        let plan = if next.part_id != original.part_id {
            self.prepare_organization_move(
                object,
                crate::OrganizationDestination::Part(
                    (!next.part_id.is_empty()).then(|| next.part_id.clone()),
                ),
            )?
        } else {
            None
        };
        if let Some(plan) = plan {
            plan.apply(edit, object)?;
        }
        if &next != original {
            edit.replace_mesh(next)?;
        }
        Ok(())
    }

    fn scalar_patch(
        &self,
        object: &ObjectRef,
        patch: &PropertyPatch,
        payload: &serde_json::Value,
    ) -> Result<kasane_core::document::PropertyPatch> {
        use kasane_core::document::PropertyPatch as P;
        let id = object.id.clone();
        fn read<T: serde::de::DeserializeOwned>(p: &serde_json::Value, key: &str) -> Result<T> {
            Ok(serde_json::from_value(p[key].clone())?)
        }
        Ok(match object.kind {
            ObjectKind::Mesh => P::Mesh {
                id,
                keyform: crate::property::mesh::keyform(
                    self.session.document(),
                    &object.id,
                    &patch.scope,
                )?
                .map(|(binding, form)| (binding.to_owned(), form.samples.clone())),
                appearance: read(payload, "appearance")?,
                draw_order: read(payload, "draw_order")?,
            },
            ObjectKind::Parameter => P::Parameter {
                id,
                minimum: read(payload, "minimum")?,
                maximum: read(payload, "maximum")?,
                default_value: read(payload, "default_value")?,
                decimal_places: read(payload, "decimal_places")?,
            },
            ObjectKind::Part => P::Part {
                id,
                draw_order: read(payload, "draw_order")?,
            },
            ObjectKind::Glue => P::Glue {
                id,
                intensity: read(payload, "intensity")?,
            },
            ObjectKind::Motion => P::Motion {
                id,
                duration: read(payload, "duration")?,
                fps: read(payload, "fps")?,
                fade_in: read(payload, "fade_in")?,
                fade_out: read(payload, "fade_out")?,
            },
            ObjectKind::Expression => P::Expression {
                id,
                fade_in: read(payload, "fade_in")?,
                fade_out: read(payload, "fade_out")?,
            },
            ObjectKind::Document => P::Export {
                padding_pixels: read(&payload["export_settings"], "padding_pixels")?,
                minimum_extent: read(&payload["export_settings"], "minimum_extent")?,
            },
            _ => bail!("This property does not support scalar patches"),
        })
    }

    fn parameter_from_payload(
        &self,
        object: &ObjectRef,
        payload: serde_json::Value,
    ) -> Result<ParameterDraft> {
        let group = payload["group_id"].as_str().unwrap_or_default().to_owned();
        let p: kasane_core::Parameter = serde_json::from_value(payload)?;
        let mut draft = self.parameter_draft(&object.id)?;
        draft.name = p.name;
        draft.runtime_id = p.runtime_id;
        draft.minimum = p.minimum;
        draft.maximum = p.maximum;
        draft.default_value = p.default_value;
        draft.decimal_places = p.decimal_places;
        draft.repeat = p.repeat;
        draft.kind = p.kind;
        draft.group_id = (!group.is_empty()).then_some(group);
        Ok(draft)
    }

    fn parameter_group_from_payload(
        &self,
        id: &str,
        payload: &serde_json::Value,
    ) -> Result<crate::ParameterGroupDraft> {
        let mut draft = self.parameter_group_draft(id)?;
        draft.name = payload["name"].as_str().unwrap_or_default().into();
        draft.runtime_id = payload["runtime_id"].as_str().unwrap_or_default().into();
        Ok(draft)
    }

    pub fn validate_object_properties(
        &self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<()> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.validate_properties");
        if object.kind == ObjectKind::PhysicsGroup {
            let PropertyPayload::Physics(patch) = self.checked_payload(object, patch)? else {
                unreachable!()
            };
            let status = self.session.document().validate_physics_patch(&patch);
            ensure!(status.is_ok(), "{}: {}", status.code, status.message);
            return Ok(());
        }
        self.prepare_object_properties(object, patch).map(|_| ())
    }

    pub(crate) fn prepare_object_properties(
        &self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<Option<EditSession<'static>>> {
        let payload = self.checked_payload(object, patch)?;
        if object.kind == ObjectKind::Parameter {
            let draft = self.parameter_from_payload(object, payload.json()?)?;
            return self.prepare_parameter_update(&object.id, &draft).map(Some);
        }
        if object.kind == ObjectKind::ParameterGroup {
            self.validate_parameter_group(
                Some(&object.id),
                &self.parameter_group_from_payload(&object.id, payload.json_ref()?)?,
            )?;
            return Ok(None);
        }
        let mut edit = self
            .session
            .begin_owned_edit("Edit object properties", None)?;
        self.apply_properties(&mut edit, object, patch, payload)?;
        Ok(Some(edit))
    }

    pub fn set_object_properties(
        &mut self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<()> {
        let prepared = self.prepare_object_properties(object, patch)?;
        self.set_prepared_properties(object, patch, prepared)
    }

    pub(crate) fn set_prepared_properties(
        &mut self,
        object: &ObjectRef,
        patch: &PropertyPatch,
        prepared: Option<EditSession<'static>>,
    ) -> Result<()> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.set_properties");
        if patch
            .changes
            .iter()
            .all(|(id, value)| patch.expected.get(id) == Some(value))
        {
            return Ok(());
        }
        self.finish_edit()?;
        // Ending another owner may change the keyform scope or field baseline
        // without advancing the SDK version (for example, preview navigation).
        self.validate_property_baseline(object, patch)?;
        let prepared = if prepared
            .as_ref()
            .is_some_and(|edit| edit.base_version() != self.session.version())
        {
            self.prepare_object_properties(object, patch)?
        } else {
            prepared
        };
        let Some(edit) = prepared else {
            let payload = self.checked_payload(object, patch)?;
            let draft = self.parameter_group_from_payload(&object.id, payload.json_ref()?)?;
            return self.update_parameter_group(&object.id, draft, self.parameter_model_revision());
        };
        let preserves_structure = patch.preserves_structure()
            && (object.kind != ObjectKind::Parameter || {
                let old = self.session.document().get_parameter(&object.id).unwrap();
                let next = edit.candidate_document().get_parameter(&object.id).unwrap();
                old.name == next.name && old.runtime_id == next.runtime_id
            });
        let previous_revision = self.projection_revision;
        let receipt = edit.commit_to(&mut self.session)?;
        if matches!(
            object.kind,
            ObjectKind::Expression
                | ObjectKind::ExpressionEntry
                | ObjectKind::Motion
                | ObjectKind::MotionGroup
                | ObjectKind::MotionRegistration
                | ObjectKind::PhysicsGroup
                | ObjectKind::Physics
                | ObjectKind::Pose
                | ObjectKind::PoseGroup
                | ObjectKind::PoseEntry
                | ObjectKind::PoseLink
        ) || (!receipt.changes.scene_fields.is_empty()
            && receipt
                .changes
                .scene_fields
                .iter()
                .all(|f| f.field == kasane_sdk::SceneField::Label))
        {
            if receipt.changed {
                self.pristine_blank = false;
            }
            self.consume_authoring_receipt(&receipt, false);
            return Ok(());
        }
        self.session.drain_events();
        if receipt.changed {
            self.pristine_blank = false;
            let presentation_only = patch.mesh_presentation_only(object.kind);
            use crate::property::mesh;
            self.invalidate_projection(!preserves_structure);
            if object.kind == ObjectKind::Mesh && patch.changes.contains_key(&mesh::NAME.key) {
                if let Some(topology) = self.canvas_topology.get_mut().unwrap().as_mut() {
                    let name = &self.session.document().get_mesh(&object.id).unwrap().name;
                    Arc::make_mut(topology).rename(object, name);
                }
            }
            if presentation_only {
                let mut cache = self.object_index.lock().unwrap();
                if let Some((revision, index)) = cache.as_mut() {
                    if *revision == previous_revision && {
                        #[cfg(feature = "bench-instrumentation")]
                        let _benchmark_cow =
                            kasane_core::benchmark::span(if Arc::strong_count(index) > 1 {
                                "editor.object_index_cow_clone"
                            } else {
                                "editor.object_index_unique"
                            });
                        Arc::make_mut(index)
                    }
                    .refresh_mesh_presentation(self.session.document(), object)
                    {
                        *revision = self.projection_revision;
                    }
                }
            }
            if preserves_structure && !presentation_only {
                self.refresh_property_index(previous_revision, object, patch.affects_draw_groups());
            }
            if object.kind == ObjectKind::Parameter {
                self.reconcile_parameter_preview()?;
            }
            let index = self.object_index();
            self.context.reconcile(&index);
        }
        Ok(())
    }

    pub(super) fn refresh_property_index(
        &self,
        previous_revision: u64,
        object: &ObjectRef,
        draw_groups_changed: bool,
    ) {
        let mut cache = self.object_index.lock().unwrap();
        if let Some((revision, index)) = cache.as_mut() {
            if *revision == previous_revision && {
                #[cfg(feature = "bench-instrumentation")]
                let _benchmark_cow =
                    kasane_core::benchmark::span(if Arc::strong_count(index) > 1 {
                        "editor.object_index_cow_clone"
                    } else {
                        "editor.object_index_unique"
                    });
                Arc::make_mut(index).refresh_property_preview(
                    self.model(),
                    object,
                    draw_groups_changed,
                )
            } {
                *revision = self.projection_revision;
            }
        }
    }

    pub(crate) fn property_edit_serial(&self) -> Option<u64> {
        self.pending.as_ref().and_then(|p| match p.kind {
            Gesture::Properties(serial) => Some(serial),
            _ => None,
        })
    }

    pub(crate) fn property_edit_matches(&self, serial: u64, object: &ObjectRef) -> bool {
        self.property_edit_serial() == Some(serial)
            && self.pending.as_ref().is_some_and(|pending| {
                let PendingOriginal::Properties(target, _, scope) = &pending.original else {
                    return false;
                };
                target == object
                    && match scope {
                        PropertyScope::Object => true,
                        // A resource inspector also versions its Clip/Expression fields,
                        // whose default standalone scope is Object.
                        PropertyScope::Resource { revision } => {
                            *revision == self.model_generation()
                        }
                        _ => *scope == self.property_scope(object),
                    }
            })
    }

    pub(crate) fn validate_property_preview(
        &self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<()> {
        ensure!(
            patch.changes.len() == 1,
            "Preview edits one property at a time"
        );
        ensure!(
            patch
                .changes
                .keys()
                .all(|id| crate::property_spec(*id)
                    .is_some_and(|s| s.key.object_kind() == object.kind && s.preview)),
            "This property does not support preview"
        );
        self.validate_object_properties(object, patch)
    }

    pub(crate) fn begin_property_edit(
        &mut self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<u64> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.begin_property_candidate");
        self.validate_property_preview(object, patch)?;
        self.finish_edit()?;
        let original = Self::property_values_in(self.session.document(), object, &patch.scope)?;
        self.next_property_edit += 1;
        let serial = self.next_property_edit;
        self.pending = Some(PendingEdit {
            kind: Gesture::Properties(serial),
            original: PendingOriginal::Properties(object.clone(), original, patch.scope.clone()),
            edit: self
                .session
                .begin_owned_edit("Edit object property", None)?,
            preview: PreviewState::default(),
            changed: OnceLock::new(),
            topology_changed: !patch.preserves_structure(),
            draw_groups_changed: patch.affects_draw_groups(),
            property_write: patch
                .changes
                .keys()
                .next()
                .and_then(|key| crate::property_spec(*key))
                .map(|s| s.write),
        });
        Ok(serial)
    }

    pub(crate) fn update_property_edit(
        &mut self,
        object: &ObjectRef,
        serial: u64,
        patch: &PropertyPatch,
    ) -> Result<()> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.update_property_candidate");
        ensure!(
            self.property_edit_matches(serial, object),
            "Property edit ended"
        );
        ensure!(
            patch
                .changes
                .keys()
                .all(|id| crate::property_spec(*id)
                    .is_some_and(|s| s.key.object_kind() == object.kind && s.preview)),
            "This property does not support preview"
        );
        let payload = self.checked_payload(object, patch)?;
        if let PropertyPayload::Expression(kasane_core::document::ExpressionEdit::PatchEntry {
            id,
            fields,
            ..
        }) = payload
        {
            let owner = self
                .session
                .document()
                .get_expression_entry(&id)
                .unwrap()
                .0
                .id
                .clone();
            let pending = self.pending.as_mut().unwrap();
            pending.edit.preview_expression_entry(&id, fields)?;
            pending.changed.take();
            let previous = self.projection_revision;
            self.invalidate_expression_projection();
            self.refresh_expression_index(previous, &[owner]);
            return Ok(());
        }

        if let PropertyPayload::Physics(patch) = payload {
            // Local SDK patches are atomic. Validate before touching the owned edit
            // so invalid text preserves the last good candidate and its token.
            let pending = self.pending.as_mut().unwrap();
            let status = pending
                .edit
                .candidate_document()
                .validate_physics_patch(&patch);
            ensure!(status.is_ok(), "{}: {}", status.code, status.message);
            pending.edit.patch_physics(&patch)?;
            pending.changed.take();
            let previous = self.projection_revision;
            self.projection_revision += 1;
            if let Some((revision, index)) = self.object_index.get_mut().unwrap() {
                if *revision == previous && {
                    #[cfg(feature = "bench-instrumentation")]
                    let _benchmark_cow =
                        kasane_core::benchmark::span(if Arc::strong_count(index) > 1 {
                            "editor.object_index_cow_clone"
                        } else {
                            "editor.object_index_unique"
                        });
                    Arc::make_mut(index)
                }
                .refresh_property_preview(
                    pending.edit.candidate_document(),
                    object,
                    false,
                ) {
                    *revision = self.projection_revision;
                }
            }
            return Ok(());
        }
        let keyform_order_changed =
            self.pending.as_ref().unwrap().draw_groups_changed || patch.affects_draw_groups();
        let previous_revision = self.projection_revision;
        if self.pending.as_ref().unwrap().property_write
            == Some(crate::property::PropertyWrite::ScalarPatch)
        {
            let scalar = self.scalar_patch(object, patch, payload.json_ref()?)?;
            let pending = self.pending.as_mut().unwrap();
            pending.edit.preview_property(
                &scalar,
                &mut pending.preview,
                self.session.preview_values().clone(),
            )?;
        } else {
            // Preview capability does not require a scalar backend. Full object
            // writes retain isolated replacement and atomic preview validation.
            let mut edit = self
                .session
                .begin_owned_edit("Edit object property", None)?;
            self.apply_properties(&mut edit, object, patch, payload)?;
            let mut preview = PreviewState::default();
            preview
                .replace(
                    edit.candidate_document(),
                    0,
                    self.session.preview_values().clone(),
                )
                .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
            let pending = self.pending.as_mut().unwrap();
            pending.edit = edit;
            pending.preview = preview;
        }
        let pending = self.pending.as_mut().unwrap();
        pending.changed.take();
        pending.draw_groups_changed |= keyform_order_changed;
        pending.topology_changed |= !patch.preserves_structure();
        let structure_changed = pending.topology_changed;
        self.invalidate_projection(structure_changed);
        if !structure_changed {
            self.refresh_property_index(previous_revision, object, keyform_order_changed);
        }
        Ok(())
    }
}

fn replace(edit: &mut EditSession<'_>, kind: ObjectKind, payload: serde_json::Value) -> Result<()> {
    macro_rules! replace {
        ($method:ident) => {
            edit.$method(serde_json::from_value(payload)?)?
        };
    }
    match kind {
        ObjectKind::Document => {
            edit.replace_export_settings(serde_json::from_value(
                payload["export_settings"].clone(),
            )?)?;
        }
        ObjectKind::Part => replace!(replace_part),
        ObjectKind::Parameter => replace!(replace_parameter),
        ObjectKind::Mesh => replace!(replace_mesh),
        ObjectKind::Deformer => replace!(replace_transform),
        ObjectKind::Glue => replace!(replace_glue),
        ObjectKind::Offscreen => replace!(replace_offscreen),
        ObjectKind::Texture => replace!(replace_asset),
        ObjectKind::Motion => replace!(replace_motion),
        ObjectKind::Expression => replace!(replace_expression),
        _ => bail!("This property is read only"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewParameter, PropertyValue};
    use std::sync::atomic::Ordering;

    #[test]
    fn physics_field_candidates_reuse_indexes_and_keep_scalar_preview_snapshots_independent()
    -> Result<()> {
        use crate::property::physics as schema;
        let mut doc = Document::new_blank()?;
        let asset_id = uuid::Uuid::new_v4().to_string();
        let mut asset = kasane_core::document::PhysicsAsset::empty(asset_id.clone());
        let group_id = uuid::Uuid::new_v4().to_string();
        let other_id = uuid::Uuid::new_v4().to_string();
        for (id, name) in [(&group_id, "Rig"), (&other_id, "Other")] {
            let group = kasane_core::document::PhysicsAsset::default_group(id.clone(), name.into());
            asset.data.settings.push(Arc::new(group));
            asset
                .data
                .meta
                .dictionary
                .push(kasane_core::physics::PhysicsDictionaryEntry {
                    id: id.clone(),
                    name: name.into(),
                    extensions: Default::default(),
                });
        }
        asset.reconcile_counts();
        let node_id = asset.data.settings[0].vertices[1].id.clone();
        doc.session
            .edit("Fixture", None, |edit| edit.set_physics(asset))?;
        doc.invalidate_projection(true);
        let object = ObjectRef::new(ObjectKind::PhysicsGroup, &group_id);
        let initial = doc.property_values(&object)?;
        let key = schema::MOBILITY.key.for_entity(&node_id);
        let patch = |value| PropertyPatch {
            scope: PropertyScope::Object,
            expected: [(key, initial[&key].clone())].into(),
            changes: [(key, PropertyValue::Float(value))].into(),
        };
        let serial = doc.begin_property_edit(&object, &patch(0.2))?;
        let snapshot = doc.object_index();
        let old = snapshot.get(&object).unwrap().properties.clone();
        let builds = doc.object_index_builds.load(Ordering::Relaxed);
        let topology = doc.canvas_topology();
        for value in [0.2, 0.4, 0.7] {
            doc.update_property_edit(&object, serial, &patch(value))?;
            let index = doc.object_index();
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            assert!(Arc::ptr_eq(&topology, &doc.canvas_topology()));
            assert_eq!(snapshot.get(&object).unwrap().properties, old);
            for info in snapshot
                .objects
                .iter()
                .filter(|info| info.reference != object && info.reference.id != node_id)
            {
                assert!(std::ptr::eq(info, index.get(&info.reference).unwrap()));
            }
            let rebuilt = crate::ObjectIndex::build(doc.model());
            assert_eq!(
                index.get(&object).unwrap().properties,
                rebuilt.get(&object).unwrap().properties
            );
            assert_eq!(doc.property_values(&object)?[&key], initial[&key]);
        }
        doc.cancel_edit();
        assert_eq!(doc.property_values(&object)?, initial);
        assert_eq!(doc.object_index().get(&object).unwrap().properties, old);
        assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
        for finish in [false, true] {
            let patch = doc.object_property_patch(&object, "fps", &serde_json::json!(60.))?;
            let serial = doc.begin_property_edit(&object, &patch)?;
            doc.update_property_edit(&object, serial, &patch)?;
            let other = ObjectRef::new(ObjectKind::PhysicsGroup, &other_id);
            assert_eq!(
                doc.object_index().get(&other).unwrap().properties["fps"],
                60.
            );
            if finish {
                doc.finish_edit()?;
            } else {
                doc.cancel_edit();
            }
            let index = doc.object_index();
            let rebuilt = crate::ObjectIndex::build(doc.model());
            for target in [
                &object,
                &other,
                &ObjectRef::new(ObjectKind::Physics, &asset_id),
            ] {
                assert_eq!(
                    index.get(target).unwrap().properties,
                    rebuilt.get(target).unwrap().properties
                );
            }
            assert_eq!(snapshot.get(&object).unwrap().properties, old);
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            assert!(Arc::ptr_eq(&topology, &doc.canvas_topology()));
        }
        Ok(())
    }

    #[test]
    fn bound_mesh_opacity_text_and_preview_use_local_payload_publication() -> Result<()> {
        let mut doc = Document::new_blank()?;
        let object = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::new_v4().to_string());
        let peer = uuid::Uuid::new_v4().to_string();
        let parameter = uuid::Uuid::new_v4().to_string();
        let binding = uuid::Uuid::new_v4().to_string();
        let peer_binding = uuid::Uuid::new_v4().to_string();
        doc.session.edit("Bound Mesh fixture", None, |edit| {
            edit.create_parameter(kasane_core::Parameter {
                id: parameter.clone(),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })?;
            for (mesh, binding) in [(&object.id, &binding), (&peer, &peer_binding)] {
                let positions = vec![
                    kasane_core::Vec2::new(0., 0.),
                    kasane_core::Vec2::new(1., 0.),
                    kasane_core::Vec2::new(0., 1.),
                ];
                edit.create_mesh(kasane_core::Mesh {
                    id: mesh.clone(),
                    name: "Bound Mesh".into(),
                    vertex_ids: vec![0, 1, 2],
                    base_positions: positions.clone(),
                    uvs: vec![kasane_core::Vec2::new(0., 0.); 3],
                    triangles: vec![[0, 1, 2]],
                    ..Default::default()
                })?;
                edit.create_binding(kasane_core::MeshBinding {
                    id: binding.clone(),
                    mesh_id: mesh.clone(),
                    axes: vec![kasane_core::BindingAxis::new(
                        parameter.clone(),
                        vec![0., 1.],
                    )],
                    keyforms: [0., 1.]
                        .into_iter()
                        .map(|key| kasane_core::MeshKeyform {
                            keys: vec![key],
                            positions: positions.clone(),
                            ..Default::default()
                        })
                        .collect(),
                })?;
            }
            Ok(())
        })?;
        doc.invalidate_projection(true);
        assert!(matches!(
            doc.property_scope(&object),
            PropertyScope::MeshKeyform { .. }
        ));
        let retained = doc.session.read_snapshot();
        for preview in [false, true] {
            let old = doc.object_index();
            let old_payload = old.get(&object).unwrap().properties.clone();
            let builds = doc.object_index_builds.load(Ordering::Relaxed);
            let history = doc.session.history_lengths().0;
            #[cfg(feature = "bench-instrumentation")]
            let capture = kasane_core::benchmark::Capture::start();
            if preview {
                let initial = doc.object_property_patch(
                    &object,
                    "appearance.opacity",
                    &serde_json::json!(0.4),
                )?;
                let serial = doc.begin_property_edit(&object, &initial)?;
                for value in [0.25, 0.5, 0.4] {
                    let patch = doc.object_property_patch(
                        &object,
                        "appearance.opacity",
                        &serde_json::json!(value),
                    )?;
                    doc.update_property_edit(&object, serial, &patch)?;
                }
                doc.finish_edit()?;
            } else {
                doc.set_object_property(&object, "appearance.opacity", serde_json::json!(0.4))?;
            }
            #[cfg(feature = "bench-instrumentation")]
            {
                let work = capture.finish();
                assert_eq!(work["sdk.publication.local"].calls, 1);
                assert_eq!(work["sdk.binding_summary.record_reads"].calls, 2);
                assert_eq!(work["sdk.binding_summary.derived_meshes"].calls, 1);
                assert_eq!(work["core.validate_structure.full"].calls, 1);
                assert!(!work.contains_key("core.validate_structure.local"));
                for key in [
                    "sdk.publication.fallback",
                    "sdk.keys_in",
                    "core.authoring_identity_scan",
                    "sdk.summarize.fallback",
                ] {
                    assert!(!work.contains_key(key), "unexpected {key}: {work:?}");
                }
            }
            assert_eq!(doc.session.history_lengths().0, history + 1);
            assert_eq!(
                doc.session
                    .document()
                    .get_binding(&binding)
                    .unwrap()
                    .keyforms[0]
                    .appearance
                    .opacity,
                0.4
            );
            assert_eq!(
                doc.session
                    .document()
                    .get_mesh(&object.id)
                    .unwrap()
                    .appearance
                    .opacity,
                1.
            );
            assert_eq!(
                doc.session.document().get_binding(&peer_binding),
                retained.document().get_binding(&peer_binding)
            );
            assert_eq!(
                retained.document().get_binding(&binding).unwrap().keyforms[0]
                    .appearance
                    .opacity,
                1.
            );
            assert_eq!(old.get(&object).unwrap().properties, old_payload);
            let index = doc.object_index();
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            let rebuilt = crate::ObjectIndex::build(doc.model());
            assert_eq!(
                index.get(&object).unwrap().properties,
                rebuilt.get(&object).unwrap().properties
            );
            assert_eq!(
                format!("{:?}", index.bindings),
                format!("{:?}", rebuilt.bindings)
            );
            assert_eq!(
                format!("{:?}", index.relations),
                format!("{:?}", rebuilt.relations)
            );
            doc.undo()?;
            assert_eq!(
                doc.session
                    .document()
                    .get_binding(&binding)
                    .unwrap()
                    .keyforms[0]
                    .appearance
                    .opacity,
                1.
            );
        }
        Ok(())
    }

    #[test]
    fn mesh_presentation_commits_preserve_index_snapshots_and_relations() -> Result<()> {
        let mut doc = Document::new_blank()?;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("texture.png");
        std::fs::write(&path, include_bytes!("../../tests/fixtures/accent.png"))?;
        let asset = uuid::Uuid::new_v4().to_string();
        let object = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::new_v4().to_string());
        doc.session.edit("Mesh fixture", None, |edit| {
            edit.create_asset(kasane_sdk::prepare_png_asset(&asset, "Texture", &path)?)?;
            edit.create_mesh(kasane_core::Mesh {
                id: object.id.clone(),
                name: "Original".into(),
                texture_asset_id: asset.into(),
                vertex_ids: vec![0, 1, 2],
                base_positions: vec![
                    kasane_core::Vec2::new(0., 0.),
                    kasane_core::Vec2::new(1., 0.),
                    kasane_core::Vec2::new(0., 1.),
                ],
                uvs: vec![kasane_core::Vec2::new(0., 0.); 3],
                triangles: vec![[0, 1, 2]],
                ..Default::default()
            })
        })?;
        doc.invalidate_projection(true);
        for (field, value) in [
            ("name", serde_json::json!("Renamed")),
            ("enabled", serde_json::json!(false)),
            ("appearance.opacity", serde_json::json!(0.5)),
        ] {
            let old = doc.object_index();
            let old_payload = old.get(&object).unwrap().properties.clone();
            let builds = doc.object_index_builds.load(Ordering::Relaxed);
            let topology = doc.canvas_topology();
            #[cfg(feature = "bench-instrumentation")]
            let capture = kasane_core::benchmark::Capture::start();
            doc.set_object_property(&object, field, value)?;
            #[cfg(feature = "bench-instrumentation")]
            {
                let stages = capture.finish();
                assert_eq!(stages["core.validate_structure.local"].calls, 1);
                assert_eq!(stages["core.validate_structure.local.mesh_checks"].calls, 1);
                assert!(
                    !stages.contains_key("core.validate_structure.full"),
                    "{field}: {stages:?}"
                );
                assert!(
                    !stages.contains_key("sdk.keys_in"),
                    "Label patch enumerated identities: {stages:?}"
                );
            }
            let index = doc.object_index();
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            assert_eq!(old.get(&object).unwrap().properties, old_payload);
            // Only the edited record detaches; large descriptor payloads,
            // relation records, bindings and lookup paths stay shared.
            for info in old.objects.iter().filter(|info| info.reference != object) {
                assert!(std::ptr::eq(info, index.get(&info.reference).unwrap()));
            }
            assert_eq!(old.bindings.as_ptr(), index.bindings.as_ptr());
            for (before, after) in old.relations.iter().zip(&index.relations) {
                assert!(std::ptr::eq(before, after));
            }
            let rebuilt = crate::ObjectIndex::build(doc.model());
            let actual = index.get(&object).unwrap();
            let expected = rebuilt.get(&object).unwrap();
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.enabled, expected.enabled);
            assert_eq!(actual.properties, expected.properties);
            assert_eq!(index.draw_groups, rebuilt.draw_groups);
            assert_eq!(
                format!("{:?}", index.relations),
                format!("{:?}", rebuilt.relations)
            );
            assert_eq!(
                Arc::ptr_eq(&topology, &doc.canvas_topology()),
                field != "name"
            );
            let committed = actual.properties.clone();
            doc.undo()?;
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties,
                old_payload
            );
            doc.redo()?;
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties,
                committed
            );
        }
        // A mixed presentation/order submission takes the generic refresh path.
        let old_index = doc.object_index();
        let old_topology = doc.canvas_topology();
        let old = old_index.get(&object).unwrap();
        let old_payload = old.properties.clone();
        let patch = PropertyPatch {
            scope: PropertyScope::Object,
            expected: doc.property_values(&object)?,
            changes: [
                (
                    crate::property::mesh::NAME.key,
                    crate::PropertyValue::Text("Mixed update".into()),
                ),
                (
                    crate::property::mesh::ENABLED.key,
                    crate::PropertyValue::Bool(true),
                ),
                (
                    crate::property::mesh::DRAW_ORDER.key,
                    crate::PropertyValue::OptionalFloat(Some(42.)),
                ),
            ]
            .into(),
        };
        doc.set_object_properties(&object, &patch)?;
        let rebuilt = crate::ObjectIndex::build(doc.model());
        let index = doc.object_index();
        let actual = index.get(&object).unwrap();
        let expected = rebuilt.get(&object).unwrap();
        assert_eq!(actual.name, expected.name);
        assert_eq!(actual.enabled, expected.enabled);
        assert_eq!(actual.properties, expected.properties);
        assert_eq!(index.draw_groups, rebuilt.draw_groups);
        assert_eq!(doc.canvas_topology().index.name(&object), "Mixed update");
        assert_eq!(old_index.get(&object).unwrap().properties, old_payload);
        assert_eq!(old_topology.index.name(&object), old.name);
        doc.undo()?;
        assert_eq!(
            doc.object_index().get(&object).unwrap().properties,
            old_payload
        );
        doc.redo()?;
        assert_eq!(
            doc.object_index().get(&object).unwrap().properties,
            expected.properties
        );
        assert_eq!(doc.canvas_topology().index.name(&object), "Mixed update");
        Ok(())
    }

    #[test]
    fn numeric_candidates_refresh_payloads_without_rebuilding_indexes() -> Result<()> {
        let mut doc = Document::new_blank()?;
        let parameter = ObjectRef::new(
            ObjectKind::Parameter,
            doc.create_parameter(NewParameter::default())?,
        );
        let part = ObjectRef::new(ObjectKind::Part, uuid::Uuid::new_v4().to_string());
        doc.session.edit("Part fixture", None, |edit| {
            edit.create_part(kasane_core::Part {
                id: part.id.clone(),
                name: "Part".into(),
                ..Default::default()
            })
        })?;
        doc.invalidate_projection(true);
        for (object, spec) in [
            (parameter, crate::property::parameter::DEFAULT_VALUE),
            (part, crate::property::part::DRAW_ORDER),
        ] {
            let field = spec.key;
            let baseline = doc.property_values(&object)?;
            let initial = doc.object_property_patch(&object, spec.path, &serde_json::json!(0.))?;
            let serial = doc.begin_property_edit(&object, &initial)?;
            let topology = doc.canvas_topology();
            // Keep an old snapshot alive too: copy-on-write must preserve it.
            let old = doc.object_index();
            let old_payload = old.get(&object).unwrap().properties.clone();
            let builds = doc.object_index_builds.load(Ordering::Relaxed);
            for sample in 1..=12 {
                let value = sample as f32 / 16.;
                let patch =
                    doc.object_property_patch(&object, spec.path, &serde_json::json!(value))?;
                doc.update_property_edit(&object, serial, &patch)?;
                let index = doc.object_index();
                assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
                assert!(Arc::ptr_eq(&topology, &doc.canvas_topology()));
                assert_eq!(old.get(&object).unwrap().properties, old_payload);
                let rebuilt = crate::ObjectIndex::build(doc.model());
                assert_eq!(
                    index.get(&object).unwrap().properties,
                    rebuilt.get(&object).unwrap().properties
                );
                assert_eq!(index.draw_groups, rebuilt.draw_groups);
                assert_eq!(
                    format!("{:?}", index.relations),
                    format!("{:?}", rebuilt.relations)
                );
                assert_eq!(doc.property_values(&object)?, baseline);
                if field == crate::property::parameter::DEFAULT_VALUE.key {
                    assert_eq!(
                        index.get(&object).unwrap().properties["default_value"],
                        serde_json::json!(value)
                    );
                }
            }
            doc.cancel_edit();
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties,
                old_payload
            );
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            assert!(Arc::ptr_eq(&topology, &doc.canvas_topology()));
            let patch = doc.object_property_patch(&object, spec.path, &serde_json::json!(0.5))?;
            let serial = doc.begin_property_edit(&object, &patch)?;
            doc.update_property_edit(&object, serial, &patch)?;
            doc.finish_edit()?;
            let rebuilt = crate::ObjectIndex::build(doc.model());
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties,
                rebuilt.get(&object).unwrap().properties
            );
            assert_eq!(doc.object_index().draw_groups, rebuilt.draw_groups);
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            assert!(Arc::ptr_eq(&topology, &doc.canvas_topology()));
            assert_eq!(
                doc.property_values(&object)?[&field],
                PropertyValue::Float(0.5)
            );
            doc.undo()?;
            assert_eq!(doc.property_values(&object)?, baseline);
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties,
                old_payload
            );
        }
        Ok(())
    }
}
