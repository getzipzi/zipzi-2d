//! Stable child resource descriptors; domain edits remain typed.
use super::*;
use kasane_core::document::{
    ExpressionBlend, ExpressionEdit, ExpressionEntryFields, ExpressionTarget, MotionEdit,
};
pub const GROUP_NAME: PropertySpec =
    PropertySpec::new(ObjectKind::MotionGroup, "name", "name", PropertyType::Text);
pub const REG_FADE_IN: PropertySpec = PropertySpec::new(
    ObjectKind::MotionRegistration,
    "registration_fade_in",
    "fade_in",
    PropertyType::OptionalFloat,
)
.range(0., f32::MAX as f64, 0.1)
.affects(PropertyEffect::Properties)
.preview()
.indexed();
pub const REG_FADE_OUT: PropertySpec = PropertySpec::new(
    ObjectKind::MotionRegistration,
    "registration_fade_out",
    "fade_out",
    PropertyType::OptionalFloat,
)
.range(0., f32::MAX as f64, 0.1)
.affects(PropertyEffect::Properties)
.preview()
.indexed();
pub const SOUND: PropertySpec = PropertySpec::new(
    ObjectKind::MotionRegistration,
    "registration_sound",
    "sound",
    PropertyType::Text,
)
.allow_empty()
.indexed();
pub const TARGET: PropertySpec = PropertySpec::new(
    ObjectKind::ExpressionEntry,
    "target",
    "target",
    PropertyType::Text,
)
.allow_empty()
.grouped("entry")
.indexed();
pub const VALUE: PropertySpec = PropertySpec::new(
    ObjectKind::ExpressionEntry,
    "value",
    "value",
    PropertyType::Float,
)
.grouped("entry")
.indexed()
.affects(PropertyEffect::Properties)
.preview();
pub const BLEND: PropertySpec = PropertySpec::new(
    ObjectKind::ExpressionEntry,
    "blend",
    "blend",
    PropertyType::Text,
)
.allow_empty()
.grouped("entry")
.indexed();
pub const GROUP_FIELDS: &[PropertySpec] = &[GROUP_NAME];
pub const REGISTRATION_FIELDS: &[PropertySpec] = &[REG_FADE_IN, REG_FADE_OUT, SOUND];
pub const ENTRY_FIELDS: &[PropertySpec] = &[TARGET, VALUE, BLEND];
pub(crate) fn payload(model: &kasane_core::Document, object: &ObjectRef) -> Result<Value> {
    Ok(match object.kind {
        ObjectKind::MotionGroup => {
            serde_json::json!({"name": model.get_motion_group(&object.id).ok_or_else(|| anyhow::anyhow!("Missing motion group"))?.name})
        }
        ObjectKind::MotionRegistration => {
            let (_, entry) = model
                .get_motion_registration(&object.id)
                .ok_or_else(|| anyhow::anyhow!("Missing registration"))?;
            serde_json::json!({"fade_in":entry.fade_in,"fade_out":entry.fade_out,"sound":entry.sound.as_ref().map(|r| match r { kasane_core::document::AttachmentRef::Resolved { attachment_id } => attachment_id.clone(), kasane_core::document::AttachmentRef::Unresolved { path } => format!("path:{path}") }).unwrap_or_default()})
        }
        ObjectKind::ExpressionEntry => {
            let (_, entry) = model
                .get_expression_entry(&object.id)
                .ok_or_else(|| anyhow::anyhow!("Missing expression entry"))?;
            let target = match &entry.target {
                ExpressionTarget::Unspecified => String::new(),
                ExpressionTarget::Resolved { parameter_id } => parameter_id.clone(),
                ExpressionTarget::Unresolved { runtime_id } => format!("runtime:{runtime_id}"),
            };
            let blend = match entry.blend {
                None => "",
                Some(ExpressionBlend::Add) => "add",
                Some(ExpressionBlend::Multiply) => "multiply",
                Some(ExpressionBlend::Overwrite) => "overwrite",
            };
            serde_json::json!({"target":target,"value":entry.value,"blend":blend})
        }
        _ => bail!("Expected an animation child"),
    })
}
pub(crate) fn motion_patch(
    model: &kasane_core::Document,
    object: &ObjectRef,
    payload: Value,
) -> Result<MotionEdit> {
    if object.kind == ObjectKind::MotionGroup {
        return Ok(MotionEdit::RenameGroup {
            id: object.id.clone(),
            name: serde_json::from_value(payload["name"].clone())?,
        });
    }
    let (_, expected) = model
        .get_motion_registration(&object.id)
        .ok_or_else(|| anyhow::anyhow!("Missing registration"))?;
    let mut entry = expected.clone();
    entry.fade_in = serde_json::from_value(payload["fade_in"].clone())?;
    entry.fade_out = serde_json::from_value(payload["fade_out"].clone())?;
    let sound = payload["sound"].as_str().unwrap();
    entry.sound = if sound.is_empty() {
        None
    } else {
        Some(if let Some(path) = sound.strip_prefix("path:") {
            kasane_core::document::AttachmentRef::Unresolved { path: path.into() }
        } else {
            kasane_core::document::AttachmentRef::Resolved {
                attachment_id: sound.into(),
            }
        })
    };
    Ok(MotionEdit::PatchRegistration {
        id: object.id.clone(),
        expected: expected.clone(),
        entry,
    })
}
pub(crate) fn expression_patch(
    model: &kasane_core::Document,
    object: &ObjectRef,
    payload: Value,
) -> Result<ExpressionEdit> {
    let (_, expected) = model
        .get_expression_entry(&object.id)
        .ok_or_else(|| anyhow::anyhow!("Missing expression entry"))?;
    let target = payload["target"].as_str().unwrap();
    let target = if target.is_empty() {
        ExpressionTarget::Unspecified
    } else if let Some(runtime_id) = target.strip_prefix("runtime:") {
        ExpressionTarget::Unresolved {
            runtime_id: runtime_id.into(),
        }
    } else {
        ExpressionTarget::Resolved {
            parameter_id: target.into(),
        }
    };
    let blend = match payload["blend"].as_str().unwrap() {
        "" => None,
        "add" => Some(ExpressionBlend::Add),
        "multiply" => Some(ExpressionBlend::Multiply),
        "overwrite" => Some(ExpressionBlend::Overwrite),
        _ => bail!("Invalid expression blend"),
    };
    Ok(ExpressionEdit::PatchEntry {
        id: object.id.clone(),
        expected: ExpressionEntryFields::from(expected),
        fields: ExpressionEntryFields {
            target,
            blend,
            value: serde_json::from_value(payload["value"].clone())?,
        },
    })
}
