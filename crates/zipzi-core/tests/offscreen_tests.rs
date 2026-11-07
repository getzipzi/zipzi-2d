mod support;

use std::collections::HashMap;
use support::{image, prepare};
use zipzi_core::state::{DocumentState, RecordDomain};
use zipzi_core::{PartKeyform, SceneTrack};

use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};
use zipzi_core::types::{
    BindingAxis, BlendShapeBinding, BlendShapeKeyTable, BlendShapeTargetKind, Canvas,
    DeltaKeyforms, DeltaOffscreenKeyform, Mesh, Offscreen, OffscreenKeyform, Parameter,
    ParameterKind, Part, SceneBinding, Vec2,
};

fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn create_base_doc() -> DocumentState {
    let base = DocumentState::new(
        id(1),
        Canvas::with_flag(640., 480., Vec2::new(320., 240.), 100., 1).into(),
    )
    .unwrap();
    let mut builder = base.builder();
    builder.create_asset(image(id(2), "Tex", 64, 64)).unwrap();

    let part = Part {
        id: id(3),
        runtime_id: "PartRoot".to_string(),
        name: "Root Part".to_string(),
        parent_id: String::new(),
        enabled: true,
        draw_order: 0.0,
    };
    builder.create_part(part).unwrap();

    let mesh = Mesh {
        id: id(4),
        runtime_id: "MeshA".to_string(),
        name: "Mesh A".to_string(),
        texture_asset_id: (id(2)).into(),
        part_id: id(3),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 10.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    };
    builder.create_mesh(mesh).unwrap();

    builder.freeze().unwrap().state
}

#[test]
fn hidden_offscreen_preserves_composition_and_restores_appearance() {
    let doc = create_base_doc();
    let mut offscreen = Offscreen {
        id: id(10),
        runtime_id: "Offscreen".into(),
        part_id: id(3),
        keyforms: vec![OffscreenKeyform {
            opacity: 0.6,
            multiply: None,
            screen: None,
        }],
        ..Default::default()
    };
    let doc = prepare(&doc, |b| b.create_offscreen(offscreen.clone()))
        .unwrap()
        .state;
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    let plan = frame.render_plan.clone();
    assert!(frame.offscreens[0].enabled);
    assert_eq!(frame.offscreens[0].opacity, 0.6);
    offscreen.enabled = false;
    let doc = prepare(&doc, |b| b.replace_offscreen(offscreen.clone()))
        .unwrap()
        .state;
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    assert!(!frame.offscreens[0].enabled);
    assert_eq!(frame.offscreens[0].opacity, 0.);
    assert_eq!(frame.render_plan, plan);
    assert!(doc.part(&id(3)).unwrap().enabled);
    offscreen.enabled = true;
    let doc = prepare(&doc, |b| b.replace_offscreen(offscreen))
        .unwrap()
        .state;
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    assert!(frame.offscreens[0].enabled);
    assert_eq!(frame.offscreens[0].opacity, 0.6);
}

#[test]
fn test_offscreen_crud_and_validation() {
    let doc = create_base_doc();

    let os = Offscreen {
        enabled: true,
        id: id(10),
        runtime_id: "Offscreen0".to_string(),
        name: "Offscreen 0".to_string(),
        part_id: id(3),
        blend_mode: 262, // Multiply with alpha 1
        flags: 4,
        masks: vec![id(4)],
        part_keyform_indices: vec![],
        keyforms: vec![OffscreenKeyform {
            opacity: 0.8,
            multiply: Some([1.0, 0.5, 0.5]),
            screen: Some([0.1, 0.1, 0.1]),
        }],
    };
    let doc = prepare(&doc, |b| b.create_offscreen(os)).unwrap().state;
    assert_eq!(doc.offscreen_order(), &[id(10)]);
    assert!(doc.offscreen(&id(10)).is_some());
    assert!(doc.offscreen_for_part(&id(3)).is_some());

    // Duplicate owner part rejection
    let os_dup_owner = Offscreen {
        id: id(11),
        runtime_id: "Offscreen1".to_string(),
        name: "Offscreen 1".to_string(),
        part_id: id(3), // same part
        ..Default::default()
    };
    assert_eq!(
        prepare(&doc, |b| b.create_offscreen(os_dup_owner))
            .unwrap_err()
            .code,
        "DUPLICATE_OWNER"
    );

    // Missing mask mesh rejection
    let os_bad_mask = Offscreen {
        id: id(12),
        runtime_id: "OffscreenBadMask".to_string(),
        name: "Bad Mask".to_string(),
        part_id: id(3),
        masks: vec![id(999)],
        ..Default::default()
    };
    assert!(prepare(&doc, |b| b.create_offscreen(os_bad_mask)).is_err());

    // Removing either referenced record leaves an invalid candidate.
    assert!(prepare(&doc, |b| b.remove_part(&id(3))).is_err());
    assert!(prepare(&doc, |b| b.remove_mesh(&id(4))).is_err());
    let removed = prepare(&doc, |b| b.remove_offscreen(&id(10)))
        .unwrap()
        .state;
    assert!(removed.offscreen_order().is_empty());
    assert!(removed.offscreen_for_part(&id(3)).is_none());
    assert!(doc.offscreen(&id(10)).is_some());
}

#[test]
fn test_offscreen_evaluation_and_blendshapes() {
    let mut builder = create_base_doc().builder();

    let param_fade = Parameter {
        id: id(20),
        runtime_id: "ParamFade".to_string(),
        name: "Fade".to_string(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::Normal,
        repeat: false,
    };
    builder.create_parameter(param_fade).unwrap();

    let param_bs = Parameter {
        id: id(21),
        runtime_id: "ParamBS".to_string(),
        name: "BS".to_string(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        decimal_places: 2,
        kind: ParameterKind::BlendShape,
        repeat: false,
    };
    builder.create_parameter(param_bs).unwrap();

    // SceneBinding for Part id(3) with 2 keyforms driven by ParamFade
    let sb = SceneBinding {
        id: id(30),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: id(20),
            keys: vec![0.0, 1.0],
        }],
        track: SceneTrack::Part {
            target_id: (id(3)).into(),
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
    };
    builder.create_scene_binding(sb).unwrap();

    // Offscreen with 2 keyforms mapped via part_keyform_indices: [0, 1]
    let os = Offscreen {
        enabled: true,
        id: id(40),
        runtime_id: "Offscreen0".to_string(),
        name: "Offscreen 0".to_string(),
        part_id: id(3),
        blend_mode: 0,
        flags: 4,
        masks: vec![],
        part_keyform_indices: vec![0, 1],
        keyforms: vec![
            OffscreenKeyform {
                opacity: 0.2,
                multiply: Some([1.0, 1.0, 1.0]),
                screen: Some([0.0, 0.0, 0.0]),
            },
            OffscreenKeyform {
                opacity: 0.8,
                multiply: Some([0.5, 0.5, 0.5]),
                screen: Some([0.2, 0.2, 0.2]),
            },
        ],
    };
    builder.create_offscreen(os).unwrap();

    // BlendShapeBinding on Offscreen id(40) adding +0.4 opacity
    let bkt = BlendShapeKeyTable {
        samples: Default::default(),
        base_sample_id: Default::default(),
        id: id(50),
        parameter_id: id(21),
        keys: vec![0.0, 1.0],
        base_key_idx: 0,
    };
    builder.create_blend_key_table(bkt).unwrap();

    let bb = BlendShapeBinding {
        id: id(51),
        target_id: id(40),
        target_kind: BlendShapeTargetKind::Offscreen,
        key_table_id: id(50),
        constraint_ids: vec![],
        keyforms: DeltaKeyforms::Offscreen(vec![
            DeltaOffscreenKeyform {
                opacity: 0.0,
                ..Default::default()
            },
            DeltaOffscreenKeyform {
                sample_id: Default::default(),
                opacity: 0.4,
                multiply: Some([0.5, 0.5, 0.5]),
                screen: Some([0.1, 0.1, 0.1]),
            },
        ]),
    };
    builder.create_blend_binding(bb).unwrap();

    let doc = builder.freeze().unwrap().state;
    let mut preview = HashMap::new();
    let mut frame = DrawableFrame::default();

    // 1. ParamFade = 0.0, ParamBS = 0.0 => opacity = 0.2
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    assert_eq!(frame.offscreens.len(), 1);
    let os_f = &frame.offscreens[0];
    assert!((os_f.opacity - 0.2).abs() < 1e-5);
    assert!(os_f.enabled);

    // 2. ParamFade = 1.0, ParamBS = 0.0 => opacity = 0.8
    preview.insert(id(20), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let os_f = &frame.offscreens[0];
    assert!((os_f.opacity - 0.8).abs() < 1e-5);

    // 3. ParamFade = 0.5, ParamBS = 0.0 => opacity = 0.5
    preview.insert(id(20), 0.5);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let os_f = &frame.offscreens[0];
    assert!((os_f.opacity - 0.5).abs() < 1e-5);

    // 4. ParamFade = 0.5, ParamBS = 1.0 => base 0.5 + delta 0.4 = 0.9
    preview.insert(id(21), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let os_f = &frame.offscreens[0];
    assert!((os_f.opacity - 0.9).abs() < 1e-5);

    // 5. ParamFade = 1.0, ParamBS = 1.0 => base 0.8 + delta 0.4 = 1.2 => clamped to 1.0
    preview.insert(id(20), 1.0);
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let os_f = &frame.offscreens[0];
    assert!((os_f.opacity - 1.0).abs() < 1e-5);

    // 6. Disable Part => opacity becomes 0.0, enabled becomes false
    let mut p = doc.part(&id(3)).unwrap().clone();
    p.enabled = false;
    let doc = prepare(&doc, |b| b.replace_part(p)).unwrap().state;
    assert!(evaluate_frame(&doc, &preview, &mut frame).is_ok());
    let os_f = &frame.offscreens[0];
    assert_eq!(os_f.opacity, 0.0);
    assert!(!os_f.enabled);
}

#[test]
fn test_offscreen_render_orders_and_hierarchical_render_plan() {
    use zipzi_core::evaluation::RenderCommand;

    let mut builder = create_base_doc().builder();
    // doc has PartRoot(id(3)), MeshA(id(4))

    // Part 10 (Child of PartRoot), has Offscreen 100, Mesh 101
    let p10 = Part {
        id: id(10),
        runtime_id: "Part10".to_string(),
        name: "Part 10".to_string(),
        parent_id: id(3),
        enabled: true,
        draw_order: 1.0,
    };
    builder.create_part(p10).unwrap();

    let m101 = Mesh {
        id: id(101),
        runtime_id: "Mesh101".to_string(),
        name: "Mesh 101".to_string(),
        texture_asset_id: (id(2)).into(),
        part_id: id(10),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(0.0, 5.0),
        ],
        uvs: vec![Vec2::new(0.0, 0.0); 3],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    };
    builder.create_mesh(m101).unwrap();

    let os100 = Offscreen {
        id: id(100),
        runtime_id: "Offscreen100".to_string(),
        name: "Offscreen 100".to_string(),
        part_id: id(10),
        blend_mode: 0,
        flags: 0,
        ..Default::default()
    };
    builder.create_offscreen(os100).unwrap();

    // Part 20 (Child of Part 10 - nested!), has Offscreen 200, Mesh 201
    let p20 = Part {
        id: id(20),
        runtime_id: "Part20".to_string(),
        name: "Part 20".to_string(),
        parent_id: id(10),
        enabled: true,
        draw_order: 2.0,
    };
    builder.create_part(p20).unwrap();

    let m201 = Mesh {
        id: id(201),
        runtime_id: "Mesh201".to_string(),
        name: "Mesh 201".to_string(),
        texture_asset_id: (id(2)).into(),
        part_id: id(20),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(0.0, 5.0),
        ],
        uvs: vec![Vec2::new(0.0, 0.0); 3],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    };
    builder.create_mesh(m201).unwrap();

    let os200 = Offscreen {
        id: id(200),
        runtime_id: "Offscreen200".to_string(),
        name: "Offscreen 200".to_string(),
        part_id: id(20),
        blend_mode: 0,
        flags: 0,
        ..Default::default()
    };
    builder.create_offscreen(os200).unwrap();

    // Part 30 (Sibling of Part 10, child of PartRoot), has Offscreen 300, Mesh 301
    let p30 = Part {
        id: id(30),
        runtime_id: "Part30".to_string(),
        name: "Part 30".to_string(),
        parent_id: id(3),
        enabled: true,
        draw_order: 3.0,
    };
    builder.create_part(p30).unwrap();

    let m301 = Mesh {
        id: id(301),
        runtime_id: "Mesh301".to_string(),
        name: "Mesh 301".to_string(),
        texture_asset_id: (id(2)).into(),
        part_id: id(30),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.0),
            Vec2::new(0.0, 5.0),
        ],
        uvs: vec![Vec2::new(0.0, 0.0); 3],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    };
    builder.create_mesh(m301).unwrap();

    let os300 = Offscreen {
        id: id(300),
        runtime_id: "Offscreen300".to_string(),
        name: "Offscreen 300".to_string(),
        part_id: id(30),
        blend_mode: 0,
        flags: 0,
        ..Default::default()
    };
    builder.create_offscreen(os300).unwrap();

    let doc = builder.freeze().unwrap().state;
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());

    // Check offscreens
    assert_eq!(frame.offscreens.len(), 3);
    let os_map: HashMap<&str, &zipzi_core::evaluation::OffscreenFrame> = frame
        .offscreens
        .iter()
        .map(|os| (os.id.as_str(), os))
        .collect();

    assert_eq!(os_map[id(100).as_str()].parent_offscreen_id, None);
    assert_eq!(os_map[id(200).as_str()].parent_offscreen_id, Some(id(100)));
    assert_eq!(os_map[id(300).as_str()].parent_offscreen_id, None);

    // Verify render plan:
    // MeshA is in PartRoot -> DrawMesh(id(4))
    // Then Part 10 starts: BeginOffscreen(100), DrawMesh(101)
    // Then Part 20 starts: BeginOffscreen(200), DrawMesh(201), EndOffscreen(200)
    // Then Part 10 ends: EndOffscreen(100)
    // Then Part 30 starts: BeginOffscreen(300), DrawMesh(301), EndOffscreen(300)
    assert_eq!(
        frame.render_plan,
        vec![
            RenderCommand::DrawMesh { mesh_id: id(4) },
            RenderCommand::BeginOffscreen {
                offscreen_id: id(100)
            },
            RenderCommand::DrawMesh { mesh_id: id(101) },
            RenderCommand::BeginOffscreen {
                offscreen_id: id(200)
            },
            RenderCommand::DrawMesh { mesh_id: id(201) },
            RenderCommand::EndOffscreen {
                offscreen_id: id(200)
            },
            RenderCommand::EndOffscreen {
                offscreen_id: id(100)
            },
            RenderCommand::BeginOffscreen {
                offscreen_id: id(300)
            },
            RenderCommand::DrawMesh { mesh_id: id(301) },
            RenderCommand::EndOffscreen {
                offscreen_id: id(300)
            },
        ]
    );
}

#[test]
fn offscreen_indices_are_checked_without_a_part_binding() {
    let doc = create_base_doc();
    let os = Offscreen {
        id: id(10),
        runtime_id: "Offscreen".into(),
        part_id: id(3),
        part_keyform_indices: vec![2],
        keyforms: vec![OffscreenKeyform {
            opacity: 0.3,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        prepare(&doc, |b| b.create_offscreen(os)).unwrap_err().code,
        "INDEX_OUT_OF_BOUNDS"
    );
}

#[test]
fn static_offscreen_honors_explicit_mapping_and_sentinel() {
    let doc = create_base_doc();
    let mut os = Offscreen {
        id: id(10),
        runtime_id: "Offscreen".into(),
        part_id: id(3),
        part_keyform_indices: vec![1],
        keyforms: vec![
            OffscreenKeyform {
                opacity: 0.3,
                ..Default::default()
            },
            OffscreenKeyform {
                opacity: 0.7,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let doc = prepare(&doc, |b| b.create_offscreen(os.clone()))
        .unwrap()
        .state;
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    assert_eq!(frame.offscreens[0].opacity, 0.7);
    os.part_keyform_indices = vec![-1];
    let doc = prepare(&doc, |b| b.replace_offscreen(os)).unwrap().state;
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut frame).is_ok());
    assert_eq!(frame.offscreens[0].opacity, 1.0);
}

#[test]
fn part_binding_edits_preserve_offscreen_mapping_invariants() {
    let mut builder = create_base_doc().builder();
    builder
        .create_parameter(Parameter {
            id: id(20),
            runtime_id: "Param".into(),
            ..Default::default()
        })
        .unwrap();
    let binding = SceneBinding {
        id: id(30),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: id(20),
            keys: vec![-1.0, 1.0],
        }],
        track: SceneTrack::Part {
            target_id: (id(3)).into(),
            keyforms: vec![
                PartKeyform {
                    keys: vec![-1.0],
                    ..Default::default()
                },
                PartKeyform {
                    keys: vec![1.0],
                    ..Default::default()
                },
            ],
        },
    };
    builder.create_scene_binding(binding.clone()).unwrap();
    builder
        .create_offscreen(Offscreen {
            id: id(10),
            runtime_id: "Offscreen".into(),
            part_id: id(3),
            part_keyform_indices: vec![0, 0],
            keyforms: vec![OffscreenKeyform {
                opacity: 0.3,
                ..Default::default()
            }],
            ..Default::default()
        })
        .unwrap();
    let doc = builder.freeze().unwrap().state;
    let binding = doc.scene_binding(&id(30)).unwrap().clone();
    let mut shorter = binding.clone();
    shorter.axes[0].keys.pop();
    shorter.axes[0].samples.pop();
    shorter.track.part_keyforms_mut().unwrap().pop();
    assert_eq!(
        prepare(&doc, |b| b.replace_scene_binding(shorter))
            .unwrap_err()
            .code,
        "INVALID_LENGTH"
    );
    assert_eq!(doc.scene_binding(&id(30)), Some(&binding));
    assert!(prepare(&doc, |b| b.remove_scene_binding(&id(30))).is_err());
}

#[test]
fn joint_part_offscreen_edit_is_atomic_and_snapshot_safe() {
    let mut builder = create_base_doc().builder();
    builder
        .create_parameter(Parameter {
            id: id(20),
            runtime_id: "Param".into(),
            ..Default::default()
        })
        .unwrap();
    let mut binding = SceneBinding {
        id: id(30),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: id(20),
            keys: vec![-1.0, 1.0],
        }],
        track: SceneTrack::Part {
            target_id: (id(3)).into(),
            keyforms: vec![
                PartKeyform {
                    keys: vec![-1.0],
                    ..Default::default()
                },
                PartKeyform {
                    keys: vec![1.0],
                    ..Default::default()
                },
            ],
        },
    };
    builder.create_scene_binding(binding.clone()).unwrap();
    let mut os = Offscreen {
        id: id(10),
        runtime_id: "OS".into(),
        part_id: id(3),
        part_keyform_indices: vec![0, 1],
        keyforms: vec![
            OffscreenKeyform {
                opacity: 0.2,
                ..Default::default()
            },
            OffscreenKeyform {
                opacity: 0.8,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    builder.create_offscreen(os.clone()).unwrap();
    let doc = builder.freeze().unwrap().state;
    let before = doc;
    binding = before.scene_binding(&id(30)).unwrap().clone();
    binding.axes[0].keys.insert(1, 0.0);
    let sample_id = uuid::Uuid::new_v4().to_string();
    binding.axes[0].samples.insert(
        1,
        zipzi_core::BindingSample {
            id: sample_id.clone(),
            value: 0.,
        },
    );
    binding.track.part_keyforms_mut().unwrap().insert(
        1,
        PartKeyform {
            samples: vec![sample_id],
            keys: vec![0.0],
            ..Default::default()
        },
    );
    assert_eq!(
        prepare(&before, |b| b.replace_scene_binding(binding.clone()))
            .unwrap_err()
            .code,
        "INVALID_LENGTH"
    );
    os.keyforms.push(OffscreenKeyform {
        opacity: 0.6,
        ..Default::default()
    });
    os.part_keyform_indices = vec![0, 2, 1];
    let mut invalid = os.clone();
    invalid.keyforms[2].opacity = f32::NAN;
    assert_eq!(
        prepare(&before, |b| {
            b.replace_scene_binding(binding.clone())?;
            b.replace_offscreen(invalid)
        })
        .unwrap_err()
        .code,
        "INVALID_OPACITY"
    );
    assert_eq!(before.scene_binding(&id(30)).unwrap().track.len(), 2);
    assert_eq!(
        before.offscreen(&id(10)).unwrap().part_keyform_indices,
        [0, 1]
    );

    let prepared = prepare(&before, |b| {
        b.replace_scene_binding(binding.clone())?;
        b.replace_offscreen(os.clone())
    })
    .unwrap();
    assert_eq!(
        prepared
            .writes
            .records()
            .iter()
            .map(|write| write.domain)
            .collect::<Vec<_>>(),
        [RecordDomain::SceneBinding, RecordDomain::Offscreen]
    );
    let after = prepared.state;
    let mut frame = DrawableFrame::default();
    assert!(evaluate_frame(&after, &HashMap::from([(id(20), 0.0)]), &mut frame).is_ok());
    assert_eq!(frame.offscreens[0].opacity, 0.6);
    assert!(evaluate_frame(&before, &HashMap::from([(id(20), 0.0)]), &mut frame).is_ok());
    assert_eq!(frame.offscreens[0].opacity, 0.5);
    assert_eq!(after.scene_binding(&id(30)), Some(&binding));
    assert_eq!(after.offscreen(&id(10)), Some(&os));
}
