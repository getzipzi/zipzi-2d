use super::*;
use zipzi_core::BindingAxisEditing;
use zipzi_core::WarpBezierEvaluation;

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
