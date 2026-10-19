mod common;
use common::*;
use kasane_cmo3::*;
use kasane_core::{
    document::*,
    evaluation::{evaluate_frame_including_hidden, PreviewValues},
    DrawableFrame,
};

fn uuid(n: u32) -> String {
    format!("ca030001-0000-4000-8000-{n:012x}")
}

fn initial_scene(n: u32, frames: f32) -> MotionClip {
    MotionClip {
        id: uuid(n),
        // Deliberately no expression-name convention.
        name: format!("Scene {n}"),
        duration: frames / 30.,
        fps: 30.,
        looping: false,
        restricted_beziers: true,
        fade_in: Some(0.2),
        fade_out: Some(0.5),
        tracks: vec![MotionTrack {
            id: uuid(n + 1),
            target: MotionTrackTarget::Parameter {
                parameter_id: PARAM.into(),
            },
            initial: MotionKey {
                id: uuid(n + 2),
                time: 0.,
                value: 0.7,
            },
            segments: Default::default(),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        }],
        events: Vec::new(),
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    }
}

#[test]
fn model3_groups_roundtrip_as_native_playlists_without_reclassifying_registered_motions() {
    let (mut source, _) = fixture();
    // Known registrations establish motion identity even for a static pose.
    assert!(source.create_motion(initial_scene(100, 8.)).status.is_ok());
    assert!(source.create_motion(initial_scene(200, 8.)).status.is_ok());
    let registration = MotionRegistration {
        clip_id: uuid(100),
        fade_in: None,
        fade_out: None,
        sound: None,
        extensions: Default::default(),
    };
    let groups = vec![
        MotionGroup {
            name: String::new(),
            entries: vec![registration.clone(), registration.clone()],
        },
        MotionGroup {
            name: "Scenes".into(),
            entries: vec![registration.clone()],
        },
        MotionGroup {
            name: "空分组".into(),
            entries: Vec::new(),
        },
    ];
    assert!(source.set_motion_groups(groups.clone()).status.is_ok());
    let bytes = encode_can3(&source, &Default::default()).unwrap().bytes;
    let (mut imported, _) = fixture();
    import_animation(&bytes, &mut imported).unwrap();
    assert_eq!(imported.motion_groups(), groups);
    assert_eq!(imported.motion_order(), [uuid(100)]);
    assert_eq!(imported.expression_order(), [uuid(200)]);
    let again = encode_can3(&imported, &Default::default()).unwrap().bytes;
    let (mut reopened, _) = fixture();
    import_animation(&again, &mut reopened).unwrap();
    assert_eq!(reopened.motion_groups(), groups);
    for unsupported in ["fade_in", "fade_out", "sound", "extension"] {
        let mut groups = groups.clone();
        let entry = &mut groups[0].entries[0];
        match unsupported {
            "fade_in" => entry.fade_in = Some(0.25),
            "fade_out" => entry.fade_out = Some(0.5),
            "sound" => entry.sound = Some("voice.wav".into()),
            _ => {
                entry
                    .extensions
                    .insert("custom".into(), serde_json::json!(true));
            }
        }
        assert!(imported.set_motion_groups(groups).status.is_ok());
        assert!(encode_can3(&imported, &Default::default()).is_err());
    }
    // Even an empty registration group is metadata a standalone CMO3 cannot
    // carry. With no clips, the ordinary animation-loss guard cannot catch it.
    let (mut groups_only, png) = fixture();
    assert!(groups_only
        .set_motion_groups(vec![MotionGroup {
            name: "Empty".into(),
            entries: Vec::new(),
        }])
        .status
        .is_ok());
    assert!(encode_cmo3(
        &groups_only,
        &|_: &kasane_core::ImageAsset| Ok(png.clone()),
        &Default::default(),
    )
    .is_err());
}

#[test]
fn initial_values_classify_as_expressions_regardless_of_duration_or_name() {
    let (mut source, _) = fixture();
    for (n, frames) in [(100, 1.), (200, 8.), (300, 240.)] {
        assert!(source
            .create_motion(initial_scene(n, frames))
            .status
            .is_ok());
    }
    let bytes = encode_can3(&source, &Default::default()).unwrap().bytes;
    let (mut imported, _) = fixture();
    import_animation(&bytes, &mut imported).unwrap();
    assert!(imported.motion_order().is_empty());
    assert_eq!(imported.expression_order().len(), 3);
    for (n, frames) in [(100, 1.), (200, 8.), (300, 240.)] {
        let expression = imported.get_expression(&uuid(n)).unwrap();
        assert_eq!(expression.name, format!("Scene {n}"));
        assert_eq!(expression.entries.len(), 1);
        assert_eq!(expression.entries[0].value, 0.7);
        assert_eq!(expression.entries[0].blend, Some(ExpressionBlend::Add));
        assert_eq!(expression.fade_in, Some(0.2));
        assert_eq!(expression.fade_out, Some(0.5));
        assert_eq!(expression.source_scene().unwrap().duration, frames / 30.);
        assert!(!expression.has_extensions());
        assert!(expression.runtime_extensions().is_empty());
    }
}

#[test]
fn later_keys_events_and_non_parameter_scenes_remain_motions() {
    let (mut source, _) = fixture();
    let mut later = initial_scene(100, 8.);
    later.name = "exp_01".into();
    // Even equal-valued keys are authored animation, not just initial state.
    later.tracks[0].segments = vec![MotionSegment::Linear {
        end: MotionKey {
            id: uuid(103),
            time: 4. / 30.,
            value: 0.7,
        },
    }]
    .into();
    let mut event = initial_scene(200, 8.);
    event.events.push(MotionEvent {
        id: uuid(203),
        time: 0.,
        value: "event".into(),
        extensions: Default::default(),
    });
    let mut delayed = initial_scene(300, 8.);
    delayed.tracks[0].initial.time = 4. / 30.;
    let mut empty = initial_scene(400, 8.);
    empty.tracks.clear();
    for clip in [later, event, delayed, empty] {
        assert!(source.create_motion(clip).status.is_ok());
    }
    let bytes = encode_can3(&source, &Default::default()).unwrap().bytes;
    let (mut imported, _) = fixture();
    import_animation(&bytes, &mut imported).unwrap();
    assert_eq!(imported.motion_order().len(), 4);
    assert!(imported.expression_order().is_empty());
}

#[test]
fn expression_engineering_roundtrip_preserves_setup_and_uses_current_entries() {
    let (mut source, png) = fixture();
    let mut parameter = source.get_parameter(PARAM).unwrap().clone();
    parameter.default_value = 0.5;
    assert!(source.replace_parameter(parameter).status.is_ok());
    let part = uuid(1);
    assert!(source
        .create_part(kasane_core::Part {
            id: part.clone(),
            runtime_id: "PartArm".into(),
            name: "Arm".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut scene = initial_scene(100, 8.);
    let mut setup = scene.tracks[0].clone();
    setup.id = uuid(110);
    setup.initial.id = uuid(111);
    setup.initial.value = 0.;
    setup.target = MotionTrackTarget::PartOpacity { part_id: part };
    scene.tracks.push(setup);
    let original_scene = scene.clone();
    let model = encode_cmo3(
        &source,
        &|_: &kasane_core::ImageAsset| Ok(png.clone()),
        &Default::default(),
    )
    .unwrap();
    assert!(source.create_motion(scene).status.is_ok());
    let bytes = encode_can3(&source, &Default::default()).unwrap().bytes;
    // Use a separately decoded model, without source animations.
    let mut imported = import_model(&model.bytes).unwrap().document;
    import_animation(&bytes, &mut imported).unwrap();
    let mut expression = imported.get_expression(&uuid(100)).unwrap().clone();
    assert_eq!(expression.entries[0].value, 0.7 - 0.5);
    assert_eq!(expression.entries[0].blend, Some(ExpressionBlend::Add));
    assert_eq!(
        expression.source_scene().unwrap().tracks[1].target,
        original_scene.tracks[1].target
    );
    expression.entries[0].value = -0.25;
    // An absent Blend also means Add, including when encoding a CAN3 scene.
    expression.entries[0].blend = None;
    expression.name = "Edited expression".into();
    assert!(imported.replace_expression(expression).status.is_ok());
    let pair = encode_cubism(
        &imported,
        &|_: &kasane_core::ImageAsset| Ok(png.clone()),
        &Default::default(),
    )
    .unwrap();
    let mut reopened = import_model(&pair.cmo3.bytes).unwrap().document;
    import_animation(&pair.can3.bytes, &mut reopened).unwrap();
    assert!(reopened.motion_order().is_empty());
    let expression = reopened.get_expression(&uuid(100)).unwrap();
    assert_eq!(expression.name, "Edited expression");
    assert_eq!(expression.entries[0].value, -0.25);
    assert_eq!(expression.entries[0].blend, None);
    let scene = expression.source_scene().unwrap();
    assert_eq!(scene.duration, 8. / 30.);
    assert_eq!(scene.tracks[0].initial.value, 0.25);
    assert_eq!(scene.tracks[1].initial.value, 0.);
    let mut multiply = expression.clone();
    multiply.entries[0].blend = Some(ExpressionBlend::Multiply);
    assert!(reopened.replace_expression(multiply).status.is_ok());
    let report = inspect_cubism(&reopened).unwrap();
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_EXPRESSION_SEMANTICS"));
    assert!(!report.has_loss());
    assert!(encode_cubism(
        &reopened,
        &|_: &kasane_core::ImageAsset| Ok(png.clone()),
        &Default::default()
    )
    .is_ok());
}

#[test]
fn exported_model_import_preserves_parameter_bound_geometry_and_texture_pixels() {
    let (mut document, png) = fixture();
    let part_id = uuid(1);
    assert!(document
        .create_part(kasane_core::Part {
            id: part_id.clone(),
            runtime_id: "PartArm".into(),
            name: "手臂 Arm".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut settings = document.model3_settings().clone();
    settings.groups = Some(vec![kasane_core::document::ModelParameterGroup {
        name: "EyeBlink".into(),
        parameters: vec![kasane_core::document::ModelTargetRef::Resolved {
            object_id: PARAM.into(),
        }],
        extensions: Default::default(),
    }]);
    assert!(document
        .set_model3_settings(settings.clone())
        .status
        .is_ok());
    let artifact = encode_cmo3(
        &document,
        &|_: &kasane_core::ImageAsset| Ok(png.clone()),
        &Default::default(),
    )
    .unwrap();
    let imported = import_model(&artifact.bytes).unwrap();
    assert_eq!(imported.document.mesh_order().len(), 1);
    assert_eq!(imported.document.parameter_order().len(), 1);
    assert_eq!(imported.document.model3_settings().groups, settings.groups);
    assert_eq!(
        imported.document.get_part(&part_id).unwrap().name,
        "手臂 Arm"
    );
    assert_eq!(
        imported.document.display_info().parts,
        Some(vec![CdiPartEntry::Resolved {
            part_id,
            extensions: Default::default(),
        }])
    );
    assert_eq!(imported.images.values().next().unwrap(), &png);
    for value in [-1., -0.4, 0., 0.7, 1.] {
        let preview: PreviewValues = std::collections::HashMap::from([(PARAM.into(), value)]);
        let mut before = DrawableFrame::default();
        let mut after = DrawableFrame::default();
        assert!(evaluate_frame_including_hidden(&document, &preview, &mut before).is_ok());
        assert!(evaluate_frame_including_hidden(&imported.document, &preview, &mut after).is_ok());
        let a = &before.drawables[0];
        let b = &after.drawables[0];
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.uvs, b.uvs);
        assert_eq!(a.opacity, b.opacity);
        assert_eq!(a.indices, b.indices);
    }
}
#[test]
fn malformed_or_corrupt_archives_return_errors() {
    for bytes in [&[][..], b"CAFF", &[0; 60][..]] {
        assert!(import_model(bytes).is_err());
    }
    let (document, png) = fixture();
    let mut artifact = encode_cmo3(
        &document,
        &|_: &kasane_core::ImageAsset| Ok(png.clone()),
        &Default::default(),
    )
    .unwrap();
    artifact.bytes.truncate(artifact.bytes.len() - 10);
    assert!(import_model(&artifact.bytes).is_err());
}

#[test]
fn opacity_curve_roundtrip_preserves_initial_keys_and_bezier_controls() {
    let (mut source, _) = fixture();
    let mut clip = initial_scene(100, 60.);
    let track = &mut clip.tracks[0];
    track.target = MotionTrackTarget::Model {
        runtime_id: "Opacity".into(),
    };
    track.initial.value = 1.;
    track.segments = vec![
        MotionSegment::Bezier {
            control1: MotionPoint {
                time: 0.25,
                value: 0.9,
            },
            control2: MotionPoint {
                time: 0.75,
                value: 0.4,
            },
            end: MotionKey {
                id: uuid(103),
                time: 1.,
                value: 0.5,
            },
        },
        MotionSegment::Linear {
            end: MotionKey {
                id: uuid(104),
                time: 2.,
                value: 0.,
            },
        },
    ]
    .into();
    assert!(source.create_motion(clip.clone()).status.is_ok());
    for _ in 0..2 {
        let bytes = encode_can3(&source, &Default::default()).unwrap().bytes;
        let (mut imported, _) = fixture();
        import_animation(&bytes, &mut imported).unwrap();
        let actual = &imported.get_motion(&clip.id).unwrap().tracks[0];
        let expected = &clip.tracks[0];
        assert_eq!(actual.initial.point(), expected.initial.point());
        for (a, b) in actual.segments.iter().zip(expected.segments.iter()) {
            assert_eq!(a.end().point(), b.end().point());
            if let MotionSegment::Bezier {
                control1, control2, ..
            } = a
            {
                let MotionSegment::Bezier {
                    control1: c1,
                    control2: c2,
                    ..
                } = b
                else {
                    panic!()
                };
                assert_eq!((control1, control2), (c1, c2));
            }
        }
        source = imported;
    }
}

#[test]
fn morph_roundtrip_uses_interpolated_default_pose_and_appearance() {
    use kasane_core::types::*;
    let (mut source, png) = fixture();
    let mut parameter = source.get_parameter(PARAM).unwrap().clone();
    parameter.default_value = 0.5;
    assert!(source.replace_parameter(parameter).status.is_ok());
    assert!(source
        .create_parameter(Parameter {
            id: uuid(9),
            runtime_id: "GridY".into(),
            name: "Grid Y".into(),
            default_value: 0.25,
            ..Default::default()
        })
        .status
        .is_ok());
    let mut binding = source.binding_for_mesh(MESH).unwrap().clone();
    binding.axes.push(BindingAxis {
        parameter_id: uuid(9),
        keys: vec![-1., 1.],
    });
    binding.keyforms = [-1., 1.]
        .into_iter()
        .flat_map(|y| {
            binding.keyforms.iter().cloned().map(move |mut form| {
                form.keys.push(y);
                for p in &mut form.positions {
                    p.y += y * 8.;
                }
                form.appearance.multiply = [0.8, 0.8 - y * 0.1, 1.];
                form.appearance.screen = [0.1 + y * 0.1, 0., 0.];
                form
            })
        })
        .collect();
    assert!(source.replace_binding(binding).status.is_ok());
    assert!(source
        .create_parameter(Parameter {
            id: uuid(10),
            runtime_id: "Blend".into(),
            name: "Blend".into(),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(source
        .create_blend_key_table(BlendShapeKeyTable {
            id: uuid(11),
            parameter_id: uuid(10),
            keys: vec![0., 1.],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    assert!(source
        .create_blend_binding(BlendShapeBinding {
            id: uuid(12),
            target_id: MESH.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: uuid(11),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::default(); 3],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(10., 0.); 3],
                    opacity: Some(-0.1),
                    multiply: Some([-0.2, 0., 0.]),
                    screen: Some([0., 0.1, 0.]),
                    ..Default::default()
                },
            ]),
        })
        .status
        .is_ok());
    let mut current = source.clone();
    for _ in 0..2 {
        let bytes = encode_cmo3(
            &current,
            &|_: &ImageAsset| Ok(png.clone()),
            &Default::default(),
        )
        .unwrap()
        .bytes;
        current = import_model(&bytes).unwrap().document;
        for normal in [-0.5, 0.5, 0.75] {
            for blend in [0., 0.5, 1.] {
                let values = PreviewValues::from([(PARAM.into(), normal), (uuid(10), blend)]);
                let mut before = DrawableFrame::default();
                let mut after = DrawableFrame::default();
                assert!(evaluate_frame_including_hidden(&source, &values, &mut before).is_ok());
                assert!(evaluate_frame_including_hidden(&current, &values, &mut after).is_ok());
                let (a, b) = (&before.drawables[0], &after.drawables[0]);
                assert_eq!(a.positions, b.positions);
                assert!((a.opacity - b.opacity).abs() < 1e-6);
                for (a, b) in a
                    .multiply_color
                    .iter()
                    .zip(b.multiply_color)
                    .chain(a.screen_color.iter().zip(b.screen_color))
                {
                    assert!((a - b).abs() < 1e-6);
                }
            }
        }
    }
}

#[test]
fn expression_source_scene_targets_block_deletion_until_detached() {
    let (mut source, _) = fixture();
    let parameter = uuid(1);
    let part = uuid(2);
    assert!(source
        .create_parameter(kasane_core::Parameter {
            id: parameter.clone(),
            runtime_id: "Unbound".into(),
            name: "Unbound".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(source
        .create_part(kasane_core::Part {
            id: part.clone(),
            runtime_id: "Part".into(),
            name: "Part".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut scene = initial_scene(100, 8.);
    scene.tracks[0].target = MotionTrackTarget::Parameter {
        parameter_id: parameter.clone(),
    };
    let mut setup = scene.tracks[0].clone();
    setup.id = uuid(110);
    setup.initial.id = uuid(111);
    setup.target = MotionTrackTarget::PartOpacity {
        part_id: part.clone(),
    };
    scene.tracks.push(setup);
    let mut imported = source.clone();
    assert!(source.create_motion(scene).status.is_ok());
    import_animation(
        &encode_can3(&source, &Default::default()).unwrap().bytes,
        &mut imported,
    )
    .unwrap();
    let mut expression = imported.get_expression(&uuid(100)).unwrap().clone();
    // The source scene still refers to this parameter after its runtime entry is removed.
    expression.entries.clear();
    assert!(imported
        .replace_expression(expression.clone())
        .status
        .is_ok());
    for target in [&parameter, &part] {
        assert!(!imported.references_to(target).is_empty());
        assert_eq!(
            imported.erase_object(target).status.code,
            "OBJECT_REFERENCED"
        );
        assert!(imported.validate_structure().is_empty());
    }
    expression.extensions.remove("KasaneSourceScene");
    assert!(imported.replace_expression(expression).status.is_ok());
    for target in [&parameter, &part] {
        assert!(imported.references_to(target).is_empty());
        assert!(imported.erase_object(target).status.is_ok());
    }
    assert!(imported.validate_structure().is_empty());
}
