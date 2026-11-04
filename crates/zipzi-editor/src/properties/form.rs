//! A single end-of-edit policy for every configured field. Draft text exists
//! only during editing; rejected submissions restore committed values.
use std::collections::{BTreeMap, HashMap};
use zipzi_editor_core::{
    DocumentId, Editor, ObjectKind, PropertyKey, PropertyPatch, PropertySpec, PropertyValue,
    PropertyValues, property_specs,
};

use super::PropertyTarget;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FieldDraft {
    scale: f64,
    display_decimals: Option<usize>,
    pub revision: u64,
    pub baseline: PropertyValue,
    pub raw: String,
    pub parsed: Result<PropertyValue, String>,
    pub touched: bool,
    pub conflict: bool,
}
impl FieldDraft {
    fn new(value: PropertyValue, scale: f64, display_decimals: Option<usize>) -> Self {
        Self {
            scale,
            display_decimals,
            revision: 0,
            raw: display_value(&value, scale, display_decimals),
            parsed: Ok(value.clone()),
            baseline: value,
            touched: false,
            conflict: false,
        }
    }
    pub fn dirty(&self) -> bool {
        self.conflict
            || self
                .parsed
                .as_ref()
                .ok()
                .is_none_or(|value| *value != self.baseline)
    }
}

fn display_value(value: &PropertyValue, scale: f64, decimals: Option<usize>) -> String {
    let text = if scale != 1. {
        value.number().map_or_else(
            || value.text(),
            |n| ((value.text().parse::<f64>().unwrap_or(n) * scale) as f32).to_string(),
        )
    } else {
        value.text()
    };
    decimals.map_or_else(
        || text.clone(),
        |digits| truncate_decimal_text(&text, digits),
    )
}

/// Presentation only. Never turn a small nonzero value into an apparent zero.
pub(crate) fn truncate_decimal_text(text: &str, digits: usize) -> String {
    if text.contains(['e', 'E']) {
        return text.to_owned();
    }
    let Some(dot) = text.find('.') else {
        return text.to_owned();
    };
    let end = text.len().min(dot + 1 + digits);
    let shortened = &text[..end];
    if shortened.parse::<f64>().is_ok_and(|v| v == 0.) && text.parse::<f64>().is_ok_and(|v| v != 0.)
    {
        return text.to_owned();
    }
    let shortened = shortened.trim_end_matches('0').trim_end_matches('.');
    if shortened == "-0" {
        "0".into()
    } else {
        shortened.into()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PropertyForm {
    pub fields: BTreeMap<PropertyKey, FieldDraft>,
    current: PropertyValues,
    specs: Vec<PropertySpec>,
}
impl PropertyForm {
    #[cfg(test)]
    pub fn sync(&mut self, kind: ObjectKind, values: PropertyValues) {
        self.sync_definition(crate::properties::definition(kind), values);
    }
    pub fn sync_definition(
        &mut self,
        definition: crate::properties::FormDefinition,
        values: PropertyValues,
    ) {
        let mut specs: BTreeMap<_, _> = values
            .keys()
            .next()
            .into_iter()
            .flat_map(|key| property_specs(key.object_kind()))
            .map(|spec| (spec.key, *spec))
            .collect();
        specs.extend(
            definition
                .fields
                .iter()
                .map(|field| (field.binding.key, field.binding)),
        );
        self.specs = specs.into_values().collect();
        self.fields.retain(|key, _| values.contains_key(key));
        for spec in &self.specs {
            let Some(value) = values.get(&spec.key) else {
                continue;
            };
            let scale = definition.field(spec.key).map_or(1., |field| field.scale);
            let display_decimals = definition
                .field(spec.key)
                .and_then(|field| field.display_decimals);
            let field = self
                .fields
                .entry(spec.key)
                .or_insert_with(|| FieldDraft::new(value.clone(), scale, display_decimals));
            if field.touched {
                field.conflict = field.baseline != *value;
            } else if field.baseline != *value
                || field.scale != scale
                || field.display_decimals != display_decimals
            {
                let revision = field.revision + 1;
                *field = FieldDraft::new(value.clone(), scale, display_decimals);
                field.revision = revision;
            }
        }
        self.current = values;
    }
    pub fn edit(&mut self, spec: &PropertySpec, raw: String) {
        if let Some(field) = self.fields.get_mut(&spec.key) {
            field.parsed = if field.display_decimals.is_some()
                && raw.trim() == display_value(&field.baseline, field.scale, field.display_decimals)
            {
                // Focusing/submitting shortened display text must not round-trip
                // it into the full-precision authored value or create history.
                Ok(field.baseline.clone())
            } else if spec.value_type == zipzi_editor_core::PropertyType::Color
                && raw.trim().eq_ignore_ascii_case(&field.baseline.text())
            {
                Ok(field.baseline.clone())
            } else if field.scale != 1. {
                raw.trim()
                    .parse::<f64>()
                    .map_err(|error| error.to_string())
                    .and_then(|number| {
                        spec.parse(&(number / field.scale).to_string())
                            .map_err(|error| error.to_string())
                    })
            } else {
                spec.parse(&raw).map_err(|error| error.to_string())
            };
            field.raw = raw;
            field.touched = true;
        }
    }
    pub fn set_value(&mut self, spec: &PropertySpec, value: PropertyValue) {
        if let Some(field) = self.fields.get_mut(&spec.key) {
            field.raw = display_value(&value, field.scale, field.display_decimals);
            field.parsed = spec
                .validate(&value)
                .map(|_| value)
                .map_err(|error| error.to_string());
            field.touched = true;
        }
    }
    pub fn field_active(&self, id: PropertyKey, live_raw: &str) -> bool {
        self.fields
            .get(&id)
            .is_some_and(|field| field.touched || field.dirty() || field.raw != live_raw)
    }
    #[cfg(test)]
    pub fn dirty(&self) -> bool {
        self.fields.values().any(FieldDraft::dirty)
    }

    /// Dependencies come from the object, never from another field's draft.
    pub fn patch(&self, kind: ObjectKind, id: PropertyKey) -> anyhow::Result<PropertyPatch> {
        let spec = self
            .specs
            .iter()
            .find(|s| s.key == id)
            .ok_or_else(|| anyhow::anyhow!("This property is read only"))?;
        let field = &self.fields[&id];
        anyhow::ensure!(
            !field.conflict,
            "The value changed outside this form. Restored the current value."
        );
        let value = field
            .parsed
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        let expected: PropertyValues = self
            .specs
            .iter()
            .filter(|s| s.same_group(spec))
            .filter_map(|s| self.current.get(&s.key).map(|v| (s.key, v.clone())))
            .collect();
        let changes = if *value == field.baseline {
            PropertyValues::new()
        } else {
            [(id, value.clone())].into()
        };
        if kind == ObjectKind::Parameter {
            zipzi_editor_core::property::parameter::validate_change(&self.current, id, value)?;
        }

        Ok(PropertyPatch {
            scope: Default::default(),
            expected,
            changes,
        })
    }
    pub fn reload_field(&mut self, id: PropertyKey) {
        if let Some(value) = self.current.get(&id) {
            let revision = self.fields.get(&id).map_or(0, |field| field.revision + 1);
            let mut field = FieldDraft::new(
                value.clone(),
                self.fields[&id].scale,
                self.fields[&id].display_decimals,
            );
            field.revision = revision;
            self.fields.insert(id, field);
        }
    }
    pub fn committed_values(&self) -> &PropertyValues {
        &self.current
    }

    pub fn numeric_visible(&self, id: PropertyKey) -> bool {
        !matches!(
            self.fields[&id].parsed,
            Ok(PropertyValue::OptionalFloat(None))
        )
    }
}

#[derive(Default)]
pub(crate) struct PropertyForms {
    pub entries: HashMap<PropertyTarget, PropertyForm>,
}
impl PropertyForms {
    pub fn sync(
        &mut self,
        target: &PropertyTarget,
        definition: crate::properties::FormDefinition,
        values: PropertyValues,
    ) -> &PropertyForm {
        // Transient sampled selections have no drafts to restore after leaving
        // them. Keep one per document instead of retaining every seek time.
        if !target.has_object_details() {
            self.entries.retain(|key, _| {
                key.document != target.document || key.has_object_details() || key == target
            });
        }
        if matches!(
            target.scope,
            zipzi_editor_core::PropertyScope::Resource { .. }
        ) {
            self.entries.retain(|key, _| {
                key.document != target.document
                    || key.context != target.context
                    || key.object != target.object
                    || key == target
            });
        }
        let form = self.entries.entry(target.clone()).or_default();
        form.sync_definition(definition, values);
        form
    }
    pub fn commit(
        &mut self,
        editor: &mut Editor,
        target: &PropertyTarget,
        id: PropertyKey,
    ) -> anyhow::Result<()> {
        let form = self
            .entries
            .get_mut(target)
            .ok_or_else(|| anyhow::anyhow!("Form ended"))?;
        let result = (|| {
            anyhow::ensure!(
                editor.active_id() == Some(target.document)
                    && editor.editing_context() == Some(target.context),
                "Editing context changed"
            );
            let current = target.values(editor)?;
            form.sync_definition(target.definition(editor), current);
            let mut patch = form.patch(target.object.kind, id)?;
            if property_specs(target.object.kind)
                .iter()
                .any(|spec| spec.key == id && spec.keyframed)
            {
                anyhow::ensure!(
                    editor.document().unwrap().property_scope(&target.object) == target.scope,
                    "Keyform target changed"
                );
                patch.scope = target.scope.clone();
            }
            if !patch.changes.is_empty() {
                target.commit(editor, patch)?;
            }
            Ok(())
        })();
        if let Ok(values) = target.values(editor) {
            form.sync_definition(target.definition(editor), values);
        }
        form.reload_field(id);
        result
    }
    #[cfg(test)]
    pub fn dirty_document(&self, document: DocumentId) -> bool {
        self.entries
            .iter()
            .any(|(target, form)| target.document == document && form.dirty())
    }
    pub fn discard_document(&mut self, editor: &Editor, document: DocumentId) {
        for (target, form) in &mut self.entries {
            if target.document != document {
                continue;
            }
            if let Ok(values) = target.values(editor) {
                form.sync_definition(target.definition(editor), values);
            }
            for id in form.fields.keys().copied().collect::<Vec<_>>() {
                form.reload_field(id);
            }
        }
    }

    /// End text input with the same validation/reversion policy as blur.
    pub fn settle_document(&mut self, editor: &mut Editor, document: DocumentId) -> Option<String> {
        let fields: Vec<_> = self
            .entries
            .iter()
            .filter(|(target, _)| target.document == document)
            .flat_map(|(target, form)| {
                form.fields
                    .iter()
                    .filter(|(_, field)| field.touched)
                    .map(move |(&id, _)| (target.clone(), id))
            })
            .collect();
        let mut last_error = None;
        for (target, id) in fields {
            if let Err(error) = self.commit(editor, &target, id) {
                last_error = Some(error.to_string());
            }
        }
        last_error
    }
}

#[cfg(test)]
mod tests;
