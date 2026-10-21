use super::*;
use kasane_core::*;
use kasane_sdk::{
    AxisReduction as R, BindingAuthoringEdit as E, BindingFieldPatch, BindingGridEdit as G,
    BindingGridTarget as T, BlendAuthoringEdit as B, BlendTableOperation as BO, GridOperation as O,
    ObjectKind,
};
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn apply(s: &mut AuthoringSession, e: E) -> kasane_sdk::EditReceipt {
    let p = s.plan_binding_authoring(e).unwrap();
    s.execute_binding_plan(&p).unwrap()
}
fn base() -> AuthoringSession {
    let mut s = fixture();
    let b = binding(&s);
    s.edit("parameters", None, |e| {
        e.create_parameter(parameter(false))?;
        e.create_parameter(Parameter {
            id: id(1600),
            ..Default::default()
        })?;
        e.create_binding(b)
    })
    .unwrap();
    s
}
fn op(s: &mut AuthoringSession, owner: &str, operation: O) -> kasane_sdk::EditReceipt {
    apply(
        s,
        E::Grid(G::Edit {
            owner: owner.into(),
            operation,
        }),
    )
}
fn roundtrip(s: &AuthoringSession) -> Document {
    let mut d = s.document().fork_candidate();
    let mut a = d.get_asset(ASSET_ID).unwrap().clone();
    a.source = "assets/texture.png".into();
    assert!(d.replace_asset(a).status.is_ok());
    let text = kasane_project::encode_project(&d).unwrap();
    let out = kasane_project::decode_project(&text).unwrap();
    assert!(
        out.validate_structure().is_empty(),
        "{:?}",
        out.validate_structure()
    );
    out
}
fn frame(s: &AuthoringSession, p: f32, b: f32) -> DrawableFrame {
    s.evaluate(
        &s.parameter_ids()
            .iter()
            .map(|parameter| {
                (
                    parameter.clone(),
                    if parameter == PARAMETER_ID {
                        p
                    } else if parameter == &id(1700) {
                        b
                    } else {
                        0.
                    },
                )
            })
            .collect(),
    )
    .unwrap()
}
#[test]
fn stable_axis_sample_ids_roundtrip_and_old_addresses_do_not_revive() {
    let mut s = base();
    let old = s.binding(BINDING_ID).unwrap();
    let axis = &old.axes[0];
    let sample = axis.samples[0].id.clone();
    let sample_handle = s.handle(ObjectKind::BindingSample, &sample).unwrap();
    let axis_handle = s.handle(ObjectKind::BindingAxis, &axis.id).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let b = s.binding(BINDING_ID).unwrap();
    let address = s.binding_field_address(&sample).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::ReorderAxes {
            axis_ids: vec![b.axes[1].id.clone(), b.axes[0].id.clone()],
        },
    );
    let p = s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.1))
        .unwrap();
    let receipt = s.execute_binding_plan(&p).unwrap();
    assert!(receipt
        .changes
        .modified
        .iter()
        .any(|e| e.kind == ObjectKind::BindingSample && e.id == sample));
    assert_eq!(
        s.document().get_binding(BINDING_ID).unwrap().axes[1].samples[0].id,
        sample
    );
    let loaded = roundtrip(&s);
    assert_eq!(
        loaded.get_binding(BINDING_ID),
        s.document().get_binding(BINDING_ID)
    );
    assert!(s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.2))
        .is_err());
    let address = s.binding_field_address(&sample).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::SetParameter {
            axis_id: axis.id.clone(),
            parameter_id: String::new(),
            reduction: Some(R::Slice(0.1)),
        },
    );
    s.undo().unwrap();
    s.resolve_handle(&sample_handle).unwrap();
    s.resolve_handle(&axis_handle).unwrap();
    assert_eq!(
        s.plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.2))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );
    let b = s.binding(BINDING_ID).unwrap();
    let axis = b.axes.iter().find(|a| a.id == axis.id).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::RemoveAxis {
            axis_id: axis.id.clone(),
            reduction: Some(R::Slice(0.1)),
            keep_pending: false,
        },
    );
    s.undo().unwrap();
    assert!(s.resolve_handle(&sample_handle).is_err());
    assert!(s.resolve_handle(&axis_handle).is_err());
    assert!(s.document().binding_identity(&sample).is_some());
    let _ = address;
}
#[test]
fn pending_axis_activation_reads_current_rotation_base_and_keeps_identity() {
    let mut s = base();
    let target = id(1601);
    let binding = id(1602);
    let pending = BindingAxis::new("", vec![0., 1.]);
    let axis = pending.id.clone();
    s.edit("rotation", None, |e| {
        e.create_transform(Transform {
            id: target.clone(),
            ..Default::default()
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Grid(G::Create {
            id: binding.clone(),
            target: T::Scene(target.clone()),
        }),
    );
    op(&mut s, &binding, O::AddAxis { axis: pending });
    assert!(s
        .document()
        .get_scene_binding(&binding)
        .unwrap()
        .track
        .is_empty());
    let mut t = s.document().get_transform(&target).unwrap().clone();
    t.rotation_mut().unwrap().pose.angle = 42.;
    s.edit("latest base", None, |e| e.replace_transform(t))
        .unwrap();
    op(
        &mut s,
        &binding,
        O::SetParameter {
            axis_id: axis.clone(),
            parameter_id: PARAMETER_ID.into(),
            reduction: None,
        },
    );
    let b = s.document().get_scene_binding(&binding).unwrap();
    assert_eq!(b.axes[0].id, axis);
    assert!(b.track.samples().all(|s| s.rotation.angle == 42.));
    let mut f = match &b.track {
        SceneTrack::Rotation { keyforms, .. } => keyforms[1].clone(),
        _ => panic!(),
    };
    f.rotation.angle = 62.;
    s.edit("pose", None, |e| {
        e.set_scene_keyform(&binding, SceneKeyform::Rotation(f))
    })
    .unwrap();
    op(
        &mut s,
        &binding,
        O::RemoveAxis {
            axis_id: axis.clone(),
            reduction: Some(R::Resample(0.25)),
            keep_pending: true,
        },
    );
    assert_eq!(
        s.document()
            .get_transform(&target)
            .unwrap()
            .rotation()
            .unwrap()
            .pose
            .angle,
        47.
    );
    assert!(s
        .document()
        .get_scene_binding(&binding)
        .unwrap()
        .track
        .is_empty());
    roundtrip(&s);
    op(
        &mut s,
        &binding,
        O::SetParameter {
            axis_id: axis.clone(),
            parameter_id: PARAMETER_ID.into(),
            reduction: None,
        },
    );
    assert!(s
        .document()
        .get_scene_binding(&binding)
        .unwrap()
        .track
        .samples()
        .all(|s| s.rotation.angle == 47.));
}
#[test]
fn rotation_reflection_resampling_rejects_atomically() {
    let mut s = base();
    let t = id(1610);
    let b = id(1611);
    s.edit("rotation", None, |e| {
        e.create_transform(Transform {
            id: t.clone(),
            ..Default::default()
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Grid(G::Create {
            id: b.clone(),
            target: T::Scene(t),
        }),
    );
    op(
        &mut s,
        &b,
        O::AddAxis {
            axis: BindingAxis::new(PARAMETER_ID, vec![0., 1.]),
        },
    );
    let binding = s.document().get_scene_binding(&b).unwrap();
    let axis = binding.axes[0].id.clone();
    let mut f = match &binding.track {
        SceneTrack::Rotation { keyforms, .. } => keyforms[1].clone(),
        _ => panic!(),
    };
    f.rotation.reflect_x = true;
    s.edit("reflect", None, |e| {
        e.set_scene_keyform(&b, SceneKeyform::Rotation(f))
    })
    .unwrap();
    let v = s.version();
    let h = s.history_state();
    let error = s
        .plan_binding_authoring(E::Grid(G::Edit {
            owner: b,
            operation: O::InsertSample {
                axis_id: axis,
                id: id(1612),
                value: 0.5,
            },
        }))
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "DISCRETE_ROTATION_NOT_REPRESENTABLE");
    assert_eq!(s.version(), v);
    assert_eq!(s.history_state(), h);
}
#[test]
fn warp_grid_add_insert_reduce_preserves_controllers_and_domain() {
    let mut s = base();
    let t = id(1620);
    let b = id(1621);
    let points = vec![
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(0., 1.),
        Vec2::new(1., 1.),
    ];
    let bezier = WarpBezier::from_lattice(1, 1, true, &points);
    s.edit("warp", None, |e| {
        e.create_transform(Transform {
            id: t.clone(),
            data: TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                points,
                bezier: Some(bezier),
            }),
            ..Default::default()
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Grid(G::Create {
            id: b.clone(),
            target: T::Scene(t.clone()),
        }),
    );
    op(
        &mut s,
        &b,
        O::AddAxis {
            axis: BindingAxis::new(PARAMETER_ID, vec![0., 1.]),
        },
    );
    let binding = s.document().get_scene_binding(&b).unwrap();
    let axis = binding.axes[0].id.clone();
    let mut f = match &binding.track {
        SceneTrack::Warp { keyforms, .. } => keyforms[1].clone(),
        _ => panic!(),
    };
    for p in &mut f.positions {
        p.x += 0.4;
    }
    for n in &mut f.bezier.as_mut().unwrap().nodes {
        n.position.x += 0.4;
        for p in &mut n.handles {
            p.x += 0.4;
        }
    }
    s.edit("warp pose", None, |e| {
        e.set_scene_keyform(&b, SceneKeyform::Warp(f))
    })
    .unwrap();
    let expected = frame(&s, 0.3, 0.)
        .deformers
        .into_iter()
        .find(|d| d.id == t)
        .unwrap()
        .points;
    op(
        &mut s,
        &b,
        O::InsertSample {
            axis_id: axis.clone(),
            id: id(1622),
            value: 0.3,
        },
    );
    let actual = frame(&s, 0.3, 0.)
        .deformers
        .into_iter()
        .find(|d| d.id == t)
        .unwrap()
        .points;
    for (a, b) in actual.iter().zip(&expected) {
        assert!((a.x - b.x).abs() < 1e-6);
    }
    op(
        &mut s,
        &b,
        O::RemoveAxis {
            axis_id: axis,
            reduction: Some(R::Slice(0.3)),
            keep_pending: false,
        },
    );
    assert_eq!(
        s.document()
            .get_transform(&t)
            .unwrap()
            .warp()
            .unwrap()
            .points,
        actual
    );
    assert!(s
        .document()
        .get_transform(&t)
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .is_some());
    roundtrip(&s);
}
#[test]
fn part_grid_remaps_offscreen_and_does_not_revive_old_static_appearance() {
    let mut s = base();
    let p = id(1630);
    let b = id(1631);
    let os = id(1632);
    s.edit("part/offscreen", None, |e| {
        e.create_part(Part {
            id: p.clone(),
            ..Default::default()
        })?;
        e.create_scene_binding(SceneBinding {
            id: b.clone(),
            axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
            track: SceneTrack::Part {
                target_id: p.clone().into(),
                keyforms: vec![
                    PartKeyform {
                        keys: vec![0.],
                        draw_order: 3.,
                        ..Default::default()
                    },
                    PartKeyform {
                        keys: vec![1.],
                        draw_order: 17.,
                        ..Default::default()
                    },
                ],
            },
        })?;
        e.create_offscreen(Offscreen {
            id: os.clone(),
            runtime_id: "offscreen".into(),
            part_id: p.clone(),
            part_keyform_indices: vec![1, 0],
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
        })
    })
    .unwrap();
    let primary = s.document().get_scene_binding(&b).unwrap().axes[0]
        .id
        .clone();
    op(
        &mut s,
        &b,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let secondary = s.document().get_scene_binding(&b).unwrap().axes[1]
        .id
        .clone();
    assert_eq!(
        s.document()
            .get_offscreen(&os)
            .unwrap()
            .part_keyform_indices,
        vec![1, 0, 1, 0]
    );
    op(
        &mut s,
        &b,
        O::ReorderAxes {
            axis_ids: vec![secondary.clone(), primary.clone()],
        },
    );
    assert_eq!(
        s.document()
            .get_offscreen(&os)
            .unwrap()
            .part_keyform_indices,
        vec![1, 1, 0, 0]
    );
    op(
        &mut s,
        &b,
        O::RemoveAxis {
            axis_id: primary,
            reduction: Some(R::Resample(0.25)),
            keep_pending: false,
        },
    );
    op(
        &mut s,
        &b,
        O::RemoveAxis {
            axis_id: secondary,
            reduction: Some(R::Slice(1.)),
            keep_pending: false,
        },
    );
    assert_eq!(s.document().get_part(&p).unwrap().draw_order, 6.5);
    let appearance = frame(&s, 0., 0.)
        .offscreens
        .into_iter()
        .find(|o| o.id == os)
        .unwrap();
    assert!((appearance.opacity - 0.65).abs() < 1e-6);
    let mut o = s.document().get_offscreen(&os).unwrap().clone();
    let slot = o.part_keyform_indices[0] as usize;
    o.keyforms[slot].opacity = 0.33;
    s.edit("static appearance", None, |e| e.replace_offscreen(o))
        .unwrap();
    op(
        &mut s,
        &b,
        O::AddAxis {
            axis: BindingAxis::new(PARAMETER_ID, vec![0., 1.]),
        },
    );
    for v in [0., 0.4, 1.] {
        assert!(
            (frame(&s, v, 0.)
                .offscreens
                .iter()
                .find(|o| o.id == os)
                .unwrap()
                .opacity
                - 0.33)
                .abs()
                < 1e-6
        );
    }
    roundtrip(&s);
}
#[test]
fn glue_zero_axis_grid_conversion_uses_stable_samples() {
    let mut s = base();
    let g = id(1640);
    s.edit("glue", None, |e| {
        e.create_glue(Glue {
            id: g.clone(),
            runtime_id: "glue".into(),
            name: "glue".into(),
            mesh_a_id: MESH_ID.into(),
            mesh_b_id: MESH_ID.into(),
            pairs: vec![],
            intensity: 0.6,
            binding: None,
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Grid(G::Create {
            id: g.clone(),
            target: T::Glue(g.clone()),
        }),
    );
    op(
        &mut s,
        &g,
        O::AddAxis {
            axis: BindingAxis::new(PARAMETER_ID, vec![0., 1.]),
        },
    );
    let a = s
        .document()
        .get_glue(&g)
        .unwrap()
        .binding
        .as_ref()
        .unwrap()
        .axes[0]
        .id
        .clone();
    let mut glue = s.document().get_glue(&g).unwrap().clone();
    glue.binding.as_mut().unwrap().keyforms[1].intensity = 0.9;
    s.edit("glue pose", None, |e| e.replace_glue(glue)).unwrap();
    op(
        &mut s,
        &g,
        O::InsertSample {
            axis_id: a.clone(),
            id: id(1641),
            value: 0.5,
        },
    );
    assert!(
        (s.document()
            .get_glue(&g)
            .unwrap()
            .binding
            .as_ref()
            .unwrap()
            .keyforms[1]
            .intensity
            - 0.75)
            .abs()
            < 1e-6
    );
    op(
        &mut s,
        &g,
        O::RemoveAxis {
            axis_id: a,
            reduction: Some(R::Slice(0.5)),
            keep_pending: false,
        },
    );
    assert!((s.document().get_glue(&g).unwrap().intensity - 0.75).abs() < 1e-6);
    roundtrip(&s);
    s.undo().unwrap();
    assert_eq!(
        s.document()
            .get_glue(&g)
            .unwrap()
            .binding
            .as_ref()
            .unwrap()
            .keyforms
            .len(),
        3
    );
}
fn shared() -> AuthoringSession {
    let mut s = base();
    let p = id(1700);
    let table = id(1701);
    s.edit("blend parameter", None, |e| {
        e.create_parameter(Parameter {
            id: p.clone(),
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: table.clone(),
                parameter_id: p,
                keys: vec![-1., 0., 1.],
                base_key_idx: 1,
                samples: vec![],
                base_sample_id: String::new(),
            },
        }),
    );
    for n in [1702, 1703] {
        let b = BlendShapeBinding {
            id: id(n),
            target_id: MESH_ID.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: table.clone(),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(
                [-4., 0., 8.]
                    .into_iter()
                    .map(|x| DeltaMeshKeyform {
                        positions: vec![Vec2::new(x, 0.); 4],
                        ..Default::default()
                    })
                    .collect(),
            ),
        };
        s.edit("shared user", None, |e| e.create_blend_binding(b))
            .unwrap();
    }
    s
}
#[test]
fn shared_blend_insert_move_rebase_updates_every_user_and_ordinary_grid() {
    let mut s = shared();
    // The base entry is authoring content even though evaluation ignores it.
    let mut authored = s.document().get_blend_binding(&id(1702)).unwrap().clone();
    if let DeltaKeyforms::Mesh(f) = &mut authored.keyforms {
        f[1].positions.fill(Vec2::new(91., 37.));
    }
    s.edit("authored base entry", None, |e| {
        e.replace_blend_binding(authored)
    })
    .unwrap();
    let original = s
        .document()
        .get_blend_binding(&id(1702))
        .unwrap()
        .keyforms
        .clone();
    let table = id(1701);
    let before: Vec<_> = (0..=16)
        .map(|i| {
            frame(&s, 0.37, i as f32 / 8. - 1.).drawables[0]
                .positions
                .clone()
        })
        .collect();
    let old = s.document().get_blend_key_table(&table).unwrap().clone();
    let sample = id(1704);
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: sample.clone(),
                    value: 0.5,
                },
            },
        }),
    );
    for n in [1702, 1703] {
        let DeltaKeyforms::Mesh(f) = &s.document().get_blend_binding(&id(n)).unwrap().keyforms
        else {
            panic!()
        };
        assert_eq!(f[2].sample_id, sample);
        assert_eq!(f[2].positions[0].x, 4.);
    }
    let DeltaKeyforms::Mesh(original_forms) = &original else {
        panic!()
    };
    let DeltaKeyforms::Mesh(after_insert) =
        &s.document().get_blend_binding(&id(1702)).unwrap().keyforms
    else {
        panic!()
    };
    for (before, after) in
        original_forms
            .iter()
            .zip([&after_insert[0], &after_insert[1], &after_insert[3]])
    {
        assert_eq!(before, after);
    }
    let base = old.samples[2].id.clone();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetBase {
                sample_id: base.clone(),
            },
        }),
    );
    assert_eq!(
        s.document()
            .get_blend_key_table(&table)
            .unwrap()
            .base_sample_id,
        base
    );
    assert_eq!(
        s.document().get_mesh(MESH_ID).unwrap().base_positions[0].x,
        56.
    );
    assert_eq!(
        s.binding(BINDING_ID).unwrap().keyforms[0].positions[0].x,
        56.
    );
    for (i, expected) in before.iter().enumerate() {
        for (a, b) in frame(&s, 0.37, i as f32 / 8. - 1.).drawables[0]
            .positions
            .iter()
            .zip(expected)
        {
            assert!((a.x - b.x).abs() < 2e-5);
        }
    }
    roundtrip(&s);
    s.undo().unwrap();
    s.redo().unwrap();
    let address = s.binding_field_address(&sample).unwrap();
    let p = s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.6))
        .unwrap();
    s.execute_binding_plan(&p).unwrap();
    let t = s.document().get_blend_key_table(&table).unwrap();
    assert_eq!(
        t.samples.iter().find(|k| k.id == sample).unwrap().value,
        0.6
    );
    let handle = s.handle(ObjectKind::BindingSample, &sample).unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Delete {
                sample_id: sample.clone(),
                replacement_base: None,
            },
        }),
    );
    s.undo().unwrap();
    assert!(s.document().binding_identity(&sample).is_some());
    assert!(s.resolve_handle(&handle).is_err());
}
#[test]
fn pending_blend_constraint_entries_pair_weights_by_id_and_export_explicitly() {
    let mut s = base();
    let table = id(1710);
    let b = id(1711);
    let c = id(1712);
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: table.clone(),
                parameter_id: String::new(),
                keys: vec![],
                base_key_idx: 0,
                samples: vec![],
                base_sample_id: String::new(),
            },
        }),
    );
    apply(
        &mut s,
        E::Blend(B::CreateBinding {
            id: b.clone(),
            target_id: MESH_ID.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: String::new(),
        }),
    );
    let original = frame(&s, 0.5, 0.).drawables[0].positions.clone();
    assert_eq!(frame(&s, 0.5, 0.).drawables[0].positions, original);
    roundtrip(&s);
    let plan = s.plan_available_binding_export();
    assert!(plan.omitted.iter().any(|o| o.object_id == b));
    let out = s
        .export_binding_plan(&plan, &kasane_sdk::Cmo3Options::default())
        .unwrap();
    assert!(out.report.diagnostics.iter().any(|d| d.object_id == b));
    apply(
        &mut s,
        E::Blend(B::CreateConstraint {
            constraint: BlendShapeConstraint {
                id: c.clone(),
                parameter_id: String::new(),
                keys: vec![],
                weights: vec![],
                samples: vec![],
            },
        }),
    );
    apply(
        &mut s,
        E::Blend(B::Constraint {
            id: c.clone(),
            parameter_id: PARAMETER_ID.into(),
            samples: vec![
                ConstraintSample {
                    id: id(1713),
                    value: 0.,
                    weight: 0.2,
                },
                ConstraintSample {
                    id: id(1714),
                    value: 1.,
                    weight: 0.9,
                },
            ],
        }),
    );
    let a = s.binding_field_address(&id(1713)).unwrap();
    let p = s
        .plan_binding_field_patch(
            &a,
            BindingFieldPatch::ConstraintSample {
                value: 0.4,
                weight: 0.3,
            },
        )
        .unwrap();
    s.execute_binding_plan(&p).unwrap();
    let c = s.document().get_blend_constraint(&c).unwrap();
    assert_eq!(c.samples[0].id, id(1713));
    assert_eq!(c.keys, vec![0.4, 1.]);
    assert_eq!(c.weights, vec![0.3, 0.9]);
    roundtrip(&s);
}
#[test]
fn shared_rebase_constraints_locks_limits_and_missing_base_reject_without_history() {
    let mut s = shared();
    let table = id(1701);
    let t = s.document().get_blend_key_table(&table).unwrap().clone();
    let before = s.version();
    let h = s.history_state();
    assert_eq!(
        s.plan_binding_authoring(E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Delete {
                sample_id: t.base_sample_id.clone(),
                replacement_base: None
            }
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "BASE_REPLACEMENT_REQUIRED"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
    let p = s
        .plan_binding_authoring(E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: id(1720),
                    value: 0.5,
                },
            },
        }))
        .unwrap();
    let mut limits = s.authoring_limits();
    limits.max_conversion_bytes = p.cost().temporary_bytes - 1;
    s.set_authoring_limits(limits).unwrap();
    assert!(s.execute_binding_plan(&p).is_err());
    assert_eq!(s.version(), before);
    s.set_authoring_limits(Default::default()).unwrap();
    let c = id(1721);
    apply(
        &mut s,
        E::Blend(B::CreateConstraint {
            constraint: BlendShapeConstraint {
                id: c.clone(),
                parameter_id: PARAMETER_ID.into(),
                keys: vec![0., 1.],
                weights: vec![0.5, 1.],
                samples: vec![],
            },
        }),
    );
    let mut b = s.document().get_blend_binding(&id(1702)).unwrap().clone();
    b.constraint_ids.push(c);
    s.edit("constrain", None, |e| e.replace_blend_binding(b))
        .unwrap();
    let before = s.version();
    let h = s.history_state();
    assert_eq!(
        s.plan_binding_authoring(E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetBase {
                sample_id: t.samples[2].id.clone()
            }
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "CONSTRAINED_REBASE_NOT_REPRESENTABLE"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
}

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
    s.edit("targets", None, |e| {
        e.create_part(Part {
            id: part.clone(),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: warp.clone(),
            data: TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                points: lattice.clone(),
                bezier: Some(bezier.clone()),
            }),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: rotation.clone(),
            ..Default::default()
        })?;
        e.create_glue(Glue {
            id: glue.clone(),
            runtime_id: "glue".into(),
            name: "glue".into(),
            mesh_a_id: MESH_ID.into(),
            mesh_b_id: MESH_ID.into(),
            pairs: vec![],
            intensity: 0.5,
            binding: None,
        })?;
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
        e.create_offscreen(Offscreen {
            id: offscreen.clone(),
            runtime_id: "offscreen".into(),
            part_id: part.clone(),
            part_keyform_indices: vec![-1, 0],
            keyforms: vec![OffscreenKeyform {
                opacity: 0.5,
                ..Default::default()
            }],
            ..Default::default()
        })?;
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
            e.create_blend_binding(BlendShapeBinding {
                id: id(n),
                target_id: target,
                target_kind: kind,
                key_table_id: table.clone(),
                constraint_ids: vec![],
                keyforms,
            })?;
        }
        Ok(())
    })
    .unwrap();
    let before: Vec<_> = (0..=20)
        .map(|i| frame(&s, 0.37, i as f32 / 10. - 1.))
        .collect();
    let base = s.document().get_blend_key_table(&table).unwrap().samples[2]
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
    assert!((s.document().get_glue(&glue).unwrap().intensity - 0.6).abs() < 1e-6);
    assert_eq!(
        s.document()
            .get_blend_key_table(&table)
            .unwrap()
            .base_key_idx,
        2
    );
    roundtrip(&s);
}
#[test]
fn noop_shared_edits_preserve_redo_and_native_invalid_ids_are_rejected() {
    let mut s = shared();
    let t = s.document().get_blend_key_table(&id(1701)).unwrap().clone();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: t.id.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: id(1900),
                    value: 0.5,
                },
            },
        }),
    );
    s.undo().unwrap();
    let history = s.history_state();
    let receipt = apply(
        &mut s,
        E::Blend(B::Table {
            id: t.id.clone(),
            operation: BO::Move {
                sample_id: t.samples[0].id.clone(),
                value: t.samples[0].value,
            },
        }),
    );
    assert!(!receipt.changed);
    assert_eq!(s.history_state(), history);
    let mut d = s.document().fork_candidate();
    let mut a = d.get_asset(ASSET_ID).unwrap().clone();
    a.source = "assets/texture.png".into();
    assert!(d.replace_asset(a).status.is_ok());
    let mut wire: serde_json::Value =
        serde_json::from_str(&kasane_project::encode_project(&d).unwrap()).unwrap();
    wire["document"]["bindings"][0]["axes"][0]["id"] = serde_json::json!("");
    assert_eq!(
        kasane_project::decode_project(&wire.to_string())
            .unwrap_err()
            .code,
        "INVALID_ID"
    );
}
#[test]
fn driver_remapping_is_atomic_and_preserves_sample_identity() {
    let mut s = base();
    s.edit("new domain", None, |e| {
        e.create_parameter(Parameter {
            id: id(1910),
            minimum: 10.,
            maximum: 20.,
            default_value: 15.,
            ..Default::default()
        })
    })
    .unwrap();
    let old = s.binding(BINDING_ID).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::RemapParameter {
            axis_id: old.axes[0].id.clone(),
            parameter_id: id(1910),
            policy: kasane_sdk::ParameterRangePolicy::MapKeyValues,
        },
    );
    let new = s.binding(BINDING_ID).unwrap();
    assert_eq!(new.axes[0].id, old.axes[0].id);
    assert_eq!(new.axes[0].samples[0].id, old.axes[0].samples[0].id);
    assert_eq!(new.axes[0].keys, vec![10., 20.]);
    assert_eq!(new.keyforms[1].samples, old.keyforms[1].samples);
    roundtrip(&s);
}

#[test]
fn shared_mesh_rebase_keeps_colors_opacity_and_draw_order_and_honors_child_locks() {
    let mut s = shared();
    for n in [1702, 1703] {
        let mut binding = s.document().get_blend_binding(&id(n)).unwrap().clone();
        if let DeltaKeyforms::Mesh(forms) = &mut binding.keyforms {
            for (f, v) in forms.iter_mut().zip([-4., 0., 8.]) {
                f.opacity = Some(-v * 0.01);
                f.draw_order = Some(v);
                f.multiply = Some([-v * 0.005; 3]);
                f.screen = Some([v * 0.002; 3]);
            }
        }
        s.edit("appearance deltas", None, |e| {
            e.replace_blend_binding(binding)
        })
        .unwrap();
    }
    let table = s.document().get_blend_key_table(&id(1701)).unwrap().clone();
    let intent = E::Blend(B::Table {
        id: table.id.clone(),
        operation: BO::SetBase {
            sample_id: table.samples[2].id.clone(),
        },
    });
    for child in [
        &table.samples[2].id,
        &s.binding(BINDING_ID).unwrap().axes[0].samples[0].id,
    ] {
        let mut flags = s.document().editor_state().clone();
        flags.objects.insert(
            child.clone(),
            kasane_core::document::ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        s.edit("lock child", None, |e| e.replace_editor_state(flags))
            .unwrap();
        let version = s.version();
        assert_eq!(
            s.plan_binding_authoring(intent.clone())
                .unwrap_err()
                .code
                .as_ref(),
            "OBJECT_LOCKED"
        );
        assert_eq!(s.version(), version);
        s.undo().unwrap();
    }
    let expected: Vec<_> = (0..=16)
        .map(|i| frame(&s, 0.37, i as f32 / 8. - 1.))
        .collect();
    let receipt = apply(&mut s, intent);
    assert!(receipt
        .changes
        .binding_references
        .iter()
        .any(|r| r.entity.id == table.id && r.field == "base_sample_id"));
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
    let inverse = s.undo().unwrap();
    assert!(inverse
        .changes
        .binding_references
        .iter()
        .any(|r| r.field == "base_sample_id"
            && r.after.as_deref() == Some(table.base_sample_id.as_str())));
}

#[test]
fn pending_shared_table_builds_from_empty_and_samples_its_retained_domain() {
    let mut s = shared();
    let table = id(1950);
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: table.clone(),
                parameter_id: String::new(),
                samples: vec![],
                base_sample_id: String::new(),
                keys: vec![],
                base_key_idx: 0,
            },
        }),
    );
    apply(
        &mut s,
        E::Blend(B::CreateBinding {
            id: id(1951),
            target_id: MESH_ID.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: table.clone(),
        }),
    );
    for (n, v) in [(1952, -1.), (1953, 1.)] {
        apply(
            &mut s,
            E::Blend(B::Table {
                id: table.clone(),
                operation: BO::Insert {
                    sample: BindingSample {
                        id: id(n),
                        value: v,
                    },
                },
            }),
        );
    }
    let mut binding = s.document().get_blend_binding(&id(1951)).unwrap().clone();
    if let DeltaKeyforms::Mesh(f) = &mut binding.keyforms {
        f[1].positions.fill(Vec2::new(6., 0.));
    }
    s.edit("pending deltas", None, |e| e.replace_blend_binding(binding))
        .unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: id(1954),
                    value: 0.,
                },
            },
        }),
    );
    let DeltaKeyforms::Mesh(f) = &s.document().get_blend_binding(&id(1951)).unwrap().keyforms
    else {
        panic!()
    };
    assert_eq!(f[1].positions[0].x, 3.);
    let before = frame(&s, 0.4, 0.).drawables[0].positions[0].x;
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetParameter {
                parameter_id: id(1700),
            },
        }),
    );
    assert_eq!(frame(&s, 0.4, 0.).drawables[0].positions[0].x, before + 3.);
    let address = s.binding_field_address(&id(1954)).unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetParameter {
                parameter_id: String::new(),
            },
        }),
    );
    s.undo().unwrap();
    assert!(s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.2))
        .is_err());
    roundtrip(&s);
}

#[test]
fn native_rejects_missing_shared_and_glue_identities_and_numeric_adapters_keep_ids() {
    let mut s = shared();
    let g = id(1960);
    s.edit("glue", None, |e| {
        e.create_glue(Glue {
            id: g.clone(),
            runtime_id: g.clone(),
            name: String::new(),
            mesh_a_id: MESH_ID.into(),
            mesh_b_id: MESH_ID.into(),
            pairs: vec![],
            intensity: 1.,
            binding: Some(GlueBinding {
                axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
                keyforms: vec![
                    GlueKeyform {
                        samples: vec![],
                        intensity: 1.
                    };
                    2
                ],
            }),
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Blend(B::CreateConstraint {
            constraint: BlendShapeConstraint {
                id: id(1961),
                parameter_id: PARAMETER_ID.into(),
                samples: vec![],
                keys: vec![0., 1.],
                weights: vec![0., 1.],
            },
        }),
    );
    let old = s.binding(BINDING_ID).unwrap();
    let mut numeric = old.clone();
    for axis in &mut numeric.axes {
        axis.id.clear();
        axis.samples.clear();
    }
    for form in &mut numeric.keyforms {
        form.samples.clear();
    }
    let (_, receipt) = s
        .edit("numeric import adapter", None, |e| {
            e.replace_binding(numeric)
        })
        .unwrap();
    assert!(!receipt.changed);
    assert_eq!(s.binding(BINDING_ID).unwrap(), old);
    let mut d = s.document().fork_candidate();
    let mut a = d.get_asset(ASSET_ID).unwrap().clone();
    a.source = "assets/texture.png".into();
    assert!(d.replace_asset(a).status.is_ok());
    let wire: serde_json::Value =
        serde_json::from_str(&kasane_project::encode_project(&d).unwrap()).unwrap();
    for path in [
        "/document/glues/0/binding/axes/0/id",
        "/document/glues/0/binding/axes/0/samples/0/id",
        "/document/blend_key_tables/0/base_sample_id",
        "/document/blend_key_tables/0/samples/0/id",
        "/document/blend_constraints/0/samples/0/id",
        "/document/blend_bindings/0/keyforms/items/0/sample_id",
    ] {
        let mut invalid = wire.clone();
        *invalid.pointer_mut(path).unwrap() = serde_json::json!("");
        assert!(
            kasane_project::decode_project(&invalid.to_string()).is_err(),
            "{path}"
        );
    }
    let mut invalid = wire.clone();
    invalid["document"]["glues"][0]["binding"]["keyforms"][0]["samples"] = serde_json::json!([]);
    assert!(kasane_project::decode_project(&invalid.to_string()).is_err());
    assert!(s
        .document()
        .references_to(&old.axes[0].samples[0].id)
        .contains(&BINDING_ID.to_string()));
}

#[test]
fn owned_conversion_edits_keep_limits_and_recheck_them_at_publication() {
    let mut s = base();
    let axis = s.binding(BINDING_ID).unwrap().axes[0].id.clone();
    let intent = E::Grid(G::Edit {
        owner: BINDING_ID.into(),
        operation: O::InsertSample {
            axis_id: axis.clone(),
            id: id(1980),
            value: 0.5,
        },
    });
    let plan = s.plan_binding_authoring(intent.clone()).unwrap();
    let mut edit = s.begin_owned_edit("owned conversion", None).unwrap();
    edit.binding_authoring(intent).unwrap();
    let before = s.version();
    let h = s.history_state();
    let mut limits = s.authoring_limits();
    limits.max_conversion_bytes = 1;
    s.set_authoring_limits(limits).unwrap();
    assert_eq!(
        edit.commit_to(&mut s).unwrap_err().code.as_ref(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
    s.set_authoring_limits(Default::default()).unwrap();
    let mut edit = s.begin_owned_edit("owned conversion", None).unwrap();
    edit.apply_binding_plan(&plan).unwrap();
    edit.commit_to(&mut s).unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        id(1980),
        kasane_core::document::ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    s.edit("hide child", None, |e| e.replace_editor_state(flags))
        .unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::DeleteSample {
            sample_id: id(1980),
        },
    );
    assert!(!s.document().editor_state().objects.contains_key(&id(1980)));
    s.undo().unwrap();
    assert!(s.document().editor_state().get(&id(1980)).hidden_in_editor);
}

#[test]
fn keyform_setters_resolve_stable_combinations_after_axis_reorder() {
    let mut s = base();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let source = s.binding(BINDING_ID).unwrap();
    let mut form = source.keyforms[1].clone();
    form.keys.clear();
    form.appearance.opacity = 0.7;
    op(
        &mut s,
        BINDING_ID,
        O::ReorderAxes {
            axis_ids: source.axes.iter().rev().map(|a| a.id.clone()).collect(),
        },
    );
    s.edit("stable pose", None, |e| {
        e.set_mesh_keyform(BINDING_ID, form)
    })
    .unwrap();
    let after = s.binding(BINDING_ID).unwrap();
    let f = after
        .keyforms
        .iter()
        .find(|f| f.keys == vec![-1., 1.])
        .unwrap();
    assert_eq!(f.appearance.opacity, 0.7);
    assert_eq!(
        f.samples,
        vec![
            source.axes[1].samples[0].id.clone(),
            source.axes[0].samples[1].id.clone()
        ]
    );
    roundtrip(&s);
}

#[test]
fn zero_offset_rebase_still_rejects_changed_optional_color_selection() {
    let mut s = shared();
    let mut b = s.document().get_blend_binding(&id(1702)).unwrap().clone();
    if let DeltaKeyforms::Mesh(f) = &mut b.keyforms {
        for f in f.iter_mut() {
            f.positions.fill(Vec2::default());
        }
        f[2].multiply = Some([0.1; 3]);
    }
    s.edit("optional channels", None, |e| e.replace_blend_binding(b))
        .unwrap();
    let t = s.document().get_blend_key_table(&id(1701)).unwrap();
    let intent = E::Blend(B::Table {
        id: t.id.clone(),
        operation: BO::SetBase {
            sample_id: t.samples[0].id.clone(),
        },
    });
    let before = s.version();
    let history = s.history_state();
    assert_eq!(
        s.plan_binding_authoring(intent).unwrap_err().code.as_ref(),
        "OPTIONAL_CHANNEL_NOT_REPRESENTABLE"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), history);
}

#[test]
fn shared_base_removal_and_table_switch_preserve_retained_samples_or_require_reset() {
    let mut s = shared();
    let old = s.document().get_blend_key_table(&id(1701)).unwrap().clone();
    let endpoints = [
        frame(&s, 0.37, -1.).drawables[0].positions.clone(),
        frame(&s, 0.37, 1.).drawables[0].positions.clone(),
    ];
    let handle = s
        .handle(ObjectKind::BindingSample, &old.base_sample_id)
        .unwrap();
    let plan = s
        .plan_binding_authoring(E::Blend(B::Table {
            id: old.id.clone(),
            operation: BO::Delete {
                sample_id: old.base_sample_id.clone(),
                replacement_base: Some(old.samples[0].id.clone()),
            },
        }))
        .unwrap();
    let kasane_sdk::BindingConversion::Blend(conversion) = plan.conversion() else {
        panic!()
    };
    assert_eq!(conversion.losses().len(), 1);
    s.execute_binding_plan(&plan).unwrap();
    for (i, value) in [-1., 1.].into_iter().enumerate() {
        assert_eq!(frame(&s, 0.37, value).drawables[0].positions, endpoints[i]);
    }
    let source = s.document().get_blend_key_table(&old.id).unwrap().clone();
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: id(1990),
                parameter_id: source.parameter_id.clone(),
                keys: source.keys.clone(),
                base_key_idx: source.base_key_idx,
                samples: vec![],
                base_sample_id: String::new(),
            },
        }),
    );
    apply(
        &mut s,
        E::Blend(B::SetBindingTable {
            id: id(1702),
            key_table_id: id(1990),
            reset: false,
        }),
    );
    for (i, value) in [-1., 1.].into_iter().enumerate() {
        assert_eq!(frame(&s, 0.37, value).drawables[0].positions, endpoints[i]);
    }
    assert_eq!(
        s.document()
            .get_blend_binding(&id(1703))
            .unwrap()
            .key_table_id,
        old.id
    );
    let before = s.version();
    let history = s.history_state();
    assert_eq!(
        s.plan_binding_authoring(E::Blend(B::SetBindingTable {
            id: id(1702),
            key_table_id: String::new(),
            reset: false
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "BLEND_REMAP_REQUIRED"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), history);
    let plan = s
        .plan_binding_authoring(E::Blend(B::SetBindingTable {
            id: id(1702),
            key_table_id: String::new(),
            reset: true,
        }))
        .unwrap();
    let kasane_sdk::BindingConversion::Blend(conversion) = plan.conversion() else {
        panic!()
    };
    assert_eq!(conversion.losses().len(), 1);
    s.execute_binding_plan(&plan).unwrap();
    assert!(!s
        .document()
        .blend_binding_active(s.document().get_blend_binding(&id(1702)).unwrap()));
    roundtrip(&s);
    while s.document().binding_identity(&old.base_sample_id).is_none() {
        s.undo().unwrap();
    }
    assert!(s.resolve_handle(&handle).is_err());
}

#[test]
fn table_field_context_tracks_driver_domains_even_when_values_do_not_move() {
    let mut s = shared();
    let table = s.document().get_blend_key_table(&id(1701)).unwrap().clone();
    let address = s.binding_field_address(&table.samples[0].id).unwrap();
    let mut parameter = s
        .document()
        .get_parameter(&table.parameter_id)
        .unwrap()
        .clone();
    parameter.minimum = -2.;
    parameter.maximum = 2.;
    s.edit("domain", None, |e| e.replace_parameter(parameter))
        .unwrap();
    s.undo().unwrap();
    assert_eq!(
        s.plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(-0.8))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );
    s.edit("new blend driver", None, |e| {
        e.create_parameter(Parameter {
            id: id(1991),
            kind: ParameterKind::BlendShape,
            minimum: 10.,
            maximum: 20.,
            default_value: 15.,
            ..Default::default()
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.id.clone(),
            operation: BO::RemapParameter {
                parameter_id: id(1991),
                policy: kasane_sdk::ParameterRangePolicy::MapKeyValues,
            },
        }),
    );
    let mapped = s.document().get_blend_key_table(&table.id).unwrap();
    assert_eq!(mapped.keys, vec![10., 15., 20.]);
    assert_eq!(mapped.base_sample_id, table.base_sample_id);
    assert_eq!(
        mapped.samples.iter().map(|s| &s.id).collect::<Vec<_>>(),
        table.samples.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    roundtrip(&s);
}

#[test]
fn topology_replacement_cannot_revive_deleted_child_handles_in_one_edit() {
    let mut s = base();
    let original = s.binding(BINDING_ID).unwrap();
    let handle = s
        .handle(ObjectKind::BindingSample, &original.axes[0].samples[0].id)
        .unwrap();
    let source = s.geometry(MESH_ID).unwrap();
    let mesh = s.document().get_mesh(MESH_ID).unwrap().clone();
    let replacement = kasane_sdk::TopologyReplacement {
        vertex_mapping: mesh.vertex_ids.iter().map(|id| (*id, Some(*id))).collect(),
        mesh,
        binding: Some(MeshBinding {
            id: BINDING_ID.into(),
            mesh_id: MESH_ID.into(),
            axes: vec![],
            keyforms: vec![],
        }),
        blend_bindings: vec![],
        glues: vec![],
    };
    let (_, receipt) = s
        .edit("recreate through topology", None, |e| {
            e.replace_topology(&source, replacement)?;
            e.replace_binding(original.clone())
        })
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(s.binding(BINDING_ID).unwrap(), original);
    assert!(s.resolve_handle(&handle).is_err());
}

#[test]
fn offscreen_blend_colors_survive_insert_and_rebase() {
    let mut s = shared();
    s.edit("offscreen color fixture", None, |e| {
        e.create_part(Part {
            id: id(2100),
            ..Default::default()
        })?;
        e.create_offscreen(Offscreen {
            id: id(2101),
            part_id: id(2100),
            keyforms: vec![OffscreenKeyform {
                opacity: 0.7,
                multiply: Some([0.8; 3]),
                screen: Some([0.1; 3]),
            }],
            ..Default::default()
        })?;
        e.create_blend_binding(BlendShapeBinding {
            id: id(2102),
            target_id: id(2101),
            target_kind: BlendShapeTargetKind::Offscreen,
            key_table_id: id(1701),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Offscreen(
                [-1., 0., 1.]
                    .into_iter()
                    .map(|v| DeltaOffscreenKeyform {
                        opacity: v * 0.1,
                        multiply: Some([1. + v * 0.2; 3]),
                        screen: Some([v * 0.05; 3]),
                        ..Default::default()
                    })
                    .collect(),
            ),
        })
    })
    .unwrap();
    let expected: Vec<_> = (0..=20)
        .map(|i| frame(&s, 0.37, i as f32 / 10. - 1.).offscreens[0].clone())
        .collect();
    let base = s.document().get_blend_key_table(&id(1701)).unwrap().samples[2]
        .id
        .clone();
    for operation in [
        BO::Insert {
            sample: BindingSample {
                id: id(2103),
                value: 0.5,
            },
        },
        BO::SetBase { sample_id: base },
    ] {
        apply(
            &mut s,
            E::Blend(B::Table {
                id: id(1701),
                operation,
            }),
        );
        let check = |s: &AuthoringSession| {
            for (i, expected) in expected.iter().enumerate() {
                let actual = &frame(s, 0.37, i as f32 / 10. - 1.).offscreens[0];
                for (a, b) in actual
                    .multiply_color
                    .iter()
                    .chain(&actual.screen_color)
                    .chain([&actual.opacity])
                    .zip(
                        expected
                            .multiply_color
                            .iter()
                            .chain(&expected.screen_color)
                            .chain([&expected.opacity]),
                    )
                {
                    assert!(
                        (a - b).abs() < 1e-6,
                        "parameter {}: {a} != {b}",
                        i as f32 / 10. - 1.
                    );
                }
            }
        };
        check(&s);
        s.undo().unwrap();
        check(&s);
        s.redo().unwrap();
        check(&s);
    }
    roundtrip(&s);
}

#[test]
fn offscreen_blend_edits_respect_owner_locks_and_retargeting() {
    let mut s = shared();
    s.edit("offscreen targets", None, |e| {
        for n in [2110, 2111] {
            e.create_part(Part {
                id: id(n),
                ..Default::default()
            })?;
            e.create_offscreen(Offscreen {
                id: id(n + 2),
                part_id: id(n),
                runtime_id: format!("offscreen{n}"),
                ..Default::default()
            })?;
        }
        e.create_blend_binding(BlendShapeBinding {
            id: id(2114),
            target_id: id(2112),
            target_kind: BlendShapeTargetKind::Offscreen,
            key_table_id: id(1701),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Offscreen(vec![DeltaOffscreenKeyform::default(); 3]),
        })
    })
    .unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        id(2111),
        kasane_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock owner", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let version = s.version();
    let intent = E::Blend(B::CreateBinding {
        id: id(2115),
        target_id: id(2113),
        target_kind: BlendShapeTargetKind::Offscreen,
        key_table_id: id(1701),
    });
    assert_eq!(
        s.plan_binding_authoring(intent).unwrap_err().code.as_ref(),
        "OBJECT_LOCKED"
    );
    let mut b = s.document().get_blend_binding(&id(2114)).unwrap().clone();
    b.target_id = id(2113);
    assert_eq!(
        s.edit("retarget", None, |e| e.replace_blend_binding(b))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    let mut offscreen = s.document().get_offscreen(&id(2113)).unwrap().clone();
    offscreen.enabled = false;
    assert_eq!(
        s.edit("replace locked offscreen", None, |e| e
            .replace_offscreen(offscreen))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
}

#[test]
fn property_patch_rejects_duplicate_sample_coordinates() {
    let mut s = base();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let b = s.binding(BINDING_ID).unwrap();
    let f = &b.keyforms[0];
    let patch = kasane_core::document::PropertyPatch::Mesh {
        id: MESH_ID.into(),
        keyform: Some((BINDING_ID.into(), vec![f.samples[0].clone(); 2])),
        appearance: Appearance {
            opacity: 0.25,
            ..f.appearance
        },
        draw_order: f.draw_order,
    };
    let version = s.version();
    assert_eq!(
        s.edit("invalid coordinate", None, |e| e.patch_property(&patch))
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_PROPERTY_TARGET"
    );
    assert_eq!(s.version(), version);
    let mut patch = patch;
    if let kasane_core::document::PropertyPatch::Mesh { keyform, .. } = &mut patch {
        *keyform = Some((BINDING_ID.into(), f.samples.iter().rev().cloned().collect()));
    }
    s.edit("unordered stable coordinate", None, |e| {
        e.patch_property(&patch)
    })
    .unwrap();
    assert_eq!(
        s.binding(BINDING_ID).unwrap().keyforms[0]
            .appearance
            .opacity,
        0.25
    );
}

#[test]
fn topology_conversion_respects_owned_sample_locks() {
    let mut s = base();
    let b = s.binding(BINDING_ID).unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        b.axes[0].samples[0].id.clone(),
        kasane_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock sample", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let source = s.geometry(MESH_ID).unwrap();
    let mesh = s.document().get_mesh(MESH_ID).unwrap().clone();
    let mut changed = b;
    changed.keyforms[0].positions[0].x += 1.;
    let replacement = kasane_sdk::TopologyReplacement {
        vertex_mapping: mesh.vertex_ids.iter().map(|id| (*id, Some(*id))).collect(),
        mesh,
        binding: Some(changed),
        blend_bindings: vec![],
        glues: vec![],
    };
    let version = s.version();
    assert_eq!(
        s.edit("bypass locked sample", None, |e| e
            .replace_topology(&source, replacement))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
}

#[test]
fn warp_replacement_tracks_removed_sample_lifetime_and_locks() {
    let mut s = base();
    let transform = Transform {
        id: id(2120),
        data: TransformData::Warp(WarpTransform {
            rows: 1,
            columns: 1,
            quad: true,
            points: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(0., 1.),
                Vec2::new(1., 1.),
            ],
            bezier: None,
        }),
        ..Default::default()
    };
    let binding = SceneBinding {
        id: id(2121),
        axes: vec![BindingAxis::new("", vec![0.])],
        track: SceneTrack::Warp {
            target_id: transform.id.clone().into(),
            keyforms: vec![],
        },
    };
    s.edit("warp fixture", None, |e| {
        e.create_transform(transform.clone())?;
        e.create_scene_binding(binding.clone())
    })
    .unwrap();
    let transform = s.document().get_transform(&transform.id).unwrap().clone();
    let sample = &binding.axes[0].samples[0].id;
    let handle = s.handle(ObjectKind::BindingSample, sample).unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        sample.clone(),
        kasane_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock sample", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let mut empty = binding.clone();
    empty.axes.clear();
    let version = s.version();
    assert_eq!(
        s.edit("bypass warp sample lock", None, |e| e
            .replace_warp_authoring(transform.clone(), Some(empty.clone())))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
    s.undo().unwrap();
    s.edit("remove and restore warp sample", None, |e| {
        e.replace_warp_authoring(transform.clone(), Some(empty))?;
        e.replace_warp_authoring(transform, Some(binding.clone()))
    })
    .unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    assert_eq!(s.document().get_scene_binding(&binding.id), Some(&binding));
}
