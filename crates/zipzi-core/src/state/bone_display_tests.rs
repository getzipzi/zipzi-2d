use super::*;
use crate::model_validation::bone_display::rotation_bone_ray_length as length;
use crate::*;
fn id(n: u32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn animated_rotations() -> DocumentBuilder {
    let mut doc = DocumentState::new(id(1), Canvas::new(100., 100., Vec2::default(), 1.).into())
        .unwrap()
        .builder();
    for (n, kind, default_value) in [
        (2, ParameterKind::Normal, 0.),
        (3, ParameterKind::BlendShape, 0.5),
    ] {
        assert!(doc
            .create_parameter(Parameter {
                id: id(n),
                kind,
                default_value,
                minimum: -1.,
                maximum: 1.,
                ..Default::default()
            })
            .is_ok());
    }
    for (n, parent, scale) in [(4, None, 0.25), (6, Some(id(5)), 0.1)] {
        assert!(doc
            .create_transform(Transform {
                id: id(n),
                runtime_id: id(n),
                parent_id: parent.map(Into::into),
                enabled: n != 4,
                data: TransformData::Rotation(RotationTransform {
                    pose: RotationPose {
                        scale,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                ..Default::default()
            })
            .is_ok());
        let scales = if n == 4 {
            [0.25, 4., 6.]
        } else {
            [0.1, 1., 2.]
        };
        assert!(doc
            .create_scene_binding(SceneBinding {
                id: id(n + 10),
                axes: vec![BindingAxis {
                    parameter_id: id(2),
                    keys: vec![-1., 0., 1.],
                    ..Default::default()
                }],
                track: SceneTrack::Rotation {
                    target_id: id(n).into(),
                    keyforms: [-1., 0., 1.]
                        .into_iter()
                        .zip(scales)
                        .map(|(key, scale)| RotationKeyform {
                            keys: vec![key],
                            rotation: RotationPose {
                                scale,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .collect(),
                },
            })
            .is_ok());
        if n == 4 {
            assert!(doc
                .create_transform(Transform {
                    id: id(5),
                    runtime_id: id(5),
                    parent_id: Some(id(4).into()),
                    data: TransformData::Warp(WarpTransform {
                        bezier: None,
                        rows: 1,
                        columns: 1,
                        quad: true,
                        points: vec![
                            Vec2::new(0., 0.),
                            Vec2::new(100., 0.),
                            Vec2::new(0., 100.),
                            Vec2::new(100., 100.)
                        ],
                    }),
                    ..Default::default()
                })
                .is_ok());
        }
    }
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            id: id(20),
            parameter_id: id(3),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: Default::default(),
            base_sample_id: Default::default(),
        })
        .is_ok());
    assert!(doc
        .create_blend_binding(BlendShapeBinding {
            id: id(21),
            target_id: id(6),
            target_kind: BlendShapeTargetKind::Rotation,
            key_table_id: id(20),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Rotation(vec![
                DeltaRotationKeyform::default(),
                DeltaRotationKeyform {
                    scale: Some(0.5),
                    ..Default::default()
                },
            ]),
        })
        .is_ok());
    doc
}

#[test]
fn rotation_reference_uses_default_evaluation_with_warp_inheritance_blends_and_hidden_objects() {
    for native in [false, true] {
        let mut doc = animated_rotations();
        // Imported numeric axes have no identities yet. Initialization must
        // normalize them before validating and evaluating the complete graph.
        if !native {
            doc.initialize_rotation_bone_reference_scales().unwrap();
            assert!(!doc.scene_binding(&id(14)).unwrap().axes[0].id.is_empty());
        }
        let mut before = crate::DrawableFrame::default();
        assert!(crate::evaluation::evaluate_frame_including_hidden(
            &doc.preview().unwrap(),
            &Default::default(),
            &mut before
        )
        .is_ok());
        assert_eq!(
            doc.content.model().rotation_bone_reference_scale(&id(4)),
            Some(4.)
        );
        assert_eq!(
            doc.content.model().rotation_bone_reference_scale(&id(6)),
            Some(5.)
        );
        if native {
            doc.scene_initializations.extend([id(4), id(6)]);
            doc.validate_candidate().unwrap();
        } else {
            assert!(doc.initialize_rotation_bone_reference_scales().is_ok());
        }
        assert_eq!(
            doc.editor_state().rotation_bone_reference_scales[&id(4)],
            4.
        );
        assert_eq!(
            doc.editor_state().rotation_bone_reference_scales[&id(6)],
            5.
        );
        let mut after = crate::DrawableFrame::default();
        assert!(crate::evaluation::evaluate_frame_including_hidden(
            &doc.preview().unwrap(),
            &Default::default(),
            &mut after
        )
        .is_ok());
        assert_eq!(before.deformers, after.deformers);
        assert_eq!(before.drawables, after.drawables);
    }
}

#[test]
fn default_evaluated_rotation_reference_stays_frozen_after_later_animation_edits() {
    let mut doc = animated_rotations();
    assert!(doc.initialize_rotation_bone_reference_scales().is_ok());
    let state = doc.editor_state().clone();
    let mut parameter = doc.parameter(&id(2)).unwrap().clone();
    parameter.default_value = 1.;
    assert!(doc.replace_parameter(parameter).is_ok());
    assert!(doc.initialize_rotation_bone_reference_scales().is_ok());
    assert_eq!(doc.editor_state(), &state);
    assert_eq!(
        doc.content.model().rotation_bone_reference_scale(&id(6)),
        Some(5.)
    );
}

#[test]
fn ray_length_uses_direction_and_off_center_origin() {
    let bounds = [-100., -20., 100., 20.];
    for (origin, direction, expected) in [
        ([0., 0.], [1., 0.], 85.),
        ([0., 0.], [0., 1.], 17.),
        ([80., 10.], [1., 0.], 17.),
        ([80., 10.], [-1., 0.], 153.),
        ([80., 10.], [0., -1.], 25.5),
        ([0., 0.], [1., 1.], 17. * 2_f64.sqrt()),
        ([200., 0.], [-1., 0.], 270.),
    ] {
        assert!((length(origin, direction, bounds).unwrap() - expected).abs() < 1e-9);
    }
}
#[test]
fn ray_length_rejects_misses_and_invalid_or_degenerate_inputs() {
    let bounds = [-100., -20., 100., 20.];
    assert_eq!(length([200., 0.], [1., 0.], bounds), None);
    assert_eq!(length([200., 0.], [0., 1.], bounds), None);
    assert_eq!(length([0., 0.], [0., 0.], bounds), None);
    assert_eq!(length([f64::NAN, 0.], [1., 0.], bounds), None);
    assert_eq!(length([0., 0.], [1., 0.], [0.; 4]), None);
}
