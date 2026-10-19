//! Atomic property edits and owned inspector candidates.
use super::*;
use crate::{ObjectKind, ObjectRef, PropertyPatch, PropertyScope, PropertyValues};
use anyhow::bail;

impl Document {
    /// Committed values remain stable while an inspector candidate is displayed.
    pub fn property_values(&self, object: &ObjectRef) -> Result<PropertyValues> {
        Self::property_values_in(
            self.session.document(),
            object,
            &self.property_scope(object),
        )
    }

    pub fn property_scope(&self, object: &ObjectRef) -> PropertyScope {
        if object.kind != ObjectKind::Mesh {
            return PropertyScope::Object;
        }
        let model = self.session.document();
        let Some(binding) = model.binding_for_mesh(&object.id) else {
            return PropertyScope::Object;
        };
        let form = binding.keyforms.iter().find(|form| {
            binding.axes.len() == form.keys.len()
                && binding.axes.iter().zip(&form.keys).all(|(axis, key)| {
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
                                let wrapped =
                                    (normalized - normalized.floor()) * range + parameter.minimum;
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
                keys: f.keys.iter().map(|v| v.to_bits()).collect(),
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

    fn checked_payload(
        &self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<serde_json::Value> {
        self.ensure_editable(std::slice::from_ref(object))?;
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
            let spec = specs
                .iter()
                .find(|s| s.key == *id)
                .ok_or_else(|| anyhow!("This property is read only"))?;
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
                    .filter(|s| s.same_group(spec))
                    .all(|s| patch.expected.contains_key(&s.key)),
                "Missing property baseline"
            );
        }
        let mut payload = if object.kind == ObjectKind::Mesh {
            if patch.mesh_presentation_only(object.kind) {
                crate::property::mesh::inspector_payload(
                    self.session.document(),
                    &object.id,
                    &patch.scope,
                )?
            } else {
                crate::property::mesh::payload(self.session.document(), &object.id, &patch.scope)?
            }
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
        Ok(payload)
    }

    fn apply_properties(
        &self,
        edit: &mut EditSession<'_>,
        object: &ObjectRef,
        patch: &PropertyPatch,
        payload: serde_json::Value,
    ) -> Result<()> {
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
        self.validate_parameter_update(&object.id, &draft)?;
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
        let payload = self.checked_payload(object, patch)?;
        if object.kind == ObjectKind::Parameter {
            self.parameter_from_payload(object, payload)?;
        } else if object.kind == ObjectKind::ParameterGroup {
            self.validate_parameter_group(
                Some(&object.id),
                &self.parameter_group_from_payload(&object.id, &payload)?,
            )?;
        } else {
            let mut edit = self.session.begin_owned_edit("Validate properties", None)?;
            self.apply_properties(&mut edit, object, patch, payload)?;
        }
        Ok(())
    }

    pub fn set_object_properties(
        &mut self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<()> {
        // Validate before ending another edit: a rejected patch has no side effects.
        self.validate_object_properties(object, patch)?;
        if patch
            .changes
            .iter()
            .all(|(id, value)| patch.expected.get(id) == Some(value))
        {
            return Ok(());
        }
        self.finish_edit()?;
        let payload = self.checked_payload(object, patch)?;
        if object.kind == ObjectKind::Parameter {
            let draft = self.parameter_from_payload(object, payload)?;
            return self.update_parameter(&object.id, draft, self.parameter_model_revision());
        }
        if object.kind == ObjectKind::ParameterGroup {
            let draft = self.parameter_group_from_payload(&object.id, &payload)?;
            return self.update_parameter_group(&object.id, draft, self.parameter_model_revision());
        }
        let mut edit = self
            .session
            .begin_owned_edit("Edit object properties", None)?;
        self.apply_properties(&mut edit, object, patch, payload)?;
        let previous_revision = self.projection_revision;
        let receipt = edit.commit_to(&mut self.session)?;
        self.session.drain_events();
        if receipt.changed {
            self.pristine_blank = false;
            let presentation_only = patch.mesh_presentation_only(object.kind);
            use crate::property::mesh;
            self.invalidate_projection(!presentation_only);
            if presentation_only {
                if patch.changes.contains_key(&mesh::NAME.key) {
                    if let Some(topology) = self.canvas_topology.get_mut().unwrap().as_mut() {
                        let name = &self.session.document().get_mesh(&object.id).unwrap().name;
                        Arc::make_mut(topology).rename(object, name);
                    }
                }
                let mut cache = self.object_index.lock().unwrap();
                if let Some((revision, index)) = cache.as_mut() {
                    if *revision == previous_revision
                        && Arc::make_mut(index)
                            .refresh_mesh_presentation(self.session.document(), object)
                    {
                        *revision = self.projection_revision;
                    }
                }
            }
            let index = self.object_index();
            self.context.reconcile(&index);
        }
        Ok(())
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
                matches!(&pending.original,
                PendingOriginal::Properties(target, _, scope) if target == object && (*scope == PropertyScope::Object || *scope == self.property_scope(object)))
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
                .all(|id| crate::property_specs(object.kind)
                    .iter()
                    .any(|s| s.key == *id && s.preview)),
            "This property does not support preview"
        );
        self.validate_object_properties(object, patch)
    }

    pub(crate) fn begin_property_edit(
        &mut self,
        object: &ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<u64> {
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
            topology_changed: true,
        });
        Ok(serial)
    }

    pub(crate) fn update_property_edit(
        &mut self,
        object: &ObjectRef,
        serial: u64,
        patch: &PropertyPatch,
    ) -> Result<()> {
        ensure!(
            self.property_edit_matches(serial, object),
            "Property edit ended"
        );
        ensure!(
            patch
                .changes
                .keys()
                .all(|id| crate::property_specs(object.kind)
                    .iter()
                    .any(|s| s.key == *id && s.preview)),
            "This property does not support preview"
        );
        let payload = self.checked_payload(object, patch)?;
        // Each update starts from committed data; rejected candidates never damage
        // the last good preview or become the basis of the next update.
        let mut edit = self
            .session
            .begin_owned_edit("Edit object property", None)?;
        self.apply_properties(&mut edit, object, patch, payload)?;
        let keyform_order = |model: &kasane_core::Document| {
            crate::property::mesh::keyform(model, &object.id, &patch.scope)
                .map(|form| form.map(|(_, form)| form.draw_order))
        };
        let keyform_order_changed = keyform_order(edit.candidate_document())?
            != keyform_order(self.pending.as_ref().unwrap().edit.candidate_document())?;
        let mut preview = PreviewState::default();
        preview
            .replace(
                edit.candidate_document(),
                0,
                self.session.preview_values().clone(),
            )
            .map_err(|status| anyhow!("{}: {}", status.code, status.message))?;
        let previous_revision = self.projection_revision;
        let pending = self.pending.as_mut().unwrap();
        pending.edit = edit;
        pending.preview = preview;
        pending.changed.take();
        // Scalar candidates do not change canvas topology or relation edges.
        self.projection_revision += 1;
        if let Some((revision, index)) = self.object_index.get_mut().unwrap() {
            if *revision == previous_revision
                && Arc::make_mut(index).refresh_property_preview(
                    pending.edit.candidate_document(),
                    object,
                    keyform_order_changed,
                )
            {
                *revision = self.projection_revision;
            }
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
                texture_asset_id: asset,
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
            doc.set_object_property(&object, field, value)?;
            let index = doc.object_index();
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            assert_eq!(old.get(&object).unwrap().properties, old_payload);
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
            let patch = doc.object_property_patch(&object, spec.path, &serde_json::json!(0.5))?;
            let serial = doc.begin_property_edit(&object, &patch)?;
            doc.update_property_edit(&object, serial, &patch)?;
            doc.finish_edit()?;
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
