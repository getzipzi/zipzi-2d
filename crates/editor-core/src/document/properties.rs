//! Atomic scalar edits and owned inspector candidates.
use super::*;
use crate::{ObjectKind, ObjectRef, PropertyPatch, PropertyValues};
use anyhow::bail;

impl Document {
    /// Committed values remain stable while an inspector candidate is displayed.
    pub fn property_values(&self, object: &ObjectRef) -> Result<PropertyValues> {
        crate::property::values_from(self.session.document(), object)
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
            ensure!(
                specs
                    .iter()
                    .filter(|s| s.same_group(spec))
                    .all(|s| patch.expected.contains_key(&s.key)),
                "Missing property baseline"
            );
        }
        let mut payload = crate::property::object_payload(self.session.document(), object)?;
        for (id, value) in &patch.changes {
            let spec = specs
                .iter()
                .find(|spec| spec.key == *id)
                .expect("validated property key");
            spec.write(&mut payload, value)?;
        }
        Ok(payload)
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
            replace(&mut edit, object.kind, payload)?;
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
        replace(&mut edit, object.kind, payload)?;
        let receipt = edit.commit_to(&mut self.session)?;
        self.session.drain_events();
        if receipt.changed {
            self.pristine_blank = false;
            self.invalidate_projection(true);
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
                PendingOriginal::Properties(target, _) if target == object)
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
        let original = self.property_values(object)?;
        self.next_property_edit += 1;
        let serial = self.next_property_edit;
        self.pending = Some(PendingEdit {
            kind: Gesture::Properties(serial),
            original: PendingOriginal::Properties(object.clone(), original),
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
        replace(&mut edit, object.kind, payload)?;
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
                && Arc::make_mut(index)
                    .refresh_property_preview(pending.edit.candidate_document(), object)
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
