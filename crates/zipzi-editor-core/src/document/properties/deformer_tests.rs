use super::*;
use crate::PropertyValue;
use crate::property::deformer as fields;
use serde_json::json;
use zipzi_core::{
    BindingAxis, BindingAxisEditing, RotationPose, SceneTrack, Transform, TransformData,
};

fn fixture(rotation: bool, bound: bool) -> Result<(Document, ObjectRef, String)> {
    let mut doc = Document::new_blank()?;
    let object = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::new_v4().to_string());
    let parameter = uuid::Uuid::new_v4().to_string();
    let pose = RotationPose {
        origin: zipzi_core::PreciseVec2::new(10_000_000.123_456_79, -23.987_654_321),
        angle: 720.,
        ..Default::default()
    };
    doc.session.edit("Fixture", None, |edit| {
        let data = if rotation {
            TransformData::Rotation(zipzi_core::RotationTransform {
                base_angle: 15.,
                pose,
            })
        } else {
            TransformData::Warp(zipzi_core::WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                bezier: None,
                points: vec![
                    zipzi_core::Vec2::new(0., 0.),
                    zipzi_core::Vec2::new(100., 0.),
                    zipzi_core::Vec2::new(0., 100.),
                    zipzi_core::Vec2::new(100., 100.),
                ],
            })
        };
        edit.create_transform(Transform {
            id: object.id.clone(),
            name: "Deformer".into(),
            runtime_id: "Deformer".into(),
            data,
            appearance: zipzi_core::Appearance {
                opacity: 1.00005,
                ..Default::default()
            },
            ..Default::default()
        })?;
        if bound {
            edit.create_parameter(zipzi_core::Parameter {
                id: parameter.clone(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            let track = if rotation {
                SceneTrack::Rotation {
                    target_id: object.id.clone().into(),
                    keyforms: [0., 1.]
                        .map(|key| zipzi_core::RotationKeyform {
                            keys: vec![key],
                            rotation: RotationPose {
                                angle: key * 90.,
                                ..pose
                            },
                            appearance: zipzi_core::Appearance {
                                opacity: 0.2 + key * 0.4,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .to_vec(),
                }
            } else {
                SceneTrack::Warp {
                    target_id: object.id.clone().into(),
                    keyforms: [0., 1.]
                        .map(|key| zipzi_core::WarpKeyform {
                            keys: vec![key],
                            positions: edit
                                .candidate_document()
                                .get_transform(&object.id)
                                .unwrap()
                                .warp()
                                .unwrap()
                                .points
                                .clone(),
                            appearance: zipzi_core::Appearance {
                                opacity: 0.2 + key * 0.4,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .to_vec(),
                }
            };
            edit.create_scene_binding(zipzi_core::SceneBinding {
                id: uuid::Uuid::new_v4().to_string(),
                axes: vec![BindingAxis::new(parameter.clone(), vec![0., 1.])],
                track,
            })?;
        }
        Ok(())
    })?;
    doc.invalidate_projection(true);
    Ok((doc, object, parameter))
}

#[test]
fn rotation_fields_preserve_origin_precision_and_definition_scope() -> Result<()> {
    let (mut doc, object, parameter) = fixture(true, true)?;
    let base = doc.model().get_transform(&object.id).unwrap().clone();
    let original_binding = doc.model().binding_for_scene(&object.id).unwrap().clone();
    assert!(matches!(
        doc.property_scope(&object),
        PropertyScope::DeformerKeyform { .. }
    ));
    assert_eq!(
        doc.property_values(&object)?[&fields::ANGLE.key],
        PropertyValue::Float(0.)
    );
    let x = 10_000_000.234_567_89;
    doc.set_object_property(&object, fields::ORIGIN_X.path, json!(x))?;
    doc.set_object_property(&object, fields::ANGLE.path, json!(1080.))?;
    doc.set_object_property(&object, fields::SCALE.path, json!(0.125))?;
    doc.set_object_property(&object, fields::REFLECT_X.path, json!(true))?;
    let model = doc.model();
    assert_eq!(model.get_transform(&object.id), Some(&base));
    let SceneTrack::Rotation { keyforms, .. } = &model.binding_for_scene(&object.id).unwrap().track
    else {
        unreachable!()
    };
    assert_eq!(keyforms[0].rotation.origin.x, x);
    assert_eq!(
        keyforms[0].rotation.origin.y,
        base.rotation().unwrap().pose.origin.y
    );
    assert_eq!(keyforms[0].rotation.angle, 1080.);
    assert_eq!(keyforms[0].rotation.scale, 0.125);
    assert!(keyforms[0].rotation.reflect_x);
    let SceneTrack::Rotation {
        keyforms: original, ..
    } = &original_binding.track
    else {
        unreachable!()
    };
    assert_eq!(keyforms[1], original[1]);
    let edited_binding = model.binding_for_scene(&object.id).unwrap().clone();
    drop(model);
    // Definition-level controls remain writable between ordinary keyforms.
    doc.set_preview_parameter(&parameter, 0.5)?;
    assert_eq!(
        doc.property_scope(&object),
        PropertyScope::InterpolatedDeformer
    );
    doc.set_object_property(&object, fields::BASE_ANGLE.path, json!(-30.))?;
    assert_eq!(
        doc.model()
            .get_transform(&object.id)
            .unwrap()
            .rotation()
            .unwrap()
            .base_angle,
        -30.
    );
    assert_eq!(
        doc.model().binding_for_scene(&object.id),
        Some(&edited_binding)
    );
    doc.undo()?;
    assert_eq!(doc.model().get_transform(&object.id), Some(&base));
    Ok(())
}

#[test]
fn deformer_appearance_preview_is_local_atomic_and_one_undo_step() -> Result<()> {
    for rotation in [false, true] {
        for bound in [false, true] {
            let (mut doc, object, _) = fixture(rotation, bound)?;
            let baseline = doc.property_values(&object)?;
            let old_transform = doc.model().get_transform(&object.id).unwrap().clone();
            let old_binding = doc.model().binding_for_scene(&object.id).cloned();
            let history = doc.history_lengths();
            let patch = doc.object_property_patch(&object, fields::OPACITY.path, &json!(0.4))?;
            let serial = doc.begin_property_edit(&object, &patch)?;
            for value in [0.3, 0.7, 0.4] {
                let patch =
                    doc.object_property_patch(&object, fields::OPACITY.path, &json!(value))?;
                doc.update_property_edit(&object, serial, &patch)?;
                assert_eq!(doc.property_values(&object)?, baseline);
                let preview = Document::property_values_in(&doc.model(), &object, &patch.scope)?;
                assert_eq!(
                    preview[&fields::OPACITY.key],
                    PropertyValue::Float(value as f32)
                );
            }
            doc.cancel_edit();
            assert_eq!(doc.history_lengths(), history);
            assert_eq!(doc.model().get_transform(&object.id), Some(&old_transform));
            assert_eq!(
                doc.model().binding_for_scene(&object.id),
                old_binding.as_ref()
            );
            let serial = doc.begin_property_edit(&object, &patch)?;
            doc.update_property_edit(&object, serial, &patch)?;
            doc.finish_edit()?;
            assert_eq!(doc.history_lengths().0, history.0 + 1);
            assert_eq!(
                doc.property_values(&object)?[&fields::OPACITY.key],
                PropertyValue::Float(0.4)
            );
            if bound {
                assert_eq!(doc.model().get_transform(&object.id), Some(&old_transform));
            }
            doc.undo()?;
            assert_eq!(doc.model().get_transform(&object.id), Some(&old_transform));
            assert_eq!(
                doc.model().binding_for_scene(&object.id),
                old_binding.as_ref()
            );
            doc.redo()?;
            assert_eq!(
                doc.property_values(&object)?[&fields::OPACITY.key],
                PropertyValue::Float(0.4)
            );
        }
    }
    Ok(())
}

#[test]
fn deformer_rejects_interpolated_stale_and_wrong_type_fields_without_history() -> Result<()> {
    let (mut doc, object, parameter) = fixture(true, true)?;
    let patch = doc.object_property_patch(&object, fields::ANGLE.path, &json!(60.))?;
    let history = doc.history_lengths();
    doc.set_preview_parameter(&parameter, 1.)?;
    assert!(doc.set_object_properties(&object, &patch).is_err());
    doc.set_preview_parameter(&parameter, 0.5)?;
    assert!(
        doc.set_object_property(&object, fields::ANGLE.path, json!(60.))
            .is_err()
    );
    assert!(
        doc.set_object_property(&object, fields::MULTIPLY.path, json!([0.3, 0.4, 0.5]))
            .is_err()
    );
    assert_eq!(doc.history_lengths(), history);
    doc.set_preview_parameter(&parameter, 0.)?;
    assert!(
        doc.set_object_property(&object, fields::SCALE.path, json!(0.))
            .is_err()
    );
    assert!(
        doc.set_object_property(&object, fields::SCALE.path, json!(-1.))
            .is_err()
    );
    assert_eq!(doc.history_lengths(), history);
    let (mut warp, object, _) = fixture(false, false)?;
    assert!(
        !warp
            .property_values(&object)?
            .contains_key(&fields::ANGLE.key)
    );
    assert!(
        warp.set_object_property(&object, fields::ANGLE.path, json!(30.))
            .is_err()
    );
    let payload = crate::property::deformer::inspector_payload(
        &warp.model(),
        &object.id,
        &PropertyScope::Object,
    )?;
    assert!(payload.get("points").is_none() && payload.get("data").is_none());
    Ok(())
}

#[test]
fn imported_opacity_and_precise_base_origin_survive_noop_and_other_field_edits() -> Result<()> {
    let (mut doc, object, _) = fixture(true, false)?;
    let values = doc.property_values(&object)?;
    assert_eq!(values[&fields::OPACITY.key], PropertyValue::Float(1.00005));
    let x = values[&fields::ORIGIN_X.key].number().unwrap();
    assert_ne!(x, f64::from(x as f32));
    let history = doc.history_lengths();
    doc.set_object_property(&object, fields::ORIGIN_X.path, json!(x))?;
    assert_eq!(doc.history_lengths(), history);
    doc.set_object_property(&object, fields::REFLECT_Y.path, json!(true))?;
    doc.set_object_property(&object, fields::MULTIPLY.path, json!([0.25, 0.5, 0.75]))?;
    let after = doc.property_values(&object)?;
    assert_eq!(after[&fields::ORIGIN_X.key], values[&fields::ORIGIN_X.key]);
    assert_eq!(after[&fields::ORIGIN_Y.key], values[&fields::ORIGIN_Y.key]);
    assert_eq!(after[&fields::OPACITY.key], values[&fields::OPACITY.key]);
    Ok(())
}

#[test]
fn deformer_blend_contributions_reject_shape_edits_and_neutral_shapes_remain_editable() -> Result<()>
{
    use zipzi_core::*;
    for rotation in [false, true] {
        let (mut doc, object, _) = fixture(rotation, false)?;
        let parameter = uuid::Uuid::new_v4().to_string();
        let table = uuid::Uuid::new_v4().to_string();
        doc.session.edit("Blend fixture", None, |edit| {
            edit.create_parameter(Parameter {
                id: parameter.clone(),
                kind: ParameterKind::BlendShape,
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            edit.create_blend_key_table(BlendShapeKeyTable {
                id: table.clone(),
                parameter_id: parameter.clone(),
                keys: vec![0., 1.],
                base_key_idx: 0,
                samples: Default::default(),
                base_sample_id: Default::default(),
            })?;
            edit.create_blend_binding(BlendShapeBinding {
                id: uuid::Uuid::new_v4().to_string(),
                target_id: object.id.clone(),
                target_kind: if rotation {
                    BlendShapeTargetKind::Rotation
                } else {
                    BlendShapeTargetKind::Warp
                },
                key_table_id: table.clone(),
                constraint_ids: vec![],
                keyforms: if rotation {
                    DeltaKeyforms::Rotation(vec![
                        DeltaRotationKeyform::default(),
                        DeltaRotationKeyform {
                            angle: Some(10.),
                            ..Default::default()
                        },
                    ])
                } else {
                    DeltaKeyforms::Warp(vec![
                        DeltaWarpKeyform {
                            points: vec![Vec2::default(); 4],
                            ..Default::default()
                        },
                        DeltaWarpKeyform {
                            points: vec![Vec2::new(1., 2.); 4],
                            ..Default::default()
                        },
                    ])
                },
            })
        })?;
        doc.invalidate_projection(true);
        assert_eq!(doc.property_scope(&object), PropertyScope::Object);
        doc.set_object_property(&object, fields::OPACITY.path, json!(0.8))?;
        let base = doc.model().get_transform(&object.id).unwrap().clone();
        doc.set_preview_parameter(&parameter, 0.5)?;
        assert_eq!(
            doc.property_scope(&object),
            PropertyScope::DeformerBlendShape
        );
        let history = doc.history_lengths();
        assert!(
            doc.set_object_property(&object, fields::OPACITY.path, json!(0.4))
                .is_err()
        );
        if rotation {
            assert!(
                doc.set_object_property(&object, fields::ANGLE.path, json!(50.))
                    .is_err()
            );
        }
        assert_eq!(doc.history_lengths(), history);
        assert_eq!(doc.model().get_transform(&object.id), Some(&base));
        doc.set_preview_parameter(&parameter, 0.)?;
        assert_eq!(doc.property_scope(&object), PropertyScope::Object);
        doc.set_object_property(&object, fields::OPACITY.path, json!(0.6))?;
        doc.set_objects_locked(std::slice::from_ref(&object), true)?;
        let history = doc.history_lengths();
        assert!(
            doc.set_object_property(&object, fields::OPACITY.path, json!(0.4))
                .is_err()
        );
        assert_eq!(doc.history_lengths(), history);
    }
    Ok(())
}

#[test]
fn repeating_parameter_upper_endpoint_targets_the_wrapped_deformer_keyform() -> Result<()> {
    let (mut doc, object, parameter) = fixture(true, true)?;
    doc.session.edit("Repeat", None, |edit| {
        let mut next = edit
            .candidate_document()
            .get_parameter(&parameter)
            .unwrap()
            .clone();
        next.repeat = true;
        edit.replace_parameter(next)
    })?;
    doc.invalidate_projection(true);
    let scope = doc.property_scope(&object);
    doc.set_preview_parameter(&parameter, 1.)?;
    assert_eq!(doc.property_scope(&object), scope);
    doc.set_object_property(&object, fields::ANGLE.path, json!(45.))?;
    let model = doc.model();
    let SceneTrack::Rotation { keyforms, .. } = &model.binding_for_scene(&object.id).unwrap().track
    else {
        unreachable!()
    };
    assert_eq!(keyforms[0].rotation.angle, 45.);
    assert_eq!(keyforms[1].rotation.angle, 90.);
    Ok(())
}

#[test]
fn deformer_reference_properties_share_organization_and_preserving_tree_plans() -> Result<()> {
    use zipzi_core::{Mesh, Part, PreciseVec2, RotationTransform, Vec2};
    let (mut doc, object, parameter) = fixture(true, true)?;
    let parent = uuid::Uuid::new_v4().to_string();
    let part = uuid::Uuid::new_v4().to_string();
    let mesh = uuid::Uuid::new_v4().to_string();
    doc.session.edit("Reference fixture", None, |edit| {
        edit.create_part(Part {
            id: part.clone(),
            name: "Part".into(),
            ..Default::default()
        })?;
        edit.create_transform(Transform {
            id: parent.clone(),
            name: "Parent".into(),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 0.,
                pose: RotationPose {
                    origin: PreciseVec2::new(10., 20.),
                    angle: 30.,
                    ..Default::default()
                },
            }),
            ..Default::default()
        })?;
        let mut transform = edit
            .candidate_document()
            .get_transform(&object.id)
            .unwrap()
            .clone();
        transform.rotation_mut().unwrap().pose.origin = PreciseVec2::default();
        edit.replace_transform(transform)?;
        let mut binding = edit
            .candidate_document()
            .binding_for_scene(&object.id)
            .unwrap()
            .clone();
        let SceneTrack::Rotation { keyforms, .. } = &mut binding.track else {
            unreachable!()
        };
        for form in keyforms {
            form.rotation.origin = PreciseVec2::default();
        }
        edit.replace_scene_binding(binding)?;
        edit.create_mesh(Mesh {
            id: mesh.clone(),
            name: "Child".into(),
            deformer_id: object.id.clone(),
            vertex_ids: vec![0, 1, 2],
            base_positions: vec![Vec2::new(1., 2.), Vec2::new(11., 2.), Vec2::new(1., 12.)],
            uvs: vec![Vec2::default(); 3],
            triangles: vec![[0, 1, 2]],
            ..Default::default()
        })
    })?;
    doc.invalidate_projection(true);
    let positions = |doc: &Document| {
        [0., 0.5, 1.]
            .into_iter()
            .map(|value| {
                zipzi_core::evaluation::authoring_mesh_positions(
                    &doc.model(),
                    &mesh,
                    &[(parameter.clone(), value)].into(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>()
    };
    let before = positions(&doc);
    let history = doc.history_lengths().0;
    doc.set_object_property(&object, fields::PART.path, json!(part))?;
    assert_eq!(doc.model().get_transform(&object.id).unwrap().part(), part);
    doc.set_object_property(&object, fields::PARENT.path, json!(parent))?;
    assert_eq!(
        doc.model().get_transform(&object.id).unwrap().parent(),
        parent
    );
    for (a, b) in before
        .iter()
        .flatten()
        .zip(positions(&doc).iter().flatten())
    {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.01, "{a:?} != {b:?}");
    }
    assert_eq!(doc.history_lengths().0, history + 2);
    let frozen = doc.model().get_transform(&object.id).unwrap().clone();
    assert!(
        doc.set_object_property(&object, fields::PARENT.path, json!(object.id))
            .is_err()
    );
    let parent_object = ObjectRef::new(ObjectKind::Deformer, &parent);
    assert!(
        doc.set_object_property(&parent_object, fields::PARENT.path, json!(object.id))
            .is_err()
    );
    assert_eq!(doc.model().get_transform(&object.id), Some(&frozen));
    assert_eq!(doc.history_lengths().0, history + 2);
    doc.undo()?;
    assert_eq!(doc.model().get_transform(&object.id).unwrap().parent(), "");
    assert_eq!(positions(&doc), before);
    doc.undo()?;
    assert_eq!(doc.model().get_transform(&object.id).unwrap().part(), "");
    doc.redo()?;
    doc.redo()?;
    assert_eq!(
        doc.model().get_transform(&object.id).unwrap().parent(),
        parent
    );
    Ok(())
}

#[test]
fn warp_grid_preview_resamples_all_shapes_preserves_controllers_and_reports_loss() -> Result<()> {
    use zipzi_core::*;
    let (mut doc, object, _) = fixture(false, true)?;
    let blend = uuid::Uuid::new_v4().to_string();
    doc.session.edit("Curved Warp fixture", None, |edit| {
        edit.edit_scene_content(zipzi_sdk::SceneContentEdit::ResizeWarp {
            transform_id: object.id.clone(),
            rows: 2,
            columns: 2,
            quad: true,
        })?;
        let mut transform = edit
            .candidate_document()
            .get_transform(&object.id)
            .unwrap()
            .clone();
        let grid = transform.warp_mut().unwrap();
        grid.points[4].x += 25.;
        grid.bezier = Some(WarpBezier::from_lattice(2, 2, true, &grid.points));
        let mut binding = edit
            .candidate_document()
            .binding_for_scene(&object.id)
            .unwrap()
            .clone();
        let SceneTrack::Warp { keyforms, .. } = &mut binding.track else {
            unreachable!()
        };
        for form in keyforms {
            form.positions[4].y += 20.;
            form.bezier = Some(WarpBezier::from_lattice(2, 2, true, &form.positions));
        }
        edit.replace_warp_authoring(transform, Some(binding))?;
        let parameter = uuid::Uuid::new_v4().to_string();
        let table = uuid::Uuid::new_v4().to_string();
        edit.create_parameter(Parameter {
            id: parameter.clone(),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })?;
        edit.create_blend_key_table(BlendShapeKeyTable {
            id: table.clone(),
            parameter_id: parameter,
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: Default::default(),
            base_sample_id: Default::default(),
        })?;
        let mut delta = vec![Vec2::default(); 9];
        delta[4] = Vec2::new(5., 6.);
        edit.create_blend_binding(BlendShapeBinding {
            id: blend.clone(),
            target_id: object.id.clone(),
            target_kind: BlendShapeTargetKind::Warp,
            key_table_id: table,
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Warp(vec![
                DeltaWarpKeyform {
                    points: vec![Vec2::default(); 9],
                    ..Default::default()
                },
                DeltaWarpKeyform {
                    points: delta,
                    ..Default::default()
                },
            ]),
        })
    })?;
    doc.invalidate_projection(true);
    let original = doc.model().get_transform(&object.id).unwrap().clone();
    let binding = doc.model().binding_for_scene(&object.id).unwrap().clone();
    let deltas = doc.model().get_blend_binding(&blend).unwrap().clone();
    let values = doc.property_values(&object)?;
    let patch = PropertyPatch {
        scope: PropertyScope::Object,
        expected: [fields::COLUMNS, fields::ROWS, fields::QUAD]
            .map(|s| (s.key, values[&s.key].clone()))
            .into(),
        changes: [
            (fields::COLUMNS.key, PropertyValue::Integer(1)),
            (fields::ROWS.key, PropertyValue::Integer(1)),
        ]
        .into(),
    };
    let history = doc.history_lengths();
    for commit in [false, true] {
        let serial = doc.begin_property_edit(&object, &patch)?;
        doc.update_property_edit(&object, serial, &patch)?;
        let metrics = doc.warp_grid_preview_metrics(&object)?;
        assert_eq!(metrics.shapes, 5);
        assert_eq!(metrics.samples_per_shape, 293);
        assert!(metrics.maximum_sample_error > 10.);
        assert_eq!(doc.history_lengths(), history);
        let model = doc.model();
        let warp = model.get_transform(&object.id).unwrap().warp().unwrap();
        assert_eq!(warp.points.len(), 4);
        assert_eq!(warp.bezier, original.warp().unwrap().bezier);
        let SceneTrack::Warp { keyforms, .. } = &model.binding_for_scene(&object.id).unwrap().track
        else {
            unreachable!()
        };
        let SceneTrack::Warp { keyforms: old, .. } = &binding.track else {
            unreachable!()
        };
        for (form, old) in keyforms.iter().zip(old) {
            assert_eq!(form.positions.len(), 4);
            assert_eq!(form.bezier, old.bezier);
        }
        let DeltaKeyforms::Warp(forms) = &model.get_blend_binding(&blend).unwrap().keyforms else {
            unreachable!()
        };
        assert!(forms.iter().all(|f| f.points.len() == 4));
        drop(model);
        assert_eq!(
            doc.object_index().get(&object).unwrap().properties["warp"]["columns"],
            1
        );
        if commit {
            doc.finish_edit()?;
            doc.undo()?;
        } else {
            doc.cancel_edit();
        }
        assert_eq!(
            doc.object_index().get(&object).unwrap().properties["warp"]["columns"],
            2
        );
        assert_eq!(doc.model().get_transform(&object.id), Some(&original));
        assert_eq!(doc.model().binding_for_scene(&object.id), Some(&binding));
        assert_eq!(doc.model().get_blend_binding(&blend), Some(&deltas));
    }
    doc.redo()?;
    assert_eq!(doc.history_lengths().0, history.0 + 1);
    Ok(())
}

#[test]
fn affine_warp_grid_sampling_stays_close_and_invalid_dimensions_are_atomic() -> Result<()> {
    let (mut doc, object, _) = fixture(false, false)?;
    let patch = doc.object_property_patch(&object, fields::COLUMNS.path, &json!(3))?;
    let serial = doc.begin_property_edit(&object, &patch)?;
    doc.update_property_edit(&object, serial, &patch)?;
    assert!(doc.warp_grid_preview_metrics(&object)?.maximum_sample_error < 0.0001);
    doc.cancel_edit();
    let history = doc.history_lengths();
    for invalid in [0, 65, -1] {
        assert!(
            doc.set_object_property(&object, fields::ROWS.path, json!(invalid))
                .is_err()
        );
    }
    assert_eq!(doc.history_lengths(), history);
    assert_eq!(
        doc.property_values(&object)?[&fields::ROWS.key],
        PropertyValue::Integer(1)
    );
    Ok(())
}
