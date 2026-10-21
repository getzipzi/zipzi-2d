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
    index: Option<usize>,
}
impl PropertyKey {
    pub const fn new(object: ObjectKind, name: &'static str) -> Self {
        Self {
            object,
            name,
            index: None,
        }
    }
    /// Array element identity for providers with repeated fields.
    pub const fn at(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }
    pub const fn index(self) -> Option<usize> {
        self.index
    }
    pub fn identity(self) -> String {
        self.index
            .map_or_else(|| self.name.to_owned(), |i| format!("{}-{i}", self.name))
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
    Color,
    References,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Text(String),
    Bool(bool),
    Integer(i64),
    Float(f32),
    OptionalFloat(Option<f32>),
    Color([f32; 3]),
    References(Vec<String>),
}
impl PropertyValue {
    pub fn json(&self) -> Value {
        match self {
            Self::Text(v) => Value::from(v.clone()),
            Self::Bool(v) => Value::from(*v),
            Self::Integer(v) => Value::from(*v),
            Self::Float(v) => serde_json::json!(v),
            Self::OptionalFloat(v) => serde_json::json!(v),
            Self::Color(v) => serde_json::json!(v),
            Self::References(v) => serde_json::json!(v),
        }
    }
    pub fn text(&self) -> String {
        match self {
            Self::Text(v) => v.clone(),
            Self::Bool(v) => v.to_string(),
            Self::Integer(v) => v.to_string(),
            Self::Float(v) => v.to_string(),
            Self::OptionalFloat(v) => v.map_or_else(String::new, |v| v.to_string()),
            Self::Color(v) => format!(
                "#{:02X}{:02X}{:02X}",
                (v[0] * 255.).round() as u8,
                (v[1] * 255.).round() as u8,
                (v[2] * 255.).round() as u8
            ),
            Self::References(v) => serde_json::to_string(v).unwrap(),
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

/// The write implementation is independent of whether a control previews it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyWrite {
    ReplaceObject,
    ScalarPatch,
}

/// Effects on editor structural projections. Core edits independently own
/// runtime evaluation invalidation. The default rebuilds editor structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyEffect {
    Structure,
    Properties,
    DrawOrder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropertySpec {
    pub key: PropertyKey,
    pub path: &'static str,
    pub value_type: PropertyType,
    group: Option<&'static str>,
    trim_text: bool,
    allow_empty: bool,
    pub(crate) write: PropertyWrite,
    pub(crate) effect: PropertyEffect,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub preview: bool,
    pub readonly: bool,
    pub keyframed: bool,
    indexed: bool,
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
            write: PropertyWrite::ReplaceObject,
            effect: PropertyEffect::Structure,
            min: -(f32::MAX as f64),
            max: f32::MAX as f64,
            step: 0.1,
            preview: false,
            readonly: false,
            keyframed: false,
            indexed: false,
        }
    }
    /// Descriptor for repeated fields; the concrete key carries its element index.
    pub const fn indexed(mut self) -> Self {
        self.indexed = true;
        self
    }
    fn accepts(&self, key: PropertyKey) -> bool {
        self.key == key
            || (self.indexed
                && key.index.is_some()
                && self.key.object == key.object
                && self.key.name == key.name)
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
    pub const fn readonly(mut self) -> Self {
        self.readonly = true;
        self
    }
    pub const fn keyframed(mut self) -> Self {
        self.keyframed = true;
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
    pub(crate) const fn scalar(mut self) -> Self {
        self.write = PropertyWrite::ScalarPatch;
        self
    }
    pub(crate) const fn affects(mut self, effect: PropertyEffect) -> Self {
        self.effect = effect;
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
            PropertyType::Color => {
                let hex = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
                ensure!(
                    hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
                    "Enter a color as #RRGGBB"
                );
                let color = u32::from_str_radix(hex, 16)?;
                PropertyValue::Color([
                    ((color >> 16) & 255) as f32 / 255.,
                    ((color >> 8) & 255) as f32 / 255.,
                    (color & 255) as f32 / 255.,
                ])
            }
            PropertyType::References => PropertyValue::References(serde_json::from_str(raw)?),
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
            (PropertyType::Color, Value::Array(_)) => {
                PropertyValue::Color(serde_json::from_value(value.clone())?)
            }
            (PropertyType::References, Value::Array(_)) => {
                PropertyValue::References(serde_json::from_value(value.clone())?)
            }
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
                    | (PropertyType::Color, PropertyValue::Color(_))
                    | (PropertyType::References, PropertyValue::References(_))
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
        if let PropertyValue::Color(color) = value {
            ensure!(
                color
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v)),
                "Color channels must be between 0 and 1"
            );
        }
        if let PropertyValue::References(ids) = value {
            let unique: std::collections::HashSet<_> = ids.iter().collect();
            ensure!(
                unique.len() == ids.len() && ids.iter().all(|id| !id.is_empty()),
                "References must be nonempty and unique"
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
pub mod physics;
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
        ObjectKind::PhysicsGroup => physics::FIELDS,
        _ => &[],
    }
}

pub fn property_spec(key: PropertyKey) -> Option<PropertySpec> {
    let mut spec = *property_specs(key.object_kind())
        .iter()
        .find(|spec| spec.accepts(key))?;
    spec.key = key;
    Some(spec)
}

pub type PropertyValues = BTreeMap<PropertyKey, PropertyValue>;

/// The pose destination captured when the inspector starts editing. Float keys
/// use their exact bits so navigation cannot redirect a draft to another form.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum PropertyScope {
    #[default]
    Object,
    MeshKeyform {
        binding: String,
        keys: Vec<u32>,
    },
    InterpolatedMesh,
}

#[derive(Clone, Debug)]
pub struct PropertyPatch {
    pub scope: PropertyScope,
    /// Includes the edited group's dependencies, not unrelated object fields.
    pub expected: PropertyValues,
    pub changes: PropertyValues,
}

impl PropertyPatch {
    pub(crate) fn uses_scalar_patch(&self, kind: ObjectKind) -> bool {
        // An empty drag update restores the committed descriptor. Keep that
        // reset on the scalar path without serializing motion curves/geometry.
        if self.changes.is_empty() {
            return property_specs(kind)
                .iter()
                .any(|s| s.write == PropertyWrite::ScalarPatch);
        }
        self.changes.keys().all(|id| {
            property_spec(*id).is_some_and(|s| {
                s.key.object_kind() == kind && s.write == PropertyWrite::ScalarPatch
            })
        })
    }

    pub(crate) fn preserves_structure(&self) -> bool {
        self.changes
            .keys()
            .all(|id| property_spec(*id).is_some_and(|s| s.effect != PropertyEffect::Structure))
    }

    pub(crate) fn affects_draw_groups(&self) -> bool {
        self.changes
            .keys()
            .any(|id| property_spec(*id).is_some_and(|s| s.effect == PropertyEffect::DrawOrder))
    }

    pub(crate) fn mesh_presentation_only(&self, kind: ObjectKind) -> bool {
        kind == ObjectKind::Mesh
            && self.changes.keys().all(|key| {
                property_spec(*key).is_some_and(|s| s.effect == PropertyEffect::Properties)
            })
    }
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
    #[cfg(feature = "bench-instrumentation")]
    let _benchmark_span = kasane_core::benchmark::span("editor.object_payload");
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
        K::Mesh => mesh::payload(model, &object.id, &PropertyScope::Object)?,
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

/// Inspector reads need descriptors, not geometry, motion curves or expression entries.
pub(crate) fn inspector_payload(
    model: &kasane_core::Document,
    object: &ObjectRef,
) -> Result<Value> {
    Ok(match object.kind {
        ObjectKind::Mesh => mesh::inspector_payload(model, &object.id, &PropertyScope::Object)?,
        ObjectKind::Motion => {
            let c = model
                .get_motion(&object.id)
                .ok_or_else(|| anyhow::anyhow!("Motion no longer exists"))?;
            serde_json::json!({"name":c.name,"duration":c.duration,"fps":c.fps,"looping":c.looping,"fade_in":c.fade_in,"fade_out":c.fade_out})
        }
        ObjectKind::Expression => {
            let e = model
                .get_expression(&object.id)
                .ok_or_else(|| anyhow::anyhow!("Expression no longer exists"))?;
            serde_json::json!({"name":e.name,"fade_in":e.fade_in,"fade_out":e.fade_out})
        }
        _ => object_payload(model, object)?,
    })
}

pub(crate) fn values_from(
    model: &kasane_core::Document,
    object: &ObjectRef,
) -> Result<PropertyValues> {
    if object.kind == ObjectKind::PhysicsGroup {
        return physics::values(model, object);
    }
    let payload = inspector_payload(model, object)?;
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
                || matches!(
                    object.kind,
                    ObjectKind::Motion | ObjectKind::Expression | ObjectKind::PhysicsGroup
                ),
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
        let end = if self.document().unwrap().has_inspector_edit() {
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
