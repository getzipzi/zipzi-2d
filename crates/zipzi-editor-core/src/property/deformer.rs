//! Deformer authoring properties. Identity, storage and editing policy live together.
use super::{PropertyEffect, PropertyScope, PropertySpec, PropertyType, PropertyValues};
use crate::ObjectKind;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use zipzi_core::{SceneBinding, SceneTrackEvaluation};

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Deformer, "name", "name", PropertyType::Text);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const ENABLED: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "enabled",
    "enabled",
    PropertyType::Bool,
);

pub const KIND: PropertySpec =
    PropertySpec::new(ObjectKind::Deformer, "kind", "kind", PropertyType::Text).readonly();
pub const PART: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "part_id",
    "part_id",
    PropertyType::Text,
)
.allow_empty();
pub const PARENT: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "parent_id",
    "parent_id",
    PropertyType::Text,
)
.allow_empty();
pub const COLUMNS: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "columns",
    "warp.columns",
    PropertyType::Integer,
)
.range(1., 64., 1.)
.grouped("warp_grid")
.preview();
pub const ROWS: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "rows",
    "warp.rows",
    PropertyType::Integer,
)
.range(1., 64., 1.)
.grouped("warp_grid")
.preview();
pub const QUAD: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "quad",
    "warp.quad",
    PropertyType::Bool,
)
.grouped("warp_grid")
.preview();
pub const BEZIER_COLUMNS: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "bezier_columns",
    "warp.bezier_columns",
    PropertyType::Integer,
)
.range(1., 32., 1.)
.grouped("warp_bezier_grid")
.preview();
pub const BEZIER_ROWS: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "bezier_rows",
    "warp.bezier_rows",
    PropertyType::Integer,
)
.range(1., 32., 1.)
.grouped("warp_bezier_grid")
.preview();
pub const AUTO_REFORM: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "auto_reform",
    "warp_authoring.auto_reform",
    PropertyType::Bool,
)
.grouped("warp_authoring");
pub const EDIT_MODE: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "bezier_edit_mode",
    "warp_authoring.edit_mode",
    PropertyType::Text,
)
.grouped("warp_authoring");
pub const ORIGINAL_AUTO: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "original_auto_update",
    "original_shape.auto_update",
    PropertyType::Bool,
);
pub const OPACITY: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "opacity",
    "appearance.opacity",
    PropertyType::Float,
)
.range(0., f32::MAX as f64, 0.01)
.keyframed()
.preview()
.affects(PropertyEffect::Properties);
pub const MULTIPLY: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "multiply",
    "appearance.multiply",
    PropertyType::Color,
)
.keyframed()
.preview()
.affects(PropertyEffect::Properties);
pub const SCREEN: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "screen",
    "appearance.screen",
    PropertyType::Color,
)
.keyframed()
.preview()
.affects(PropertyEffect::Properties);

pub const ORIGIN_X: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "origin_x",
    "rotation.pose.origin.x",
    PropertyType::Double,
)
.range(-f64::MAX, f64::MAX, 0.1)
.keyframed()
.preview();
pub const ORIGIN_Y: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "origin_y",
    "rotation.pose.origin.y",
    PropertyType::Double,
)
.range(-f64::MAX, f64::MAX, 0.1)
.keyframed()
.preview();
pub const ANGLE: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "angle",
    "rotation.pose.angle",
    PropertyType::Float,
)
.keyframed()
.preview();
pub const SCALE: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "scale",
    "rotation.pose.scale",
    PropertyType::Float,
)
.range(0., f32::MAX as f64, 0.01)
.keyframed()
.preview();
/// Persistent Editor display length, independent of deformation keyforms.
pub const BONE_LENGTH: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "bone_length",
    "rotation.bone_length",
    PropertyType::Float,
)
.range(0.001, f32::MAX as f64, 1.)
.preview();
pub const REFLECT_X: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "reflect_x",
    "rotation.pose.reflect_x",
    PropertyType::Bool,
)
.keyframed();
pub const REFLECT_Y: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "reflect_y",
    "rotation.pose.reflect_y",
    PropertyType::Bool,
)
.keyframed();
pub const BASE_ANGLE: PropertySpec = PropertySpec::new(
    ObjectKind::Deformer,
    "base_angle",
    "rotation.base_angle",
    PropertyType::Float,
)
.preview();

pub const FIELDS: &[PropertySpec] = &[
    NAME,
    RUNTIME_ID,
    ENABLED,
    KIND,
    OPACITY,
    MULTIPLY,
    SCREEN,
    ORIGIN_X,
    ORIGIN_Y,
    ANGLE,
    SCALE,
    BONE_LENGTH,
    REFLECT_X,
    REFLECT_Y,
    BASE_ANGLE,
    PART,
    PARENT,
    COLUMNS,
    ROWS,
    QUAD,
    BEZIER_COLUMNS,
    BEZIER_ROWS,
    AUTO_REFORM,
    EDIT_MODE,
    ORIGINAL_AUTO,
];

pub(crate) fn keyform<'a>(
    model: &'a zipzi_core::state::DocumentState,
    id: &str,
    scope: &PropertyScope,
) -> Result<Option<(&'a SceneBinding, usize)>> {
    let PropertyScope::DeformerKeyform { binding, samples } = scope else {
        return Ok(None);
    };
    let binding = model
        .binding_for_scene(id)
        .filter(|b| b.id == *binding)
        .ok_or_else(|| anyhow::anyhow!("Deformer binding changed"))?;
    let slot = (0..binding.track.len())
        .find(|&i| {
            let ids = binding.track.sample_ids(i);
            ids.len() == samples.len() && ids.iter().all(|id| samples.contains(id))
        })
        .ok_or_else(|| anyhow::anyhow!("Keyform no longer exists"))?;
    Ok(Some((binding, slot)))
}

/// Project only inspector scalars, never the Warp grid or keyform collection.
pub(crate) fn inspector_payload(
    model: &zipzi_core::state::DocumentState,
    id: &str,
    scope: &PropertyScope,
) -> Result<Value> {
    let transform = model
        .get_transform(id)
        .ok_or_else(|| anyhow::anyhow!("Deformer no longer exists"))?;
    let mut payload = json!({
        "name": transform.name, "runtime_id": transform.runtime_id,
        "enabled": transform.enabled, "kind": transform.kind(),
        "appearance": transform.appearance,
        "part_id": transform.part(), "parent_id": transform.parent(),
    });
    payload["original_shape"] =
        json!({"auto_update":transform.original_shape.as_ref().is_none_or(|r|r.auto_update)});
    if let Some(warp) = transform.warp() {
        payload["warp"] = json!({"columns": warp.columns, "rows": warp.rows, "quad": warp.quad});
        let settings = transform.warp_authoring.clone().unwrap_or_default();
        let grid = settings
            .bezier_grid
            .or_else(|| {
                warp.bezier.as_ref().map(|b| zipzi_core::WarpBezierGrid {
                    rows: b.rows,
                    columns: b.columns,
                })
            })
            .unwrap_or(zipzi_core::WarpBezierGrid {
                rows: 1,
                columns: 1,
            });
        payload["warp"]["bezier_columns"] = json!(grid.columns);
        payload["warp"]["bezier_rows"] = json!(grid.rows);
        payload["warp_authoring"] = json!(settings);
    }
    if let Some(rotation) = transform.rotation() {
        payload["rotation"] = json!(rotation);
        payload["rotation"]["bone_length"] = json!(
            model
                .editor_state()
                .rotation_bone_lengths
                .get(id)
                .copied()
                .unwrap_or(crate::deformer::ROTATION_BONE_LENGTH)
        );
    }
    if let Some((binding, slot)) = keyform(model, id, scope)? {
        let sample = binding.track.sample(slot);
        payload["appearance"] = json!(sample.appearance);
        if transform.rotation().is_some() {
            payload["rotation"]["pose"] = json!(sample.rotation);
        }
    }
    Ok(payload)
}

pub(crate) fn values(
    model: &zipzi_core::state::DocumentState,
    id: &str,
    scope: &PropertyScope,
) -> Result<PropertyValues> {
    let payload = inspector_payload(model, id, scope)?;
    FIELDS
        .iter()
        .filter(|spec| {
            (!spec.path.starts_with("rotation.") || payload.get("rotation").is_some())
                && (!spec.path.starts_with("warp.") || payload.get("warp").is_some())
                && (!spec.path.starts_with("warp_authoring.") || payload.get("warp").is_some())
        })
        .map(|spec| Ok((spec.key, spec.read(&payload)?)))
        .collect()
}

/// Merge inspector fields into the captured shape; geometry and other shapes
/// are retained by the SDK's typed replacement operations.
pub(crate) fn apply(
    edit: &mut zipzi_sdk::EditSession<'_>,
    id: &str,
    patch: &super::PropertyPatch,
    payload: &Value,
) -> Result<()> {
    if patch.changes.contains_key(&BONE_LENGTH.key) {
        ensure!(
            patch.changes.len() == 1,
            "Bone display length must be edited independently"
        );
        ensure!(
            edit.candidate_document()
                .get_transform(id)
                .is_some_and(|t| t.rotation().is_some()),
            "Select a Rotation"
        );
        let length: f32 = serde_json::from_value(payload["rotation"]["bone_length"].clone())?;
        ensure!(
            length.is_finite() && length > 0.,
            "Bone length must be finite and positive"
        );
        let mut state = edit.candidate_document().editor_state().clone();
        state.rotation_bone_lengths.insert(id.to_owned(), length);
        edit.replace_editor_state(state)?;
        return Ok(());
    }
    if patch.changes.contains_key(&ORIGINAL_AUTO.key) {
        ensure!(
            patch.changes.len() == 1,
            "Original shape policy must be edited independently"
        );
        edit.edit_scene_content(
            zipzi_contract::document::SceneContentEdit::DeformerOriginal {
                transform_id: id.into(),
                target: zipzi_contract::document::ShapeTarget::Base,
                action: zipzi_core::DeformerOriginalAction::AutoUpdate {
                    enabled: payload["original_shape"]["auto_update"].as_bool().unwrap(),
                },
                context: Default::default(),
            },
        )?;
        return Ok(());
    }
    let authoring = [
        BEZIER_COLUMNS.key,
        BEZIER_ROWS.key,
        AUTO_REFORM.key,
        EDIT_MODE.key,
    ];
    if patch.changes.keys().any(|key| authoring.contains(key)) {
        ensure!(
            patch.changes.keys().all(|key| authoring.contains(key)),
            "Warp editing settings must be independent"
        );
        let mut settings: zipzi_core::WarpAuthoringSettings =
            serde_json::from_value(payload["warp_authoring"].clone())?;
        if patch
            .changes
            .keys()
            .any(|key| [BEZIER_COLUMNS.key, BEZIER_ROWS.key].contains(key))
        {
            settings.bezier_grid = Some(zipzi_core::WarpBezierGrid {
                rows: payload["warp"]["bezier_rows"].as_u64().unwrap() as u32,
                columns: payload["warp"]["bezier_columns"].as_u64().unwrap() as u32,
            });
        }
        return Ok(edit.edit_scene_content(
            zipzi_contract::document::SceneContentEdit::WarpAuthoring {
                transform_id: id.into(),
                settings,
            },
        )?);
    }
    if patch
        .changes
        .keys()
        .any(|key| [COLUMNS.key, ROWS.key, QUAD.key].contains(key))
    {
        ensure!(
            patch
                .changes
                .keys()
                .all(|key| [COLUMNS.key, ROWS.key, QUAD.key].contains(key)),
            "Warp grid changes must be independent"
        );
        return Ok(edit.edit_scene_content(
            zipzi_contract::document::SceneContentEdit::ResizeWarp {
                transform_id: id.into(),
                rows: payload["warp"]["rows"].as_u64().unwrap() as u32,
                columns: payload["warp"]["columns"].as_u64().unwrap() as u32,
                quad: payload["warp"]["quad"].as_bool().unwrap(),
            },
        )?);
    }
    let model = edit.candidate_document();
    let original = model
        .get_transform(id)
        .ok_or_else(|| anyhow::anyhow!("Deformer no longer exists"))?;
    let mut next = original.clone();
    next.name = serde_json::from_value(payload["name"].clone())?;
    next.runtime_id = serde_json::from_value(payload["runtime_id"].clone())?;
    next.enabled = serde_json::from_value(payload["enabled"].clone())?;
    if let Some(rotation) = next.rotation_mut() {
        rotation.base_angle = serde_json::from_value(payload["rotation"]["base_angle"].clone())?;
    }
    let appearance_changed = [OPACITY, MULTIPLY, SCREEN]
        .iter()
        .any(|s| patch.changes.contains_key(&s.key));
    let pose_changed = [ORIGIN_X, ORIGIN_Y, ANGLE, SCALE, REFLECT_X, REFLECT_Y]
        .iter()
        .any(|s| patch.changes.contains_key(&s.key));
    if patch.changes.contains_key(&SCALE.key) {
        ensure!(
            payload["rotation"]["pose"]["scale"].as_f64().unwrap_or(0.) > 0.,
            "Rotation scale must be positive"
        );
    }
    let mut keyform = None;
    if appearance_changed || pose_changed {
        if let Some((binding, slot)) = keyform_for_edit(&model, id, &patch.scope)? {
            use zipzi_core::{SceneKeyform, SceneTrack};
            let form = match &binding.track {
                SceneTrack::Warp { keyforms, .. } => {
                    let mut form = keyforms[slot].clone();
                    if appearance_changed {
                        form.appearance = serde_json::from_value(payload["appearance"].clone())?;
                    }
                    SceneKeyform::Warp(form)
                }
                SceneTrack::Rotation { keyforms, .. } => {
                    let mut form = keyforms[slot].clone();
                    if appearance_changed {
                        form.appearance = serde_json::from_value(payload["appearance"].clone())?;
                    }
                    if pose_changed {
                        form.rotation =
                            serde_json::from_value(payload["rotation"]["pose"].clone())?;
                    }
                    SceneKeyform::Rotation(form)
                }
                _ => anyhow::bail!("Not a deformer binding"),
            };
            keyform = Some((binding.id.clone(), form));
        } else {
            if appearance_changed {
                next.appearance = serde_json::from_value(payload["appearance"].clone())?;
            }
            if pose_changed {
                next.rotation_mut()
                    .ok_or_else(|| anyhow::anyhow!("Not a Rotation deformer"))?
                    .pose = serde_json::from_value(payload["rotation"]["pose"].clone())?;
            }
        }
    }
    let changed = next != *original;
    if let Some((binding, form)) = keyform {
        edit.set_scene_keyform(&binding, form)?;
    }
    if changed {
        edit.replace_transform(next)?;
    }
    Ok(())
}

fn keyform_for_edit<'a>(
    model: &'a zipzi_core::state::DocumentState,
    id: &str,
    scope: &PropertyScope,
) -> Result<Option<(&'a SceneBinding, usize)>> {
    ensure!(
        scope.shape_editable(),
        "Select a stored keyform or return BlendShapes to their base shape"
    );
    keyform(model, id, scope)
}
