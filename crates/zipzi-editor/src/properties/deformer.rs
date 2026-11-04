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
            (BONE_LENGTH, "properties.rotation_bone_length", Number),
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
                field.display_decimals = if [ORIGIN_X.key, ORIGIN_Y.key]
                    .contains(&field.binding.key)
                {
                    Some(2)
                } else if [ANGLE.key, BASE_ANGLE.key, BONE_LENGTH.key].contains(&field.binding.key)
                {
                    Some(1)
                } else if field.binding.key == SCALE.key {
                    Some(3)
                } else {
                    None
                };
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
    use zipzi_editor_core::PaletteProjection;
    let started = gpui_kit::frame_trace::enabled().then(std::time::Instant::now);
    let index = document.object_index();
    let choices: Vec<_> = document
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
            // Full support checks walk the source subtree and parameter domain.
            // Validate the chosen move at commit, not every potential parent
            // while selecting an object and building its inspector.
            let label = if destination.path.is_empty() {
                t!("properties.no_deformer").into_owned()
            } else {
                destination.path
            };
            Some(Choice {
                label: label.into(),
                value: destination.object.map_or_else(String::new, |o| o.id),
            })
        })
        .collect();
    if let Some(started) = started {
        gpui_kit::frame_trace::emit(
            gpui_kit::frame_trace::next_id(),
            "deformer_parent_choices",
            &[
                ("duration_ns", started.elapsed().as_nanos() as u64),
                ("choices", choices.len() as u64),
            ],
        );
    }
    Some(choices)
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
            ORIGIN_X,
            ORIGIN_Y,
            ANGLE,
            SCALE,
            BONE_LENGTH,
            REFLECT_X,
            REFLECT_Y,
            BASE_ANGLE,
        ] {
            assert!(warp.field(spec.key).is_none());
            assert!(rotation.field(spec.key).is_some());
        }
        assert!(!rotation.field(BONE_LENGTH.key).unwrap().binding.keyframed);
        assert_eq!(rotation.field(SCALE.key).unwrap().scale, 100.);
        assert!(!rotation.field(BASE_ANGLE.key).unwrap().binding.keyframed);
    }

    #[test]
    fn rotation_display_truncates_per_field_without_quantizing_authored_values() {
        use crate::properties::form::PropertyForm;
        let cases = [
            (
                ORIGIN_X,
                PropertyValue::Double(10_000_000.23456789),
                "10000000.23",
            ),
            (ORIGIN_Y, PropertyValue::Double(-12.39999), "-12.39"),
            (ANGLE, PropertyValue::Float(-23.987), "-23.9"),
            (BASE_ANGLE, PropertyValue::Float(12.999), "12.9"),
            (BONE_LENGTH, PropertyValue::Float(65.28785), "65.2"),
            (SCALE, PropertyValue::Float(0.0002667378), "0.026"),
        ];
        let mut form = PropertyForm::default();
        form.sync_definition(
            fields(true),
            cases
                .iter()
                .map(|(spec, value, _)| (spec.key, value.clone()))
                .collect(),
        );
        for (spec, value, expected) in &cases {
            assert_eq!(form.fields[&spec.key].raw, *expected);
            assert_eq!(form.fields[&spec.key].parsed, Ok(value.clone()));
            form.edit(spec, expected.to_string());
            assert!(
                form.patch(ObjectKind::Deformer, spec.key)
                    .unwrap()
                    .changes
                    .is_empty()
            );
            form.reload_field(spec.key);
            assert_eq!(form.fields[&spec.key].raw, *expected);
        }
        form.edit(&ORIGIN_X, "10000000.3456789".into());
        assert_eq!(
            form.patch(ObjectKind::Deformer, ORIGIN_X.key)
                .unwrap()
                .changes[&ORIGIN_X.key],
            PropertyValue::Double(10_000_000.3456789)
        );
        let value = PropertyValue::Float(0.0000001);
        form.sync_definition(fields(true), [(SCALE.key, value.clone())].into());
        assert_ne!(form.fields[&SCALE.key].raw, "0");
        assert_eq!(form.fields[&SCALE.key].parsed, Ok(value));
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
    fn deformer_parent_choices_exclude_cycles_and_locks_and_validate_the_committed_move()
    -> anyhow::Result<()> {
        let (mut doc, _assets) = crate::test_fixture::sample_document_with_scene_objects()?;
        let rotation = crate::test_fixture::object(ObjectKind::Deformer, 501);
        let warp = crate::test_fixture::object(ObjectKind::Deformer, 502);
        let choices = parent_choices(&doc, &rotation, PARENT.key).unwrap();
        assert_eq!(choices.len(), 1);
        assert!(choices[0].value.is_empty());
        let choices = parent_choices(&doc, &warp, PARENT.key).unwrap();
        assert!(choices.iter().all(|c| c.value != warp.id));
        let root = choices.iter().find(|c| c.value.is_empty()).unwrap();
        assert_eq!(root.label.as_ref(), t!("properties.no_deformer").as_ref());
        assert!(choices.iter().any(|c| c.value == rotation.id));
        let before = doc
            .object_index()
            .get(&rotation)
            .unwrap()
            .properties
            .clone();
        let history = doc.history_lengths();
        assert!(
            doc.set_object_property(&rotation, PARENT.path, serde_json::json!(warp.id))
                .is_err()
        );
        assert!(
            doc.set_object_property(&warp, PARENT.path, serde_json::json!(warp.id))
                .is_err()
        );
        assert_eq!(
            doc.object_index().get(&rotation).unwrap().properties,
            before
        );
        assert_eq!(doc.history_lengths(), history);
        doc.set_objects_locked(std::slice::from_ref(&rotation), true)?;
        let choices = parent_choices(&doc, &warp, PARENT.key).unwrap();
        assert!(choices.iter().all(|c| c.value != rotation.id));
        doc.set_objects_locked(std::slice::from_ref(&warp), true)?;
        let history = doc.history_lengths();
        assert!(
            doc.set_object_property(&warp, PARENT.path, serde_json::json!(""))
                .is_err()
        );
        assert_eq!(doc.history_lengths(), history);
        Ok(())
    }
}
