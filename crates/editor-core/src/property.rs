//! Typed inspector bindings. Presentation configuration lives in the editor;
//! these capabilities also guard every domain write.
//! Add fields in the corresponding object module and reference those descriptors
//! from its UI provider. Keys are object-scoped; paths and policies are local.
use crate::{DocumentId, EditingContextToken, ObjectKind, ObjectRef};
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

/// An object-scoped field identity. Storage paths belong to the descriptor,
/// never to the key. Keys are cheap to copy into drafts, controls and edit tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PropertyKey {
    object: ObjectKind,
    name: &'static str,
}
impl PropertyKey {
    pub const fn new(object: ObjectKind, name: &'static str) -> Self {
        Self { object, name }
    }
    pub const fn name(self) -> &'static str {
        self.name
    }
    pub const fn object_kind(self) -> ObjectKind {
        self.object
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyType {
    Text,
    Bool,
    Integer,
    Float,
    OptionalFloat,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Text(String),
    Bool(bool),
    Integer(i64),
    Float(f32),
    OptionalFloat(Option<f32>),
}
impl PropertyValue {
    pub fn json(&self) -> Value {
        match self {
            Self::Text(v) => Value::from(v.clone()),
            Self::Bool(v) => Value::from(*v),
            Self::Integer(v) => Value::from(*v),
            Self::Float(v) => serde_json::json!(v),
            Self::OptionalFloat(v) => serde_json::json!(v),
        }
    }
    pub fn text(&self) -> String {
        match self {
            Self::Text(v) => v.clone(),
            Self::Bool(v) => v.to_string(),
            Self::Integer(v) => v.to_string(),
            Self::Float(v) => v.to_string(),
            Self::OptionalFloat(v) => v.map_or_else(String::new, |v| v.to_string()),
        }
    }
    pub fn number(&self) -> Option<f64> {
        match self {
            Self::Integer(v) => Some(*v as f64),
            Self::Float(v) | Self::OptionalFloat(Some(v)) => Some(f64::from(*v)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PropertySpec {
    pub key: PropertyKey,
    pub path: &'static str,
    pub value_type: PropertyType,
    group: Option<&'static str>,
    trim_text: bool,
    allow_empty: bool,
    pub(crate) refresh_draw_groups: bool,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub preview: bool,
}
impl PropertySpec {
    pub const fn new(
        object: ObjectKind,
        name: &'static str,
        path: &'static str,
        value_type: PropertyType,
    ) -> Self {
        Self {
            key: PropertyKey::new(object, name),
            path,
            value_type,
            group: None,
            trim_text: false,
            allow_empty: false,
            refresh_draw_groups: false,
            min: -(f32::MAX as f64),
            max: f32::MAX as f64,
            step: 0.1,
            preview: false,
        }
    }
    pub const fn range(mut self, min: f64, max: f64, step: f64) -> Self {
        self.min = min;
        self.max = max;
        self.step = step;
        self
    }
    pub const fn grouped(mut self, group: &'static str) -> Self {
        self.group = Some(group);
        self
    }
    pub const fn preview(mut self) -> Self {
        self.preview = true;
        self
    }
    pub const fn trimmed(mut self) -> Self {
        self.trim_text = true;
        self
    }
    pub const fn allow_empty(mut self) -> Self {
        self.allow_empty = true;
        self
    }
    pub const fn draw_order_preview(mut self) -> Self {
        self.preview = true;
        self.refresh_draw_groups = true;
        self
    }
    pub fn same_group(&self, other: &Self) -> bool {
        self.key == other.key
            || (self.key.object == other.key.object
                && self.group.is_some()
                && self.group == other.group)
    }
    /// Read persisted data without imposing editing-only range restrictions.
    pub fn read(&self, payload: &Value) -> Result<PropertyValue> {
        let value = self.path.split('.').fold(payload, |value, key| &value[key]);
        if let (PropertyType::Text, Value::String(text)) = (self.value_type, value) {
            return Ok(PropertyValue::Text(text.clone()));
        }
        let mut reader = *self;
        reader.min = f64::NEG_INFINITY;
        reader.max = f64::INFINITY;
        reader.decode(value)
    }
    pub fn write(&self, payload: &mut Value, value: &PropertyValue) -> Result<()> {
        self.validate(value)?;
        let mut field = payload;
        for key in self.path.split('.') {
            field = field
                .get_mut(key)
                .ok_or_else(|| anyhow::anyhow!("Missing property field: {}", self.path))?;
        }
        *field = value.json();
        Ok(())
    }
    pub fn parse(&self, raw: &str) -> Result<PropertyValue> {
        let value = match self.value_type {
            PropertyType::Text => PropertyValue::Text(if self.trim_text {
                raw.trim().to_owned()
            } else {
                raw.to_owned()
            }),
            PropertyType::Bool => PropertyValue::Bool(raw.parse()?),
            PropertyType::Integer => PropertyValue::Integer(raw.trim().parse()?),
            PropertyType::Float | PropertyType::OptionalFloat => {
                let number: f32 = raw.trim().parse()?;
                ensure!(number.is_finite(), "Enter a finite number");
                if self.value_type == PropertyType::OptionalFloat {
                    PropertyValue::OptionalFloat(Some(number))
                } else {
                    PropertyValue::Float(number)
                }
            }
        };
        self.validate(&value)?;
        Ok(value)
    }
    pub fn decode(&self, value: &Value) -> Result<PropertyValue> {
        let next = match (self.value_type, value) {
            (PropertyType::Text, Value::String(v)) => PropertyValue::Text(v.clone()),
            (PropertyType::Bool, Value::Bool(v)) => PropertyValue::Bool(*v),
            (PropertyType::Integer, Value::Number(v)) if v.as_i64().is_some() => {
                PropertyValue::Integer(v.as_i64().unwrap())
            }
            (PropertyType::Float | PropertyType::OptionalFloat, Value::Number(v)) => {
                let number = v
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("Invalid number"))?
                    as f32;
                if self.value_type == PropertyType::OptionalFloat {
                    PropertyValue::OptionalFloat(Some(number))
                } else {
                    PropertyValue::Float(number)
                }
            }
            (PropertyType::OptionalFloat, Value::Null) => PropertyValue::OptionalFloat(None),
            _ => bail!("Invalid property type: {}", self.path),
        };
        self.validate(&next)?;
        Ok(next)
    }
    pub fn validate(&self, value: &PropertyValue) -> Result<()> {
        ensure!(
            matches!(
                (self.value_type, value),
                (PropertyType::Text, PropertyValue::Text(_))
                    | (PropertyType::Bool, PropertyValue::Bool(_))
                    | (PropertyType::Integer, PropertyValue::Integer(_))
                    | (PropertyType::Float, PropertyValue::Float(_))
                    | (PropertyType::OptionalFloat, PropertyValue::OptionalFloat(_))
            ),
            "Invalid property type"
        );
        if let PropertyValue::Text(value) = value {
            ensure!(
                self.allow_empty || !value.trim().is_empty(),
                "{} cannot be empty",
                self.path
            );
        }
        match value {
            PropertyValue::Float(v) | PropertyValue::OptionalFloat(Some(v)) => {
                ensure!(v.is_finite(), "Enter a finite number")
            }
            _ => {}
        }
        if let Some(number) = value.number() {
            ensure!(
                (self.min..=self.max).contains(&number),
                "{} must be between {} and {}",
                self.path,
                self.min,
                self.max
            );
        }
        Ok(())
    }
}

pub mod deformer;
pub mod document;
pub mod expression;
pub mod glue;
pub mod mesh;
pub mod motion;
pub mod offscreen;
pub mod parameter;
pub mod parameter_group;
pub mod part;
pub mod texture;

/// Only object dispatch lives here; each schema owns all of its field definitions.
pub fn property_specs(kind: ObjectKind) -> &'static [PropertySpec] {
    match kind {
        ObjectKind::Document => document::FIELDS,
        ObjectKind::Part => part::FIELDS,
        ObjectKind::Mesh => mesh::FIELDS,
        ObjectKind::Parameter => parameter::FIELDS,
        ObjectKind::ParameterGroup => parameter_group::FIELDS,
        ObjectKind::Deformer => deformer::FIELDS,
        ObjectKind::Offscreen => offscreen::FIELDS,
        ObjectKind::Texture => texture::FIELDS,
        ObjectKind::Glue => glue::FIELDS,
        ObjectKind::Motion => motion::FIELDS,
        ObjectKind::Expression => expression::FIELDS,
        _ => &[],
    }
}

pub type PropertyValues = BTreeMap<PropertyKey, PropertyValue>;

#[derive(Clone, Debug)]
pub struct PropertyPatch {
    /// Includes the edited group's dependencies, not unrelated object fields.
    pub expected: PropertyValues,
    pub changes: PropertyValues,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyEditToken {
    pub document: DocumentId,
    pub object: ObjectRef,
    pub(crate) context: EditingContextToken,
    pub(crate) serial: u64,
    property: PropertyKey,
}

pub(crate) fn object_payload(model: &kasane_core::Document, object: &ObjectRef) -> Result<Value> {
    use ObjectKind as K;
    macro_rules! get {
        ($method:ident) => {
            serde_json::to_value(
                model
                    .$method(&object.id)
                    .ok_or_else(|| anyhow::anyhow!("Object no longer exists"))?,
            )?
        };
    }
    Ok(match object.kind {
        K::Document => {
            ensure!(model.id() == object.id, "Object no longer exists");
            serde_json::json!({ "export_settings": model.export_settings() })
        }
        K::Part => get!(get_part),
        K::Mesh => get!(get_mesh),
        K::Deformer => get!(get_transform),
        K::Parameter => {
            let mut payload = get!(get_parameter);
            let group = model
                .display_info()
                .parameters
                .iter()
                .flatten()
                .find_map(|entry| match entry {
                    kasane_core::document::CdiParameterEntry::Resolved {
                        parameter_id,
                        group_id,
                        ..
                    } if parameter_id == &object.id => group_id.clone(),
                    _ => None,
                });
            payload["group_id"] = Value::String(group.unwrap_or_default());
            payload
        }
        K::ParameterGroup => serde_json::to_value(
            model
                .display_info()
                .parameter_groups
                .iter()
                .flatten()
                .find(|group| group.id == object.id)
                .ok_or_else(|| anyhow::anyhow!("Object no longer exists"))?,
        )?,
        K::Glue => get!(get_glue),
        K::Offscreen => get!(get_offscreen),
        K::Texture => get!(get_asset),
        K::Motion => get!(get_motion),
        K::Expression => get!(get_expression),
        _ => bail!("This object has no editable properties"),
    })
}

pub(crate) fn values_from(
    model: &kasane_core::Document,
    object: &ObjectRef,
) -> Result<PropertyValues> {
    let payload = object_payload(model, object)?;
    property_specs(object.kind)
        .iter()
        .map(|spec| Ok((spec.key, spec.read(&payload)?)))
        .collect()
}

impl crate::Editor {
    pub fn commit_properties(
        &mut self,
        context: EditingContextToken,
        object: ObjectRef,
        patch: PropertyPatch,
    ) -> Result<()> {
        ensure!(
            self.editing_context_token_valid(context),
            "Editing context changed"
        );
        self.execute_for(
            context.document,
            crate::EditorCommand::SetObjectProperties { object, patch },
        )
    }

    pub fn begin_property_edit(
        &mut self,
        context: EditingContextToken,
        object: ObjectRef,
        patch: &PropertyPatch,
    ) -> Result<PropertyEditToken> {
        ensure!(
            self.editing_context_token_valid(context),
            "Editing context changed"
        );
        ensure!(
            self.editing_context() == Some(crate::EditingContext::Modeling)
                || matches!(object.kind, ObjectKind::Motion | ObjectKind::Expression),
            "Preview is unavailable for this object in Animation"
        );
        ensure!(
            patch.changes.len() == 1,
            "Preview edits one property at a time"
        );
        let property = *patch.changes.keys().next().unwrap();
        // A rejected start must not commit the previous candidate as a side
        // effect of FinishEdit. Validate capabilities and baselines first.
        self.document()
            .unwrap()
            .validate_property_preview(&object, patch)?;
        // A new inspector gesture replaces the previous inspector candidate.
        // Committing it here would invalidate the already checked baseline
        // when both gestures edit the same field.
        let end = if self.document().unwrap().property_edit_serial().is_some() {
            crate::EditorCommand::CancelEdit
        } else {
            crate::EditorCommand::FinishEdit
        };
        self.execute(end)?;
        let session = self.active_session_mut().unwrap();
        let serial = session.document.begin_property_edit(&object, patch)?;
        Ok(PropertyEditToken {
            document: context.document,
            object,
            context,
            serial,
            property,
        })
    }

    pub fn property_edit_owned(&self, token: &PropertyEditToken) -> bool {
        self.editing_context_token_valid(token.context)
            && token.document == token.context.document
            && self
                .document()
                .is_some_and(|doc| doc.property_edit_matches(token.serial, &token.object))
    }

    pub fn update_property_edit(
        &mut self,
        token: &PropertyEditToken,
        patch: &PropertyPatch,
    ) -> Result<()> {
        ensure!(self.property_edit_owned(token), "Property edit ended");
        ensure!(
            patch.changes.keys().all(|id| *id == token.property),
            "Property edit target changed"
        );
        let session = self.active_session_mut().unwrap();
        session
            .document
            .update_property_edit(&token.object, token.serial, patch)?;
        session.invalidate_scene();
        session.animation.playback.property_preview_changed();
        Ok(())
    }

    pub fn finish_property_edit(&mut self, token: &PropertyEditToken) -> Result<()> {
        ensure!(self.property_edit_owned(token), "Property edit ended");
        self.execute_for(token.document, crate::EditorCommand::FinishEdit)?;
        self.active_session_mut()
            .unwrap()
            .animation
            .playback
            .property_preview_changed();
        Ok(())
    }

    pub fn cancel_property_edit(&mut self, token: &PropertyEditToken) {
        if self.property_edit_owned(token) {
            let _ = self.execute_for(token.document, crate::EditorCommand::CancelEdit);
            self.active_session_mut()
                .unwrap()
                .animation
                .playback
                .property_preview_changed();
        }
    }
}

#[cfg(test)]
mod tests;
