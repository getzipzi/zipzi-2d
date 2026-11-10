//! Glue authoring properties. Identity, storage and editing policy live together.
use super::{
    PropertyEffect, PropertyPatch, PropertyScope, PropertySpec, PropertyType, PropertyValue,
    PropertyValues,
};
use crate::{Document, ObjectKind, ObjectRef};
use anyhow::{Result, anyhow, ensure};
use zipzi_core::{DeltaKeyforms, PreviewValues, state::DocumentState};

pub const NAME: PropertySpec =
    PropertySpec::new(ObjectKind::Glue, "name", "name", PropertyType::Text);
pub const RUNTIME_ID: PropertySpec = PropertySpec::new(
    ObjectKind::Glue,
    "runtime_id",
    "runtime_id",
    PropertyType::Text,
)
.trimmed();
pub const INTENSITY: PropertySpec = PropertySpec::new(
    ObjectKind::Glue,
    "intensity",
    "intensity",
    PropertyType::Float,
)
.range(0., 1., 0.01)
.keyframed()
.affects(PropertyEffect::Properties)
.preview();

pub const FIELDS: &[PropertySpec] = &[NAME, RUNTIME_ID, INTENSITY];

pub(crate) fn scope(
    document: &Document,
    model: &DocumentState,
    object: &ObjectRef,
    values: &PreviewValues,
) -> PropertyScope {
    let Some(glue) = model.glue(&object.id) else {
        return PropertyScope::Object;
    };
    match crate::control_edit::blend_edit_target(document, model, values, object) {
        Ok(Some((binding, slot, weight))) => {
            let mut background =
                zipzi_core::evaluation::ordinary_glue_intensity(model, values, glue);
            for b in model
                .blend_bindings_for_target(&object.id)
                .filter(|b| model.blend_binding_active(b))
            {
                let DeltaKeyforms::Glue(forms) = &b.keyforms else {
                    continue;
                };
                for (index, influence) in
                    zipzi_core::evaluation::evaluate_blend_binding(model, values, b)
                {
                    if b.id != binding.id || index != slot {
                        background += forms[index].intensity * influence;
                    }
                }
            }
            let DeltaKeyforms::Glue(forms) = &binding.keyforms else {
                unreachable!()
            };
            return PropertyScope::GlueBlendShape {
                binding: binding.id.clone(),
                sample: forms[slot].sample_id.clone(),
                weight_bits: weight.to_bits(),
                background_bits: background.to_bits(),
            };
        }
        Err(reason) => {
            return PropertyScope::UnavailableGlueBlendShape {
                reason: reason.to_string(),
            };
        }
        Ok(None) => {}
    }
    let Some(binding) = glue.binding.as_ref() else {
        return PropertyScope::Object;
    };
    let axes = binding
        .axes
        .iter()
        .filter(|axis| !axis.parameter_id.is_empty())
        .collect::<Vec<_>>();
    if axes.is_empty() {
        return PropertyScope::Object;
    }
    let samples = axes
        .iter()
        .map(|axis| {
            axis.samples
                .iter()
                .find(|sample| (values[&axis.parameter_id] - sample.value).abs() < 1e-6)
                .map(|sample| sample.id.clone())
        })
        .collect::<Option<Vec<_>>>();
    samples
        .filter(|samples| binding.keyforms.iter().any(|form| form.samples == *samples))
        .map_or(PropertyScope::InterpolatedGlue, |samples| {
            PropertyScope::GlueKeyform { samples }
        })
}

pub(crate) fn values(
    model: &DocumentState,
    object: &ObjectRef,
    scope: &PropertyScope,
) -> Result<PropertyValues> {
    let glue = model
        .glue(&object.id)
        .ok_or_else(|| anyhow!("Glue no longer exists"))?;
    let value = match scope {
        PropertyScope::GlueKeyform { samples } => {
            glue.binding
                .as_ref()
                .and_then(|binding| {
                    binding
                        .keyforms
                        .iter()
                        .find(|form| form.samples == *samples)
                })
                .ok_or_else(|| anyhow!("Glue keyform no longer exists"))?
                .intensity
        }
        PropertyScope::GlueBlendShape {
            binding,
            sample,
            weight_bits,
            background_bits,
        } => {
            let form = model
                .blend_binding(binding)
                .and_then(|binding| match &binding.keyforms {
                    DeltaKeyforms::Glue(forms) => {
                        forms.iter().find(|form| form.sample_id == *sample)
                    }
                    _ => None,
                })
                .ok_or_else(|| anyhow!("Glue BlendShape keyform no longer exists"))?;
            (f32::from_bits(*background_bits) + f32::from_bits(*weight_bits) * form.intensity)
                .clamp(0., 1.)
        }
        _ => glue.intensity,
    };
    Ok([
        (NAME.key, PropertyValue::Text(glue.name.clone())),
        (RUNTIME_ID.key, PropertyValue::Text(glue.runtime_id.clone())),
        (INTENSITY.key, PropertyValue::Float(value)),
    ]
    .into())
}

pub(crate) fn apply_intensity(
    edit: &mut zipzi_sdk::Commands<'_>,
    object: &ObjectRef,
    patch: &PropertyPatch,
    payload: &serde_json::Value,
) -> Result<()> {
    let value: f32 = serde_json::from_value(payload["intensity"].clone())?;
    let model = edit.read_document()?;
    let mut glue = model
        .glue(&object.id)
        .cloned()
        .ok_or_else(|| anyhow!("Glue no longer exists"))?;
    let rename = patch.changes.contains_key(&NAME.key);
    let runtime_id_changed = patch.changes.contains_key(&RUNTIME_ID.key);
    if rename {
        glue.name = serde_json::from_value(payload["name"].clone())?;
    }
    if runtime_id_changed {
        glue.runtime_id = serde_json::from_value(payload["runtime_id"].clone())?;
    }
    match &patch.scope {
        PropertyScope::Object => glue.intensity = value,
        PropertyScope::GlueKeyform { samples } => {
            let form = glue
                .binding
                .as_mut()
                .and_then(|binding| {
                    binding
                        .keyforms
                        .iter_mut()
                        .find(|form| form.samples == *samples)
                })
                .ok_or_else(|| anyhow!("Glue keyform no longer exists"))?;
            form.intensity = value;
        }
        PropertyScope::GlueBlendShape {
            binding,
            sample,
            weight_bits,
            background_bits,
        } => {
            let mut binding = model
                .blend_binding(binding)
                .cloned()
                .ok_or_else(|| anyhow!("Glue BlendShape no longer exists"))?;
            ensure!(
                binding.target_id == object.id,
                "Glue BlendShape target changed"
            );
            let DeltaKeyforms::Glue(forms) = &mut binding.keyforms else {
                anyhow::bail!("Glue BlendShape target changed");
            };
            let form = forms
                .iter_mut()
                .find(|form| form.sample_id == *sample)
                .ok_or_else(|| anyhow!("Glue BlendShape keyform no longer exists"))?;
            form.intensity =
                (value - f32::from_bits(*background_bits)) / f32::from_bits(*weight_bits);
            if rename || runtime_id_changed {
                edit.replace_glue(glue)?;
            }
            edit.replace_blend_binding(binding)?;
            return Ok(());
        }
        _ => anyhow::bail!("Select a stored Glue keyform before editing strength"),
    }
    edit.replace_glue(glue)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use zipzi_core::{
        BindingAxis, BlendShapeBinding, BlendShapeKeyTable, BlendShapeTargetKind, DeltaGlueKeyform,
        Glue, GlueBinding, GlueKeyform, GlueVertexPair, Mesh, Parameter, ParameterKind, Vec2,
    };

    #[test]
    fn glue_strength_writes_the_selected_ordinary_or_blend_sample() -> Result<()> {
        let mut document = Document::new_blank()?;
        let id = |n| uuid::Uuid::from_u128(n).to_string();
        let object = ObjectRef::new(ObjectKind::Glue, id(30));
        document.session.edit("Glue fixture", None, |edit| {
            for n in [10, 20] {
                crate::test_sdk::insert_fixture_mesh(
                    edit,
                    Mesh {
                        id: id(n),
                        vertex_ids: vec![0, 1, 2],
                        base_positions: vec![
                            Vec2::new(n as f32, 0.),
                            Vec2::new(n as f32 + 1., 0.),
                            Vec2::new(n as f32, 1.),
                        ],
                        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                        triangles: vec![[0, 1, 2]],
                        ..Default::default()
                    },
                )?;
            }
            for (n, kind) in [(40, ParameterKind::Normal), (50, ParameterKind::BlendShape)] {
                crate::test_sdk::insert_fixture_parameter(
                    edit,
                    Parameter {
                        id: id(n),
                        kind,
                        name: format!("Parameter {n}"),
                        minimum: 0.,
                        maximum: 1.,
                        default_value: 0.,
                        ..Default::default()
                    },
                )?;
            }
            crate::test_sdk::insert_glue(
                edit,
                Glue {
                    id: object.id.clone(),
                    mesh_a_id: id(10).into(),
                    mesh_b_id: id(20).into(),
                    pairs: vec![GlueVertexPair {
                        vertex_a: Some(0),
                        vertex_b: Some(0),
                        ..Default::default()
                    }],
                    intensity: 0.25,
                    binding: Some(GlueBinding {
                        axes: vec![BindingAxis {
                            parameter_id: id(40),
                            keys: vec![0., 1.],
                            ..Default::default()
                        }],
                        keyforms: [0.5, 0.75]
                            .map(|intensity| GlueKeyform {
                                intensity,
                                samples: vec![],
                            })
                            .into(),
                    }),
                    ..Default::default()
                },
            )?;
            crate::test_sdk::insert_blend_table(
                edit,
                BlendShapeKeyTable {
                    id: id(60),
                    parameter_id: id(50),
                    keys: vec![0., 1.],
                    base_key_idx: 0,
                    samples: vec![],
                    base_sample_id: String::new(),
                },
            )?;
            crate::test_sdk::insert_blend_binding(
                edit,
                BlendShapeBinding {
                    id: id(70),
                    target_id: object.id.clone(),
                    target_kind: BlendShapeTargetKind::Glue,
                    key_table_id: id(60),
                    constraint_ids: vec![],
                    keyforms: DeltaKeyforms::Glue(vec![
                        DeltaGlueKeyform::default(),
                        DeltaGlueKeyform {
                            intensity: 0.125,
                            ..Default::default()
                        },
                    ]),
                },
            )
        })?;
        let intensity = |document: &Document| {
            document.property_values(&object).unwrap()[&INTENSITY.key].clone()
        };
        assert_eq!(intensity(&document), PropertyValue::Float(0.5));
        document.set_object_property(&object, "intensity", json!(0.375))?;
        document.set_preview_parameter(&id(40), 0.5)?;
        assert_eq!(
            document.property_scope(&object),
            PropertyScope::InterpolatedGlue
        );
        assert!(
            document
                .set_object_property(&object, "intensity", json!(1.))
                .is_err()
        );

        document.set_preview_parameter(&id(40), 1.)?;
        document.set_preview_parameter(&id(50), 0.5)?;
        assert!(matches!(
            document.property_scope(&object),
            PropertyScope::UnavailableGlueBlendShape { .. }
        ));
        document.set_preview_parameter(&id(50), 1.)?;
        assert!(matches!(
            document.property_scope(&object),
            PropertyScope::GlueBlendShape { .. }
        ));
        assert_eq!(intensity(&document), PropertyValue::Float(0.875));
        document.set_object_property(&object, "intensity", json!(0.625))?;
        assert_eq!(intensity(&document), PropertyValue::Float(0.625));
        let model = document.model();
        let glue = model.glue(&object.id).unwrap();
        assert_eq!(glue.intensity, 0.25);
        assert_eq!(glue.binding.as_ref().unwrap().keyforms[0].intensity, 0.375);
        assert_eq!(glue.binding.as_ref().unwrap().keyforms[1].intensity, 0.75);
        let DeltaKeyforms::Glue(forms) = &model.blend_binding(&id(70)).unwrap().keyforms else {
            unreachable!()
        };
        assert_eq!(forms[0].intensity, 0.);
        assert_eq!(forms[1].intensity, -0.125);
        Ok(())
    }
}
