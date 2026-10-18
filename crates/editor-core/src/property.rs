//! Typed inspector bindings. Presentation configuration lives in the editor;
//! these capabilities also guard every domain write.
use crate::{DocumentId, EditingContextToken, ObjectKind, ObjectRef};
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PropertyId {
    Name,
    RuntimeId,
    Enabled,
    DrawOrder,
    PixelsPerUnit,
    PaddingFraction,
    PaddingPixels,
    MinimumExtent,
    Minimum,
    Maximum,
    DefaultValue,
    DecimalPlaces,
    Repeat,
    Intensity,
    Looping,
    FadeIn,
    FadeOut,
}
impl PropertyId {
    pub fn path(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::RuntimeId => "runtime_id",
            Self::Enabled => "enabled",
            Self::DrawOrder => "draw_order",
            Self::PixelsPerUnit => "export_settings.pixels_per_unit",
            Self::PaddingFraction => "export_settings.padding_fraction",
            Self::PaddingPixels => "export_settings.padding_pixels",
            Self::MinimumExtent => "export_settings.minimum_extent",
            Self::Minimum => "minimum",
            Self::Maximum => "maximum",
            Self::DefaultValue => "default_value",
            Self::DecimalPlaces => "decimal_places",
            Self::Repeat => "repeat",
            Self::Intensity => "intensity",
            Self::Looping => "looping",
            Self::FadeIn => "fade_in",
            Self::FadeOut => "fade_out",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyGroup {
    Field(PropertyId),
    ParameterRange,
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
    pub id: PropertyId,
    pub value_type: PropertyType,
    pub group: PropertyGroup,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub preview: bool,
}
impl PropertySpec {
    const fn new(id: PropertyId, value_type: PropertyType) -> Self {
        Self {
            id,
            value_type,
            group: PropertyGroup::Field(id),
            min: -(f32::MAX as f64),
            max: f32::MAX as f64,
            step: 0.1,
            preview: false,
        }
    }
    const fn range(mut self, min: f64, max: f64, step: f64) -> Self {
        self.min = min;
        self.max = max;
        self.step = step;
        self
    }
    const fn grouped(mut self) -> Self {
        self.group = PropertyGroup::ParameterRange;
        self
    }
    const fn preview(mut self) -> Self {
        self.preview = true;
        self
    }
    pub fn parse(&self, raw: &str) -> Result<PropertyValue> {
        let value = match self.value_type {
            PropertyType::Text => PropertyValue::Text(if self.id == PropertyId::RuntimeId {
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
            _ => bail!("Invalid property type: {}", self.id.path()),
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
                !value.trim().is_empty(),
                "{} cannot be empty",
                self.id.path()
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
                self.id.path(),
                self.min,
                self.max
            );
        }
        Ok(())
    }
}

pub fn property_specs(kind: ObjectKind) -> &'static [PropertySpec] {
    use PropertyId::*;
    use PropertySpec as S;
    use PropertyType as T;
    const NAME: PropertySpec = S::new(Name, T::Text);
    const RUNTIME_ID: PropertySpec = S::new(RuntimeId, T::Text);
    const ENABLED: PropertySpec = S::new(Enabled, T::Bool);
    const FADE_IN: PropertySpec = S::new(FadeIn, T::OptionalFloat)
        .range(0., f32::MAX as f64, 0.1)
        .preview();
    const FADE_OUT: PropertySpec = S::new(FadeOut, T::OptionalFloat)
        .range(0., f32::MAX as f64, 0.1)
        .preview();
    const EXPORT_SETTINGS: &[PropertySpec] = &[
        S::new(PixelsPerUnit, T::Float).range(f32::MIN_POSITIVE as f64, f32::MAX as f64, 1.),
        S::new(PaddingFraction, T::Float).range(0., 10., 0.01),
        S::new(PaddingPixels, T::Float)
            .range(0., f32::MAX as f64, 1.)
            .preview(),
        S::new(MinimumExtent, T::Float)
            .range(0., f32::MAX as f64, 1.)
            .preview(),
    ];
    const PART: &[PropertySpec] = &[
        NAME,
        RUNTIME_ID,
        ENABLED,
        S::new(DrawOrder, T::Float)
            .range(-(f32::MAX as f64), f32::MAX as f64, 1.)
            .preview(),
    ];
    const PARAMETER: &[PropertySpec] = &[
        NAME,
        RUNTIME_ID,
        S::new(Minimum, T::Float).grouped().preview(),
        S::new(Maximum, T::Float).grouped().preview(),
        S::new(DefaultValue, T::Float).grouped().preview(),
        S::new(DecimalPlaces, T::Integer)
            .range(0., 9., 1.)
            .preview(),
        S::new(Repeat, T::Bool),
    ];
    const GLUE: &[PropertySpec] = &[
        NAME,
        RUNTIME_ID,
        S::new(Intensity, T::Float).range(0., 1., 0.01).preview(),
    ];
    const MOTION: &[PropertySpec] = &[NAME, S::new(Looping, T::Bool), FADE_IN, FADE_OUT];
    match kind {
        ObjectKind::Document => EXPORT_SETTINGS,
        ObjectKind::Part => PART,
        ObjectKind::Deformer | ObjectKind::Offscreen => &[NAME, RUNTIME_ID, ENABLED],
        ObjectKind::Parameter => PARAMETER,
        ObjectKind::Glue => GLUE,
        ObjectKind::Mesh => &[NAME, RUNTIME_ID, ENABLED],
        ObjectKind::Texture => &[NAME],
        ObjectKind::Motion => MOTION,
        ObjectKind::Expression => &[NAME, FADE_IN, FADE_OUT],
        _ => &[],
    }
}

pub type PropertyValues = BTreeMap<PropertyId, PropertyValue>;

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
    property: PropertyId,
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
        K::Parameter => get!(get_parameter),
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
        .map(|spec| {
            let value = spec
                .id
                .path()
                .split('.')
                .fold(&payload, |value, key| &value[key]);
            // Read persisted values without imposing new UI range restrictions.
            let mut reader = *spec;
            reader.min = f64::NEG_INFINITY;
            reader.max = f64::INFINITY;
            let value =
                if let (PropertyType::Text, Value::String(text)) = (reader.value_type, value) {
                    PropertyValue::Text(text.clone())
                } else {
                    reader.decode(value)?
                };
            Ok((spec.id, value))
        })
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
