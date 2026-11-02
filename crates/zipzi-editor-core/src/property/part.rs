//! Part authoring properties, including its optional Offscreen component.
//! Inspector paths below project the component; persisted ownership stays in Offscreen.
use super::{
    PropertyEffect, PropertyKey, PropertyPatch, PropertyScope, PropertySpec, PropertyType,
    PropertyValues,
};
use crate::ObjectKind;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use zipzi_core::OffscreenLookup;

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Part, "name", "name", PropertyType::Text);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Part,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const ENABLED: PropertySpec =
    PropertySpec::new(ObjectKind::Part, "enabled", "enabled", PropertyType::Bool);
pub const DRAW_ORDER: PropertySpec = PropertySpec::new(
    ObjectKind::Part,
    "draw_order",
    "draw_order",
    PropertyType::Float,
)
.range(-(f32::MAX as f64), f32::MAX as f64, 1.)
.scalar()
.affects(PropertyEffect::DrawOrder)
.preview();

const fn offscreen_field(name: &'static str, path: &'static str, ty: PropertyType) -> PropertySpec {
    PropertySpec::new(ObjectKind::Part, name, path, ty).grouped("offscreen")
}

pub const OFFSCREEN: PropertySpec =
    offscreen_field("offscreen", "offscreen.present", PropertyType::Bool);
// Include component identity in every baseline so removing/recreating an
// Offscreen cannot silently redirect an outstanding inspector edit.
pub const OFFSCREEN_ID: PropertySpec =
    offscreen_field("offscreen_id", "offscreen.id", PropertyType::Text)
        .allow_empty()
        .readonly();
pub const COMPOSITE_VISIBLE: PropertySpec =
    offscreen_field("composite_visible", "offscreen.enabled", PropertyType::Bool);
pub const OPACITY: PropertySpec = offscreen_field(
    "offscreen_opacity",
    "offscreen.opacity",
    PropertyType::Float,
)
.range(0., f32::MAX as f64, 0.01)
.keyframed()
.preview();
pub const MULTIPLY: PropertySpec = offscreen_field(
    "offscreen_multiply",
    "offscreen.multiply",
    PropertyType::Color,
)
.keyframed()
.preview();
pub const SCREEN: PropertySpec =
    offscreen_field("offscreen_screen", "offscreen.screen", PropertyType::Color)
        .keyframed()
        .preview();
pub const COLOR_BLEND: PropertySpec = offscreen_field(
    "offscreen_color_blend",
    "offscreen.color_blend",
    PropertyType::Text,
);
pub const ALPHA_BLEND: PropertySpec = offscreen_field(
    "offscreen_alpha_blend",
    "offscreen.alpha_blend",
    PropertyType::Text,
);
pub const MASKS: PropertySpec = offscreen_field(
    "offscreen_masks",
    "offscreen.masks",
    PropertyType::References,
);
pub const INVERTED_MASK: PropertySpec = offscreen_field(
    "offscreen_inverted_mask",
    "offscreen.inverted_mask",
    PropertyType::Bool,
);

pub const FIELDS: &[PropertySpec] = &[
    NAME,
    RUNTIME_ID,
    ENABLED,
    DRAW_ORDER,
    OFFSCREEN,
    OFFSCREEN_ID,
    COMPOSITE_VISIBLE,
    OPACITY,
    MULTIPLY,
    SCREEN,
    COLOR_BLEND,
    ALPHA_BLEND,
    MASKS,
    INVERTED_MASK,
];

pub(crate) fn is_offscreen(key: PropertyKey) -> bool {
    FIELDS
        .iter()
        .any(|spec| spec.key == key && spec.path.starts_with("offscreen."))
}

fn slot(
    model: &zipzi_core::Document,
    id: &str,
    scope: &PropertyScope,
) -> Result<(usize, Option<usize>)> {
    let binding = model
        .binding_for_scene(id)
        .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()));
    if let PropertyScope::OffscreenKeyform {
        binding: binding_id,
        samples,
    } = scope
    {
        let binding = binding
            .filter(|b| b.id == *binding_id)
            .ok_or_else(|| anyhow::anyhow!("Part binding changed"))?;
        let slot = (0..binding.track.len())
            .find(|&i| {
                let ids = binding.track.sample_ids(i);
                ids.len() == samples.len() && ids.iter().all(|id| samples.contains(id))
            })
            .ok_or_else(|| anyhow::anyhow!("Keyform no longer exists"))?;
        return Ok((slot, Some(binding.track.len())));
    }
    Ok((0, binding.map(|b| b.track.len())))
}

pub(crate) fn payload(
    model: &zipzi_core::Document,
    id: &str,
    scope: &PropertyScope,
) -> Result<Value> {
    let part = model
        .get_part(id)
        .ok_or_else(|| anyhow::anyhow!("Part no longer exists"))?;
    let mut payload = serde_json::to_value(part)?;
    let os = model.offscreen_for_part(id);
    let (slot, count) = slot(model, id, scope)?;
    let form = os
        .map(|os| os.keyform_index(slot, count))
        .transpose()
        .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?
        .flatten()
        .and_then(|index| os.map(|os| &os.keyforms[index]));
    let mode = os.map_or(0, |os| os.blend_mode);
    let mode_name = |names: &[&str], code: u32| {
        names
            .get(code as usize)
            .map_or_else(|| format!("unknown_{code}"), |name| name.to_string())
    };
    payload["offscreen"] = json!({
        "present": os.is_some(), "id": os.map_or("", |os| os.id.as_str()),
        "enabled": os.is_none_or(|os| os.enabled),
        "opacity": form.map_or(1., |f| f.opacity),
        "multiply": form.and_then(|f| f.multiply).unwrap_or([1.; 3]),
        "screen": form.and_then(|f| f.screen).unwrap_or([0.; 3]),
        "color_blend": mode_name(super::mesh::COLOR_MODES, mode & 255),
        "alpha_blend": mode_name(super::mesh::ALPHA_MODES, (mode >> 8) & 255),
        "masks": os.map_or_else(Vec::new, |os| os.masks.clone()),
        "inverted_mask": os.is_some_and(|os| os.flags & 8 != 0),
    });
    Ok(payload)
}

pub(crate) fn values(
    model: &zipzi_core::Document,
    id: &str,
    scope: &PropertyScope,
) -> Result<PropertyValues> {
    let payload = payload(model, id, scope)?;
    let driven = model
        .binding_for_scene(id)
        .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()));
    let mut values: PropertyValues = FIELDS
        .iter()
        .filter(|s| !driven || s.key != DRAW_ORDER.key)
        .map(|s| Ok((s.key, s.read(&payload)?)))
        .collect::<Result<_>>()?;
    // One inspector control represents the Part and its optional composite.
    // Keep the payload's raw Part flag intact so unrelated edits do not copy
    // a disabled composite's projected value into the Part itself.
    values.insert(
        ENABLED.key,
        super::PropertyValue::Bool(
            payload["enabled"] == true && payload["offscreen"]["enabled"] == true,
        ),
    );
    Ok(values)
}

pub(crate) fn apply_enabled(
    edit: &mut zipzi_sdk::EditSession<'_>,
    id: &str,
    payload: &Value,
) -> Result<()> {
    let next: zipzi_core::Part = serde_json::from_value(payload.clone())?;
    let model = edit.candidate_document();
    let part_changed = model.get_part(id) != Some(&next);
    let composite = model
        .offscreen_for_part(id)
        .filter(|os| next.enabled && !os.enabled)
        .map(|os| {
            let mut os = os.clone();
            os.enabled = true;
            os
        });
    // Both writes stay in the same candidate/history step. The native write
    // also checks component locks, so a refusal cannot partially enable a Part.
    if let Some(composite) = composite {
        edit.replace_offscreen(composite)?;
    }
    if part_changed {
        edit.replace_part(next)?;
    }
    Ok(())
}

pub(crate) fn apply_offscreen(
    edit: &mut zipzi_sdk::EditSession<'_>,
    id: &str,
    patch: &PropertyPatch,
    payload: &Value,
) -> Result<()> {
    ensure!(
        patch.changes.keys().all(|key| is_offscreen(*key)),
        "Edit compositing separately from Part fields"
    );
    let data = &payload["offscreen"];
    let model = edit.candidate_document();
    let original = model.offscreen_for_part(id);
    if patch.changes.contains_key(&OFFSCREEN.key) {
        ensure!(
            patch.changes.len() == 1,
            "Toggle offscreen drawing separately from its settings"
        );
        if data["present"] == true {
            if original.is_none() {
                edit.edit_scene(zipzi_sdk::SceneEdit::Create {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: model.get_part(id).unwrap().name.clone(),
                    object: zipzi_sdk::SceneCreate::Offscreen,
                    part_id: Some(id.into()),
                    parent_id: None,
                    before: None,
                    deformation_before: None,
                })?;
            }
        } else if let Some(os) = original {
            edit.edit_scene(zipzi_sdk::SceneEdit::Delete {
                ids: vec![os.id.clone()],
                scope: zipzi_sdk::SceneCopyScope::Selected,
            })?;
        }
        return Ok(());
    }
    let original = original.ok_or_else(|| anyhow::anyhow!("Enable offscreen drawing first"))?;
    let mut next = original.clone();
    if patch.changes.contains_key(&COMPOSITE_VISIBLE.key) {
        next.enabled = serde_json::from_value(data["enabled"].clone())?;
    }
    if patch.changes.contains_key(&MASKS.key) {
        next.masks = serde_json::from_value(data["masks"].clone())?;
    }
    if patch.changes.contains_key(&INVERTED_MASK.key) {
        next.flags = (next.flags & !8) | if data["inverted_mask"] == true { 8 } else { 0 };
    }
    if patch
        .changes
        .keys()
        .any(|key| [COLOR_BLEND.key, ALPHA_BLEND.key].contains(key))
    {
        let mut mesh = zipzi_core::Mesh::default();
        super::mesh::normalize_blend(&mut mesh, data)?;
        let packed = mesh.raw_blend_mode.unwrap_or(match mesh.blend_mode {
            zipzi_core::BlendMode::Normal => 0,
            zipzi_core::BlendMode::Additive => 1,
            zipzi_core::BlendMode::Multiplicative => 2,
        });
        next.blend_mode = (next.blend_mode & !0xffff) | packed;
    }
    if patch
        .changes
        .keys()
        .any(|key| [OPACITY.key, MULTIPLY.key, SCREEN.key].contains(key))
    {
        ensure!(
            patch.scope.shape_editable(),
            "Select a stored keyform or return BlendShapes to their base shape"
        );
        let (slot, count) = slot(&model, id, &patch.scope)?;
        let index = next
            .keyform_index(slot, count)
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))?;
        let mut form =
            index
                .map(|i| next.keyforms[i].clone())
                .unwrap_or(zipzi_core::OffscreenKeyform {
                    opacity: 1.,
                    ..Default::default()
                });
        if patch.changes.contains_key(&OPACITY.key) {
            form.opacity = serde_json::from_value(data["opacity"].clone())?;
        }
        if patch.changes.contains_key(&MULTIPLY.key) {
            form.multiply = Some(serde_json::from_value(data["multiply"].clone())?);
        }
        if patch.changes.contains_key(&SCREEN.key) {
            form.screen = Some(serde_json::from_value(data["screen"].clone())?);
        }
        if let Some(count) = count {
            // A source keyform may serve several Part slots. Detach only the
            // edited slot; other forms and negative neutral sentinels survive.
            if next.part_keyform_indices.is_empty() {
                next.part_keyform_indices = vec![-1; count];
            }
            if let Some(index) = index.filter(|index| {
                next.part_keyform_indices
                    .iter()
                    .filter(|&&i| i == *index as i32)
                    .count()
                    == 1
            }) {
                next.keyforms[index] = form;
            } else {
                next.part_keyform_indices[slot] = next.keyforms.len() as i32;
                next.keyforms.push(form);
            }
        } else if let Some(index) = index {
            next.keyforms[index] = form;
        } else {
            next.part_keyform_indices = vec![next.keyforms.len() as i32];
            next.keyforms.push(form);
        }
    }
    if next != *original {
        edit.replace_offscreen(next)?;
    }
    Ok(())
}
