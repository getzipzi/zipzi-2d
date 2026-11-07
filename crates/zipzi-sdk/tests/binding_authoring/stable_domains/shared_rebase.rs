use super::*;
use zipzi_core::BindingAxisEditing;
use zipzi_core::WarpBezierEvaluation;

#[test]
fn shared_table_rebase_preserves_every_target_family_and_offscreen_neutral_slots() {
    let mut s = shared();
    let table = id(1701);
    let part = id(1800);
    let warp = id(1801);
    let rotation = id(1802);
    let glue = id(1803);
    let offscreen = id(1804);
    let lattice = vec![
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(0., 1.),
        Vec2::new(1., 1.),
    ];
    let bezier = WarpBezier::from_lattice(1, 1, true, &lattice);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: part.clone(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: warp.clone(),
                        data: TransformData::Warp(WarpTransform {
                            rows: 1,
                            columns: 1,
                            quad: true,
                            points: lattice.clone(),
                            bezier: Some(bezier.clone()),
                        }),
                        ..Default::default()
                    },
                )?;
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: rotation.clone(),
                        ..Default::default()
                    },
                )?;
                crate::support::insert_glue(
                    e,
                    Glue {
                        id: glue.clone(),
                        runtime_id: "glue".into(),
                        name: "glue".into(),
                        mesh_a_id: MESH_ID.into(),
                        mesh_b_id: MESH_ID.into(),
                        pairs: vec![],
                        intensity: 0.5,
                        binding: None,
                    },
                )?;
                e.create_scene_binding(SceneBinding {
                    id: id(1805),
                    axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
                    track: SceneTrack::Part {
                        target_id: part.clone().into(),
                        keyforms: vec![
                            PartKeyform {
                                keys: vec![0.],
                                draw_order: 0.,
                                ..Default::default()
                            },
                            PartKeyform {
                                keys: vec![1.],
                                draw_order: 2.,
                                ..Default::default()
                            },
                        ],
                    },
                })?;
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
                        id: offscreen.clone(),
                        runtime_id: "offscreen".into(),
                        part_id: part.clone(),
                        part_keyform_indices: vec![-1, 0],
                        keyforms: vec![OffscreenKeyform {
                            opacity: 0.5,
                            ..Default::default()
                        }],
                        ..Default::default()
                    },
                )?;
                e.create_scene_binding(SceneBinding {
                    id: id(1806),
                    axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
                    track: SceneTrack::Warp {
                        target_id: warp.clone().into(),
                        keyforms: [0., 1.]
                            .into_iter()
                            .map(|v| WarpKeyform {
                                keys: vec![v],
                                positions: lattice
                                    .iter()
                                    .map(|p| Vec2::new(p.x + v * 0.1, p.y))
                                    .collect(),
                                bezier: Some(bezier.clone()),
                                ..Default::default()
                            })
                            .collect(),
                    },
                })?;
                e.create_scene_binding(SceneBinding {
                    id: id(1807),
                    axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
                    track: SceneTrack::Rotation {
                        target_id: rotation.clone().into(),
                        keyforms: [0., 1.]
                            .into_iter()
                            .map(|v| RotationKeyform {
                                keys: vec![v],
                                rotation: RotationPose {
                                    angle: v * 5.,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .collect(),
                    },
                })?;
                for (n, target, kind) in [
                    (1810, warp.clone(), BlendShapeTargetKind::Warp),
                    (1811, rotation.clone(), BlendShapeTargetKind::Rotation),
                    (1812, part.clone(), BlendShapeTargetKind::Part),
                    (1813, glue.clone(), BlendShapeTargetKind::Glue),
                    (1814, offscreen.clone(), BlendShapeTargetKind::Offscreen),
                ] {
                    let keys = [-1., 0., 1.];
                    let keyforms = match kind {
                        BlendShapeTargetKind::Warp => DeltaKeyforms::Warp(
                            keys.into_iter()
                                .map(|v| DeltaWarpKeyform {
                                    points: vec![Vec2::new(v * 0.2, v * 0.1); 4],
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                        BlendShapeTargetKind::Rotation => DeltaKeyforms::Rotation(
                            keys.into_iter()
                                .map(|v| DeltaRotationKeyform {
                                    origin: Some(Vec2::new(v * 0.1, v * 0.2)),
                                    angle: Some(v * 10.),
                                    scale: Some(v * 0.1),
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                        BlendShapeTargetKind::Part => DeltaKeyforms::Part(
                            keys.into_iter()
                                .map(|v| DeltaPartKeyform {
                                    draw_order: v * 4.,
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                        BlendShapeTargetKind::Glue => DeltaKeyforms::Glue(
                            keys.into_iter()
                                .map(|v| DeltaGlueKeyform {
                                    intensity: v * 0.1,
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                        BlendShapeTargetKind::Offscreen => DeltaKeyforms::Offscreen(
                            keys.into_iter()
                                .map(|v| DeltaOffscreenKeyform {
                                    opacity: v * 0.1,
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                        _ => unreachable!(),
                    };
                    {
                        let binding = BlendShapeBinding {
                            id: id(n),
                            target_id: target,
                            target_kind: kind,
                            key_table_id: table.clone(),
                            constraint_ids: vec![],
                            keyforms,
                        };
                        e.edit_blend_shape(
                            zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                                id: binding.id.clone(),
                                target_id: binding.target_id.clone(),
                                target_kind: binding.target_kind,
                                key_table_id: binding.key_table_id.clone(),
                            },
                            Default::default(),
                            &Default::default(),
                        )?;
                        e.replace_blend_binding(binding)
                    }?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("targets")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before: Vec<_> = (0..=20)
        .map(|i| frame(&s, 0.37, i as f32 / 10. - 1.))
        .collect();
    let base = s
        .snapshot()
        .document()
        .blend_key_table(&table)
        .unwrap()
        .samples[2]
        .id
        .clone();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetBase { sample_id: base },
        }),
    );
    for (i, expected) in before.iter().enumerate() {
        let actual = frame(&s, 0.37, i as f32 / 10. - 1.);
        for (a, b) in actual.drawables.iter().zip(&expected.drawables) {
            for (a, b) in a.positions.iter().zip(&b.positions) {
                assert!((a.x - b.x).hypot(a.y - b.y) < 3e-5);
            }
        }
        for (a, b) in actual.deformers.iter().zip(&expected.deformers) {
            for (a, b) in a.points.iter().zip(&b.points) {
                assert!((a.x - b.x).hypot(a.y - b.y) < 3e-5);
            }
        }
        for (a, b) in actual.offscreens.iter().zip(&expected.offscreens) {
            assert!((a.opacity - b.opacity).abs() < 1e-6);
        }
    }
    assert!((s.snapshot().document().glue(&glue).unwrap().intensity - 0.6).abs() < 1e-6);
    assert_eq!(
        s.snapshot()
            .document()
            .blend_key_table(&table)
            .unwrap()
            .base_key_idx,
        2
    );
    roundtrip(&s);
}

#[test]
fn shared_mesh_rebase_keeps_colors_opacity_and_draw_order_and_honors_child_locks() {
    let mut s = shared();
    for n in [1702, 1703] {
        let mut binding = s
            .snapshot()
            .document()
            .blend_binding(&id(n))
            .unwrap()
            .clone();
        if let DeltaKeyforms::Mesh(forms) = &mut binding.keyforms {
            for (f, v) in forms.iter_mut().zip([-4., 0., 8.]) {
                f.opacity = Some(-v * 0.01);
                f.draw_order = Some(v);
                f.multiply = Some([-v * 0.005; 3]);
                f.screen = Some([v * 0.002; 3]);
            }
        }
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_blend_binding(binding))
                .and_then(|value| {
                    let prepared = draft.prepare("appearance deltas")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
    }
    let table = s
        .snapshot()
        .document()
        .blend_key_table(&id(1701))
        .unwrap()
        .clone();
    let intent = E::Blend(B::Table {
        id: table.id.clone(),
        operation: BO::SetBase {
            sample_id: table.samples[2].id.clone(),
        },
    });
    for child in [
        &table.samples[2].id,
        &s.snapshot()
            .document()
            .binding(BINDING_ID)
            .cloned()
            .unwrap()
            .axes[0]
            .samples[0]
            .id,
    ] {
        let mut flags = s.snapshot().document().editor_state().clone();
        flags.objects.insert(
            child.clone(),
            zipzi_core::document::ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_editor_state(flags))
                .and_then(|value| {
                    let prepared = draft.prepare("lock child")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let version = s.version();
        assert_eq!(
            crate::support::binding(&s, intent.clone())
                .unwrap_err()
                .code
                .as_str(),
            "OBJECT_LOCKED"
        );
        assert_eq!(s.version(), version);
        s.undo(s.version()).unwrap();
    }
    let expected: Vec<_> = (0..=16)
        .map(|i| frame(&s, 0.37, i as f32 / 8. - 1.))
        .collect();
    let receipt = apply(&mut s, intent);
    assert!(receipt.writes.records().iter().any(|r| r.id == table.id));
    for (i, expected) in expected.iter().enumerate() {
        let actual = frame(&s, 0.37, i as f32 / 8. - 1.);
        let (a, b) = (&actual.drawables[0], &expected.drawables[0]);
        assert_eq!(a.draw_order, b.draw_order);
        assert!((a.opacity - b.opacity).abs() < 1e-6);
        for (a, b) in a
            .multiply_color
            .iter()
            .chain(&a.screen_color)
            .zip(b.multiply_color.iter().chain(&b.screen_color))
        {
            assert!((a - b).abs() < 1e-6, "{a} != {b}");
        }
    }
    let inverse = s.undo(s.version()).unwrap();
    assert!(inverse.writes.records().iter().any(|r| r.id == table.id));
    assert_eq!(
        s.snapshot()
            .document()
            .blend_key_table(&table.id)
            .unwrap()
            .base_sample_id,
        table.base_sample_id
    );
}

#[test]
fn zero_offset_rebase_still_rejects_changed_optional_color_selection() {
    let mut s = shared();
    let mut b = s
        .snapshot()
        .document()
        .blend_binding(&id(1702))
        .unwrap()
        .clone();
    if let DeltaKeyforms::Mesh(f) = &mut b.keyforms {
        for f in f.iter_mut() {
            f.positions.fill(Vec2::default());
        }
        f[2].multiply = Some([0.1; 3]);
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_blend_binding(b))
            .and_then(|value| {
                let prepared = draft.prepare("optional channels")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let t = s.snapshot().document().blend_key_table(&id(1701)).unwrap();
    let intent = E::Blend(B::Table {
        id: t.id.clone(),
        operation: BO::SetBase {
            sample_id: t.samples[0].id.clone(),
        },
    });
    let before = s.version();
    let history = s.history_state();
    assert_eq!(
        crate::support::binding(&s, intent)
            .unwrap_err()
            .code
            .as_str(),
        "OPTIONAL_CHANNEL_NOT_REPRESENTABLE"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), history);
}
