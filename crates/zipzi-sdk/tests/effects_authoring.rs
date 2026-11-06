use zipzi_contract::authoring::ObjectKind;
use zipzi_core::{
    BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
    Canvas, DeltaKeyforms, DeltaMeshKeyform, Glue, GlueVertexPair, Offscreen, OffscreenKeyform,
    Parameter, ParameterKind, Part, PartKeyform, PreviewValues, SceneBinding, SceneTrack, Vec2,
};
use zipzi_sdk::{rectangle_mesh, AuthoringSession};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

#[test]
fn part_binding_and_offscreen_mapping_resize_together() {
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(101),
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_parameter({
                    let mut value = Parameter {
                        id: id(102),
                        minimum: 0.0,
                        maximum: 1.0,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                edit.create_part({
                    let mut value = Part {
                        id: id(103),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                edit.create_scene_binding(SceneBinding {
                    id: id(104),
                    axes: vec![BindingAxis {
                        id: Default::default(),
                        samples: Default::default(),
                        parameter_id: id(102),
                        keys: vec![0.0, 1.0],
                    }],
                    track: SceneTrack::Part {
                        target_id: id(103).into(),
                        keyforms: vec![
                            PartKeyform {
                                samples: Default::default(),
                                keys: vec![0.0],
                                draw_order: 0.0,
                            },
                            PartKeyform {
                                samples: Default::default(),
                                keys: vec![1.0],
                                draw_order: 10.0,
                            },
                        ],
                    },
                })?;
                crate::support::insert_offscreen(
                    edit,
                    Offscreen {
                        enabled: true,
                        id: id(105),
                        runtime_id: id(105),
                        name: String::new(),
                        part_id: id(103),
                        blend_mode: 0,
                        flags: 0,
                        masks: vec![],
                        part_keyform_indices: vec![0, 1],
                        keyforms: vec![
                            OffscreenKeyform {
                                opacity: 0.5,
                                ..Default::default()
                            },
                            OffscreenKeyform {
                                opacity: 1.0,
                                ..Default::default()
                            },
                        ],
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("base")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = sdk.version();
    let mut binding = sdk
        .snapshot()
        .document()
        .scene_binding(&id(104))
        .cloned()
        .unwrap();
    binding.axes[0].keys = vec![0.0, 0.5, 1.0];
    binding.axes[0].samples.clear(); // Numeric adapter preserves unchanged samples.
    binding.track = SceneTrack::Part {
        target_id: id(103).into(),
        keyforms: [0.0, 0.5, 1.0]
            .into_iter()
            .map(|key| PartKeyform {
                samples: Default::default(),
                keys: vec![key],
                draw_order: key * 10.0,
            })
            .collect(),
    };
    let mut offscreen = sdk
        .snapshot()
        .document()
        .offscreen(&id(105))
        .cloned()
        .unwrap();
    offscreen.part_keyform_indices = vec![0, -1, 1];
    let mut invalid = sdk.begin_draft();
    assert_eq!(
        invalid
            .batch(invalid.source(), |c| c
                .replace_scene_binding(binding.clone()))
            .unwrap_err()
            .code,
        "INVALID_LENGTH"
    );
    assert!(invalid.prepare("invalid independent resize").is_err());
    assert_eq!(sdk.version(), version);
    let (_, change) = crate::support::prepared(&sdk, |c| {
        c.replace_offscreen(offscreen)?;
        c.replace_scene_binding(binding)
    })
    .unwrap();
    sdk.publish(change).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .scene_binding(&id(104))
            .cloned()
            .unwrap()
            .track
            .len(),
        3
    );
    assert_eq!(
        sdk.snapshot()
            .document()
            .offscreen(&id(105))
            .cloned()
            .unwrap()
            .part_keyform_indices,
        [0, -1, 1]
    );
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .scene_binding(&id(104))
            .cloned()
            .unwrap()
            .track
            .len(),
        2
    );
}

#[test]
fn effect_families_share_one_publish_and_failed_replacement_preserves_it() {
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = crate::support::image_fixture(&id(2), "texture", &path).unwrap();
    let mesh_a = rectangle_mesh(
        &id(3),
        "a",
        &id(2),
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let mesh_b = rectangle_mesh(
        &id(4),
        "b",
        &id(2),
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let before = sdk.version();
    crate::support::prepared(&sdk, |edit| {
        crate::support::insert_image(edit, asset)?;
        edit.create_mesh(mesh_a)?;
        edit.create_mesh(mesh_b)?;
        edit.create_part({
            let mut value = Part {
                id: id(5),
                ..Default::default()
            };
            value.runtime_id = value.id.clone();
            value
        })?;
        edit.create_parameter({
            let mut value = Parameter {
                id: id(6),
                minimum: 0.0,
                maximum: 1.0,
                kind: ParameterKind::BlendShape,
                ..Default::default()
            };
            value.runtime_id = value.id.clone();
            value
        })?;
        edit.create_parameter({
            let mut value = Parameter {
                id: id(7),
                minimum: 0.0,
                maximum: 1.0,
                ..Default::default()
            };
            value.runtime_id = value.id.clone();
            value
        })?;
        edit.edit_blend_shape(
            zipzi_sdk::BlendAuthoringEdit::CreateTable {
                table: BlendShapeKeyTable {
                    samples: Default::default(),
                    base_sample_id: Default::default(),
                    id: id(8),
                    parameter_id: id(6),
                    keys: vec![0.0, 1.0],
                    base_key_idx: 0,
                },
            },
            Default::default(),
            &Default::default(),
        )
        .map(|_| ())?;
        edit.edit_blend_shape(
            zipzi_sdk::BlendAuthoringEdit::CreateConstraint {
                constraint: BlendShapeConstraint {
                    samples: Default::default(),
                    id: id(9),
                    parameter_id: id(7),
                    keys: vec![0.0, 1.0],
                    weights: vec![1.0, 1.0],
                },
            },
            Default::default(),
            &Default::default(),
        )
        .map(|_| ())?;
        crate::support::insert_glue(
            edit,
            Glue {
                id: id(10),
                runtime_id: id(10),
                name: "seam".into(),
                mesh_a_id: (id(3)).into(),
                mesh_b_id: (id(4)).into(),
                pairs: vec![GlueVertexPair {
                    vertex_a: Some(0),
                    vertex_b: Some(0),
                    weight_a: 1.0,
                    weight_b: 1.0,
                    ..Default::default()
                }],
                intensity: 0.0,
                binding: None,
            },
        )?;
        crate::support::insert_offscreen(
            edit,
            Offscreen {
                enabled: true,
                id: id(11),
                runtime_id: id(11),
                name: "group".into(),
                part_id: id(5),
                blend_mode: 0,
                flags: 0,
                masks: vec![],
                part_keyform_indices: vec![],
                keyforms: vec![OffscreenKeyform {
                    opacity: 1.0,
                    ..Default::default()
                }],
            },
        )?;
        {
            let binding = BlendShapeBinding {
                id: id(12),
                target_id: id(3),
                target_kind: BlendShapeTargetKind::Mesh,
                key_table_id: id(8),
                constraint_ids: vec![id(9)],
                keyforms: DeltaKeyforms::Mesh(vec![
                    DeltaMeshKeyform {
                        positions: vec![Vec2::default(); 4],
                        ..Default::default()
                    },
                    DeltaMeshKeyform {
                        positions: vec![Vec2::new(1.0, 0.0); 4],
                        ..Default::default()
                    },
                ]),
            };
            edit.edit_blend_shape(
                zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                    id: binding.id.clone(),
                    target_id: binding.target_id.clone(),
                    target_kind: binding.target_kind,
                    key_table_id: binding.key_table_id.clone(),
                },
                Default::default(),
                &Default::default(),
            )?;
            edit.replace_blend_binding(binding)
        }
    })
    .and_then(|(_, change)| sdk.publish(change))
    .unwrap();
    assert_eq!(sdk.version().revision, before.revision + 1);
    assert_eq!(sdk.snapshot().document().blend_key_table_order(), &[id(8)]);
    assert_eq!(sdk.snapshot().document().blend_constraint_order(), &[id(9)]);
    assert_eq!(sdk.snapshot().document().blend_binding_order(), &[id(12)]);
    assert_eq!(sdk.snapshot().document().glue_order(), &[id(10)]);
    assert_eq!(sdk.snapshot().document().offscreen_order(), &[id(11)]);
    assert_eq!(
        sdk.snapshot()
            .document()
            .glue(&id(10))
            .cloned()
            .unwrap()
            .pairs
            .len(),
        1
    );
    assert_eq!(
        sdk.snapshot()
            .document()
            .offscreen(&id(11))
            .cloned()
            .unwrap()
            .part_id,
        id(5)
    );
    assert_eq!(
        sdk.snapshot()
            .document()
            .blend_key_table(&id(8))
            .cloned()
            .unwrap()
            .keys,
        [0.0, 1.0]
    );
    assert_eq!(
        sdk.snapshot()
            .document()
            .blend_constraint(&id(9))
            .cloned()
            .unwrap()
            .weights,
        [1.0, 1.0]
    );
    assert_eq!(
        sdk.snapshot()
            .document()
            .blend_binding(&id(12))
            .cloned()
            .unwrap()
            .target_id,
        id(3)
    );
    {
        let kind = ObjectKind::BlendBinding;
        let object_id = (&id(12)).to_owned();
        sdk.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, sdk.read_snapshot().source))
    }
    .unwrap();
    assert!(crate::support::frame(sdk.snapshot(), &PreviewValues::from([(id(6), 1.0)])).is_ok());

    let stable = sdk.version();
    let mut invalid = sdk.begin_draft();
    assert_eq!(
        invalid
            .batch(invalid.source(), |candidate_commands| candidate_commands
                .edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateConstraint {
                        constraint: BlendShapeConstraint {
                            samples: Default::default(),
                            id: id(13),
                            parameter_id: id(7),
                            keys: vec![0., 1.],
                            weights: vec![1.]
                        }
                    },
                    Default::default(),
                    &Default::default()
                ))
            .unwrap_err()
            .code,
        "INVALID_LENGTH"
    );
    assert!(invalid.prepare("invalid effect").is_err());
    assert_eq!(sdk.version(), stable);

    sdk.undo(sdk.version()).unwrap();
    assert!(sdk.snapshot().document().blend_binding_order().is_empty());
    assert!(sdk.snapshot().document().offscreen_order().is_empty());
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(sdk.snapshot().document().blend_binding_order(), &[id(12)]);
}
