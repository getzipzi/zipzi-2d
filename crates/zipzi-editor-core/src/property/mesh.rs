//! Mesh authoring properties. Identity, storage and editing policy live together.
use super::{PropertyEffect, PropertyScope, PropertySpec, PropertyType};
use crate::ObjectKind;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use zipzi_core::{BlendMode, Mesh};

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Mesh, "name", "name", PropertyType::Text)
        .affects(PropertyEffect::Properties);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const ENABLED: PropertySpec =
    PropertySpec::new(ObjectKind::Mesh, "enabled", "enabled", PropertyType::Bool)
        .affects(PropertyEffect::Properties);
pub const DRAW_ORDER: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "draw_order",
    "draw_order",
    PropertyType::OptionalFloat,
)
.range(-32768., 32767., 1.)
.keyframed()
.scalar()
.affects(PropertyEffect::DrawOrder)
.preview();
pub const OPACITY: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "opacity",
    "appearance.opacity",
    PropertyType::Float,
)
.range(0., 1., 0.01)
.keyframed()
.scalar()
.affects(PropertyEffect::Properties)
.preview();

pub const PART: PropertySpec =
    PropertySpec::new(ObjectKind::Mesh, "part_id", "part_id", PropertyType::Text).allow_empty();
pub const TEXTURE: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "texture",
    "texture_choice",
    PropertyType::Text,
)
.allow_empty();
pub const DEFORMER: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "deformer_id",
    "deformer_id",
    PropertyType::Text,
)
.allow_empty();
pub const MASKS: PropertySpec =
    PropertySpec::new(ObjectKind::Mesh, "masks", "masks", PropertyType::References);
pub const INVERTED_MASK: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "inverted_mask",
    "inverted_mask",
    PropertyType::Bool,
);
pub const MULTIPLY: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "multiply",
    "appearance.multiply",
    PropertyType::Color,
)
.keyframed()
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const SCREEN: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "screen",
    "appearance.screen",
    PropertyType::Color,
)
.keyframed()
.scalar()
.affects(PropertyEffect::Properties)
.preview();
pub const COLOR_BLEND: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "color_blend",
    "color_blend",
    PropertyType::Text,
)
.grouped("blend");
pub const ALPHA_BLEND: PropertySpec = PropertySpec::new(
    ObjectKind::Mesh,
    "alpha_blend",
    "alpha_blend",
    PropertyType::Text,
)
.grouped("blend");
pub const CULLING: PropertySpec =
    PropertySpec::new(ObjectKind::Mesh, "culling", "culling", PropertyType::Bool);

pub const FIELDS: &[PropertySpec] = &[
    NAME,
    RUNTIME_ID,
    PART,
    TEXTURE,
    DEFORMER,
    MASKS,
    INVERTED_MASK,
    DRAW_ORDER,
    OPACITY,
    MULTIPLY,
    SCREEN,
    COLOR_BLEND,
    ALPHA_BLEND,
    CULLING,
    ENABLED,
];

pub const COLOR_MODES: &[&str] = &[
    "normal",
    "add_compatible",
    "multiply_compatible",
    "add",
    "add_glow",
    "darken",
    "multiply",
    "color_burn",
    "linear_burn",
    "lighten",
    "screen",
    "color_dodge",
    "overlay",
    "soft_light",
    "hard_light",
    "linear_light",
    "hue",
    "color",
];
pub const ALPHA_MODES: &[&str] = &["over", "atop", "out", "conjoint_over", "disjoint_over"];

pub(crate) fn inspector_fields(mesh: &Mesh, payload: &mut Value) {
    payload["texture_choice"] = json!(match &mesh.texture_asset_id {
        zipzi_core::SceneReference::Unresolved { runtime_id } => format!("unresolved:{runtime_id}"),
        reference => reference.as_str().to_owned(),
    });
    let packed = mesh.raw_blend_mode.unwrap_or(match mesh.blend_mode {
        BlendMode::Normal => 0,
        BlendMode::Additive => 1,
        BlendMode::Multiplicative => 2,
    });
    let name = |modes: &[&str], code: u32| {
        modes
            .get(code as usize)
            .map_or_else(|| format!("unknown_{code}"), |s| s.to_string())
    };
    payload["color_blend"] = json!(name(COLOR_MODES, packed & 255));
    payload["alpha_blend"] = json!(name(ALPHA_MODES, (packed >> 8) & 255));
    payload["culling"] = json!(!mesh.double_sided);
}

pub(crate) fn keyform<'a>(
    model: &'a zipzi_core::Document,
    id: &str,
    scope: &PropertyScope,
) -> Result<Option<(&'a str, &'a zipzi_core::MeshKeyform)>> {
    let PropertyScope::MeshKeyform { binding, samples } = scope else {
        return Ok(None);
    };
    let b = model
        .binding_for_mesh(id)
        .filter(|b| b.id == *binding)
        .ok_or_else(|| anyhow::anyhow!("Mesh binding changed"))?;
    let form = b
        .keyforms
        .iter()
        .find(|f| {
            f.samples.len() == samples.len() && f.samples.iter().all(|id| samples.contains(id))
        })
        .ok_or_else(|| anyhow::anyhow!("Keyform no longer exists"))?;
    Ok(Some((&b.id, form)))
}

pub(crate) fn payload(
    model: &zipzi_core::Document,
    id: &str,
    scope: &PropertyScope,
) -> Result<Value> {
    #[cfg(feature = "bench-instrumentation")]
    let _benchmark_span = zipzi_core::benchmark::span("editor.mesh_full_json");
    let mesh = model
        .get_mesh(id)
        .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
    let mut payload = serde_json::to_value(mesh)?;
    inspector_fields(mesh, &mut payload);
    if let Some((_, form)) = keyform(model, id, scope)? {
        payload["appearance"] = json!(form.appearance);
        payload["draw_order"] = json!(form.draw_order);
    }
    Ok(payload)
}

/// Inspector reads do not need vertex arrays or authored source metadata.
pub(crate) fn inspector_payload(
    model: &zipzi_core::Document,
    id: &str,
    scope: &PropertyScope,
) -> Result<Value> {
    #[cfg(feature = "bench-instrumentation")]
    let _benchmark_span = zipzi_core::benchmark::span("editor.mesh_inspector_json");
    let mesh = model
        .get_mesh(id)
        .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
    let mut payload = json!({
        "name": mesh.name, "runtime_id": mesh.runtime_id,
        "part_id": mesh.part_id, "deformer_id": mesh.deformer_id,
        "masks": mesh.masks, "inverted_mask": mesh.inverted_mask,
        "appearance": mesh.appearance, "draw_order": mesh.draw_order,
        "enabled": mesh.enabled,
    });
    inspector_fields(mesh, &mut payload);
    if let Some((_, form)) = keyform(model, id, scope)? {
        payload["appearance"] = json!(form.appearance);
        payload["draw_order"] = json!(form.draw_order);
    }
    Ok(payload)
}

pub(crate) fn normalize_blend(mesh: &mut Mesh, payload: &Value) -> Result<()> {
    let code = |modes: &[&str], field: &str| -> Result<u32> {
        modes
            .iter()
            .position(|m| Some(*m) == payload[field].as_str())
            .map(|i| i as u32)
            .ok_or_else(|| anyhow::anyhow!("Unsupported {field}"))
    };
    let color = code(COLOR_MODES, "color_blend")?;
    let alpha = code(ALPHA_MODES, "alpha_blend")?;
    // Compatible additive/multiply have fixed alpha equations. Do not present
    // another alpha mode as if the renderer could honor it.
    ensure!(
        alpha == 0 || !matches!(color, 1 | 2),
        "Compatible blend modes require Over alpha blending"
    );
    mesh.blend_mode = match color {
        1 => BlendMode::Additive,
        2 => BlendMode::Multiplicative,
        _ => BlendMode::Normal,
    };
    mesh.raw_blend_mode = (color >= 3 || alpha != 0).then_some(color | (alpha << 8));
    Ok(())
}
