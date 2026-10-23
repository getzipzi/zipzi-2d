//! Singleton and Pose fields use native typed patches, with explicit reference states.
use super::*;
use kasane_core::document::{
    PhysicsAssetProperty, PhysicsPatch, PoseEdit, PoseFields, PosePartRef,
};
pub const POSE_FADE: PropertySpec = PropertySpec::new(
    ObjectKind::Pose,
    "fade_in",
    "fade_in",
    PropertyType::OptionalFloat,
)
.range(0., f32::MAX as f64, 0.1);
pub const POSE_NAME: PropertySpec =
    PropertySpec::new(ObjectKind::PoseGroup, "name", "name", PropertyType::Text);
pub const POSE_PART: PropertySpec = PropertySpec::new(
    ObjectKind::PoseEntry,
    "target",
    "target",
    PropertyType::Text,
)
.allow_empty();
pub const POSE_LINK: PropertySpec =
    PropertySpec::new(ObjectKind::PoseLink, "target", "target", PropertyType::Text).allow_empty();
pub const PHYSICS_FPS: PropertySpec = PropertySpec::new(
    ObjectKind::Physics,
    "fps",
    "fps",
    PropertyType::OptionalFloat,
)
.range(0., 240., 1.);
fn token(reference: &PosePartRef) -> String {
    match reference {
        PosePartRef::Unspecified => String::new(),
        PosePartRef::Resolved { part_id } => part_id.clone(),
        PosePartRef::Unresolved { runtime_id } => format!("runtime:{runtime_id}"),
    }
}
fn reference(value: &str) -> PosePartRef {
    if value.is_empty() {
        PosePartRef::Unspecified
    } else if let Some(id) = value.strip_prefix("runtime:") {
        PosePartRef::Unresolved {
            runtime_id: id.into(),
        }
    } else {
        PosePartRef::Resolved {
            part_id: value.into(),
        }
    }
}
pub(crate) fn payload(model: &kasane_core::Document, object: &ObjectRef) -> Result<Value> {
    if object.kind == ObjectKind::Physics {
        let asset = model
            .physics()
            .filter(|a| a.id == object.id)
            .ok_or_else(|| anyhow::anyhow!("Missing physics"))?;
        return Ok(serde_json::json!({"fps":asset.data.meta.fps}));
    }
    let fields = model
        .pose_fields(&object.id)
        .ok_or_else(|| anyhow::anyhow!("Missing Pose object"))?;
    Ok(match fields {
        PoseFields::Asset { fade_in } => serde_json::json!({"fade_in":fade_in}),
        PoseFields::Group { name } => serde_json::json!({"name":name}),
        PoseFields::Entry { part } => serde_json::json!({"target":token(&part)}),
        PoseFields::Link { target } => serde_json::json!({"target":token(&target)}),
    })
}
pub(crate) fn pose_patch(
    model: &kasane_core::Document,
    object: &ObjectRef,
    payload: Value,
) -> Result<PoseEdit> {
    let expected = model
        .pose_fields(&object.id)
        .ok_or_else(|| anyhow::anyhow!("Missing Pose object"))?;
    let fields = match &expected {
        PoseFields::Asset { .. } => PoseFields::Asset {
            fade_in: serde_json::from_value(payload["fade_in"].clone())?,
        },
        PoseFields::Group { .. } => PoseFields::Group {
            name: payload["name"].as_str().unwrap().into(),
        },
        PoseFields::Entry { .. } => PoseFields::Entry {
            part: reference(payload["target"].as_str().unwrap()),
        },
        PoseFields::Link { .. } => PoseFields::Link {
            target: reference(payload["target"].as_str().unwrap()),
        },
    };
    Ok(PoseEdit::Patch {
        id: object.id.clone(),
        expected,
        fields,
    })
}
pub(crate) fn physics_patch(object: &ObjectRef, payload: Value) -> Result<PhysicsPatch> {
    Ok(PhysicsPatch {
        asset_id: object.id.clone(),
        asset_changes: vec![PhysicsAssetProperty::Fps(serde_json::from_value(
            payload["fps"].clone(),
        )?)],
        rig: None,
    })
}
