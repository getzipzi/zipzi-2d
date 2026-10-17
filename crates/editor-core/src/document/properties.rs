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
            .find(|s| s.id.path() == field)
            .ok_or_else(|| anyhow!("This property is read only"))?;
        let current = self.property_values(object)?;
        let expected = crate::property_specs(object.kind)
            .iter()
            .filter(|s| s.group == spec.group)
            .map(|s| (s.id, current[&s.id].clone()))
            .collect();
        Ok(PropertyPatch {
            expected,
            changes: [(spec.id, spec.decode(value)?)].into(),
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
                id.path()
            );
        }
        for (id, value) in &patch.changes {
            let spec = specs
                .iter()
                .find(|s| s.id == *id)
                .ok_or_else(|| anyhow!("This property is read only"))?;
            spec.validate(value)?;
            ensure!(
                specs
                    .iter()
                    .filter(|s| s.group == spec.group)
                    .all(|s| patch.expected.contains_key(&s.id)),
                "Missing property baseline"
            );
        }
        let mut payload = crate::property::object_payload(self.session.document(), object)?;
        for (id, value) in &patch.changes {
            if let Some(field) = id.path().strip_prefix("canvas.") {
                payload["canvas"][field] = value.json();
            } else {
                payload[id.path()] = value.json();
            }
        }
        Ok(payload)
    }

    fn parameter_from_payload(
        &self,
        object: &ObjectRef,
        payload: serde_json::Value,
    ) -> Result<ParameterDraft> {
        let p: kasane_core::Parameter = serde_json::from_value(payload)?;
        let mut draft = self.parameter_draft(&object.id)?;
        draft.name = p.name;
        draft.runtime_id = p.runtime_id;
        draft.minimum = p.minimum;
        draft.maximum = p.maximum;
        draft.default_value = p.default_value;
        draft.decimal_places = p.decimal_places;
        draft.repeat = p.repeat;
        self.validate_parameter_update(&object.id, &draft)?;
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
                    .any(|s| s.id == *id && s.preview)),
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
                    .any(|s| s.id == *id && s.preview)),
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
        let pending = self.pending.as_mut().unwrap();
        pending.edit = edit;
        pending.preview = preview;
        pending.changed.take();
        self.invalidate_projection(true);
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
            edit.replace_canvas(serde_json::from_value(payload["canvas"].clone())?)?;
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
