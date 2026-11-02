//! Deformer fields depend on the concrete type and frozen shape destination.
use super::*;
use zipzi_editor_core::property::deformer::*;

pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    fields: |_| fields(false),
    choices: parent_choices,
    type_name: scene::scene_type_name,
    show_readonly: |path| !matches!(path, "warp.quad" | "quad"),
    ..PropertyProvider::DEFAULT
};

fn fields(rotation: bool) -> FormDefinition {
    use FieldEditor::*;
    let common = scene_object::Bindings {
        name: NAME,
        runtime_id: RUNTIME_ID,
        part: PART,
        deformer: PARENT,
        opacity: OPACITY,
        multiply: MULTIPLY,
        screen: SCREEN,
        enabled: ENABLED,
    };
    let mut definition = FormDefinition {
        fields: common
            .identity()
            .into_iter()
            .chain(common.appearance())
            .chain([common.enabled()])
            .collect(),
        ..Default::default()
    };
    let mut automatic = FieldSpec::new(ORIGINAL_AUTO, "properties.deformer_original_auto", Toggle);
    automatic.section = Some("properties.deformer_original");
    definition.fields.push(automatic);
    if rotation {
        let rotation = FormDefinition::new(&[
            (ORIGIN_X, "properties.rotation_origin_x", Number),
            (ORIGIN_Y, "properties.rotation_origin_y", Number),
            (ANGLE, "properties.rotation_angle", Number),
            (SCALE, "properties.rotation_scale", Number),
            (REFLECT_X, "properties.rotation_reflect_x", Toggle),
            (REFLECT_Y, "properties.rotation_reflect_y", Toggle),
            (BASE_ANGLE, "properties.rotation_base_angle", Number),
        ]);
        definition
            .fields
            .extend(rotation.fields.into_iter().map(|mut field| {
                field.section = Some("properties.deformer_rotation");
                if field.binding.key == SCALE.key {
                    field = field.percent();
                }
                field
            }));
    } else {
        definition.fields.extend(
            [
                FieldSpec::new(COLUMNS, "properties.warp_columns", Number),
                FieldSpec::new(ROWS, "properties.warp_rows", Number),
                FieldSpec::new(BEZIER_COLUMNS, "properties.warp_bezier_columns", Number),
                FieldSpec::new(BEZIER_ROWS, "properties.warp_bezier_rows", Number),
                FieldSpec::new(EDIT_MODE, "properties.warp_edit_mode", Select)
                    .choices(false, edit_modes),
                FieldSpec::new(AUTO_REFORM, "properties.warp_auto_reform", Toggle),
            ]
            .map(|mut field| {
                field.section = Some(
                    if [COLUMNS.key, ROWS.key, BEZIER_COLUMNS.key, BEZIER_ROWS.key]
                        .contains(&field.binding.key)
                    {
                        "properties.deformer_warp"
                    } else {
                        "properties.deformer_warp_authoring"
                    },
                );
                field
            }),
        );
    }
    definition
}
fn edit_modes(_: &Document) -> Vec<Choice> {
    [
        ("local", "properties.warp_mode_local"),
        ("smooth_local", "properties.warp_mode_smooth_local"),
        ("smooth_global", "properties.warp_mode_smooth_global"),
        ("preserve_structure", "properties.warp_mode_structure"),
    ]
    .into_iter()
    .map(|(value, label)| Choice {
        label: t!(label).into_owned().into(),
        value: value.into(),
    })
    .collect()
}

fn parent_choices(
    document: &Document,
    object: &ObjectRef,
    key: PropertyKey,
) -> Option<Vec<Choice>> {
    if key != PARENT.key {
        return None;
    }
    use zipzi_editor_core::{PaletteMove, PalettePosition, PaletteProjection};
    let index = document.object_index();
    Some(
        document
            .palette_move_destinations(PaletteProjection::Deformation)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|destination| {
                let mut current = destination.object.clone();
                let mut seen = std::collections::HashSet::new();
                while let Some(candidate) = current {
                    if candidate == *object || !seen.insert(candidate.id.clone()) {
                        return None;
                    }
                    let parent = index.get(&candidate)?.properties["parent_id"]
                        .as_str()?
                        .to_owned();
                    current =
                        (!parent.is_empty()).then(|| ObjectRef::new(ObjectKind::Deformer, parent));
                }
                if destination
                    .object
                    .as_ref()
                    .is_some_and(|o| document.object_state(o).locked())
                {
                    return None;
                }
                let request = PaletteMove {
                    projection: PaletteProjection::Deformation,
                    sources: vec![object.clone()],
                    target: destination.object.clone(),
                    position: PalettePosition::Inside,
                };
                let title = if destination.path.is_empty() {
                    t!("properties.no_deformer").into_owned()
                } else {
                    destination.path
                };
                let label = match document.palette_deformation_move_support(&request) {
                    Ok(_) => title,
                    Err(error) => {
                        format!("{title} · {}", crate::authoring::authoring_error(&error))
                    }
                };
                Some(Choice {
                    label: label.into(),
                    value: destination.object.map_or_else(String::new, |o| o.id),
                })
            })
            .collect(),
    )
}

pub(super) fn definition(target: &PropertyTarget, editor: &Editor) -> FormDefinition {
    if editor
        .document()
        .is_some_and(|doc| requires_single_selection(doc, &target.object))
    {
        return FormDefinition::default();
    }
    let rotation = target
        .values(editor)
        .is_ok_and(|values| values.get(&KIND.key) == Some(&PropertyValue::Text("rotation".into())));
    fields(rotation)
}

pub(crate) fn requires_single_selection(doc: &Document, object: &ObjectRef) -> bool {
    doc.context().selections.len() > 1 && doc.context().selections.contains(object)
}

pub(crate) fn shape_label(scope: &zipzi_editor_core::PropertyScope) -> &'static str {
    use zipzi_editor_core::PropertyScope::*;
    match scope {
        DeformerKeyform { .. } => "properties.deformer_shape_keyform",
        InterpolatedDeformer => "properties.deformer_shape_interpolated",
        DeformerBlendShape => "properties.deformer_shape_blend",
        _ => "properties.deformer_shape_base",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concrete_forms_have_common_fields_and_only_rotation_has_pose_fields() {
        let warp = fields(false);
        let rotation = fields(true);
        for spec in [NAME, RUNTIME_ID, ENABLED, OPACITY, MULTIPLY, SCREEN] {
            assert!(warp.field(spec.key).is_some());
            assert!(rotation.field(spec.key).is_some());
        }
        for spec in [
            ORIGIN_X, ORIGIN_Y, ANGLE, SCALE, REFLECT_X, REFLECT_Y, BASE_ANGLE,
        ] {
            assert!(warp.field(spec.key).is_none());
            assert!(rotation.field(spec.key).is_some());
        }
        assert_eq!(rotation.field(SCALE.key).unwrap().scale, 100.);
        assert!(!rotation.field(BASE_ANGLE.key).unwrap().binding.keyframed);
    }

    #[test]
    fn basic_fields_share_mesh_presentation_and_retain_deformer_bindings() {
        let mesh = mesh::fields();
        for rotation in [false, true] {
            let definition = fields(rotation);
            let basic: Vec<_> = definition
                .fields
                .iter()
                .filter(|f| f.section.is_none())
                .collect();
            assert_eq!(basic.len(), 8);
            for (deformer, mesh) in basic.into_iter().zip(mesh.fields.iter().filter(|f| {
                [
                    "name",
                    "runtime_id",
                    "part_id",
                    "deformer_id",
                    "opacity",
                    "multiply",
                    "screen",
                    "enabled",
                ]
                .contains(&f.binding.key.name())
            })) {
                assert_eq!(deformer.binding.key.object_kind(), ObjectKind::Deformer);
                assert_eq!(deformer.editor, mesh.editor);
                assert_eq!(deformer.label, mesh.label);
                assert_eq!(deformer.scale, mesh.scale);
                assert_eq!(deformer.searchable, mesh.searchable);
                assert_eq!(deformer.reset_color, mesh.reset_color);
            }
            assert_eq!(
                definition.field(OPACITY.key).unwrap().binding.max,
                f32::MAX as f64
            );
            assert_eq!(definition.field(PARENT.key).unwrap().binding, PARENT);
        }
    }

    #[test]
    fn deformer_parent_choices_exclude_self_and_descendants_and_explain_unsupported_moves()
    -> anyhow::Result<()> {
        let (doc, _assets) = crate::test_fixture::sample_document_with_scene_objects()?;
        let rotation = crate::test_fixture::object(ObjectKind::Deformer, 501);
        let warp = crate::test_fixture::object(ObjectKind::Deformer, 502);
        let choices = parent_choices(&doc, &rotation, PARENT.key).unwrap();
        assert_eq!(choices.len(), 1);
        assert!(choices[0].value.is_empty());
        let choices = parent_choices(&doc, &warp, PARENT.key).unwrap();
        assert!(choices.iter().all(|c| c.value != warp.id));
        let root = choices.iter().find(|c| c.value.is_empty()).unwrap();
        assert_ne!(root.label.as_ref(), t!("properties.no_deformer").as_ref());
        assert!(choices.iter().any(|c| c.value == rotation.id));
        Ok(())
    }
}
