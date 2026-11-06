use super::*;

fn fixture() -> AuthoringSession {
    let mut s = session();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(2),
                        data: TransformData::Warp(WarpTransform {
                            rows: 2,
                            columns: 2,
                            quad: true,
                            bezier: None,
                            points: (0..=2)
                                .flat_map(|y| {
                                    (0..=2).map(move |x| Vec2::new(x as f32 * 25., y as f32 * 25.))
                                })
                                .collect(),
                        }),
                        ..Default::default()
                    },
                )?;
                let mut child = WarpTransform {
                    rows: 2,
                    columns: 2,
                    quad: false,
                    bezier: None,
                    points: (0..=2)
                        .flat_map(|y| {
                            (0..=2).map(move |x| Vec2::new(x as f32 * 0.5, y as f32 * 0.5))
                        })
                        .collect(),
                };
                child.points[4].y += 0.1;
                child.bezier = Some(WarpBezier::from_lattice(2, 2, false, &child.points));
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(4),
                        parent_id: Some(id(2).into()),
                        data: TransformData::Warp(child.clone()),
                        ..Default::default()
                    },
                )?;
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(6),
                        parent_id: Some(id(2).into()),
                        data: TransformData::Rotation(RotationTransform {
                            base_angle: 12.,
                            pose: RotationPose {
                                origin: PreciseVec2::new(0.4, 0.5),
                                angle: 7.,
                                scale: 1.2,
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                )?;
                for (n, parent) in [(3, 2), (5, 4), (7, 6)] {
                    e.create_mesh({
                        let mut value = Mesh {
                            id: id(n),
                            deformer_id: id(parent),
                            vertex_ids: vec![1, 2, 3],
                            base_positions: vec![
                                Vec2::new(0.1, 0.2),
                                Vec2::new(0.8, 0.2),
                                Vec2::new(0.3, 0.9),
                            ],
                            uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                            triangles: vec![[1, 2, 3]],
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                }
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(10),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(11),
                        kind: ParameterKind::BlendShape,
                        minimum: 0.,
                        maximum: 1.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateTable {
                        table: BlendShapeKeyTable {
                            id: id(12),
                            parameter_id: id(11),
                            keys: vec![0., 1.],
                            base_key_idx: 0,
                            samples: vec![],
                            base_sample_id: String::new(),
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                e.create_binding(MeshBinding {
                    id: id(13),
                    mesh_id: id(3),
                    axes: vec![BindingAxis::new(id(10), vec![-1., 1.])],
                    keyforms: [-1., 1.]
                        .iter()
                        .map(|&v| MeshKeyform {
                            keys: vec![v],
                            positions: vec![
                                Vec2::new(0.1 + v * 0.1, 0.2),
                                Vec2::new(0.8, 0.2),
                                Vec2::new(0.3, 0.9),
                            ],
                            ..Default::default()
                        })
                        .collect(),
                })?;
                e.create_scene_binding(SceneBinding {
                    id: id(14),
                    axes: vec![BindingAxis::new(id(10), vec![-1., 1.])],
                    track: SceneTrack::Warp {
                        target_id: id(4).into(),
                        keyforms: [-1., 1.]
                            .iter()
                            .map(|&v| WarpKeyform {
                                keys: vec![v],
                                positions: child
                                    .points
                                    .iter()
                                    .map(|p| Vec2::new(p.x + v * 0.05, p.y))
                                    .collect(),
                                bezier: child.bezier.clone(),
                                ..Default::default()
                            })
                            .collect(),
                    },
                })?;
                e.create_scene_binding(SceneBinding {
                    id: id(16),
                    axes: vec![BindingAxis::new(id(10), vec![-1., 1.])],
                    track: SceneTrack::Rotation {
                        target_id: id(6).into(),
                        keyforms: [-1., 1.]
                            .iter()
                            .map(|&v| RotationKeyform {
                                keys: vec![v],
                                rotation: RotationPose {
                                    origin: PreciseVec2::new(0.4 + f64::from(v) * 0.1, 0.5),
                                    angle: 7. + v * 30.,
                                    scale: 1.2 + v * 0.1,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .collect(),
                    },
                })?;
                for (target, kind, keyforms) in [
                    (
                        3,
                        BlendShapeTargetKind::Mesh,
                        DeltaKeyforms::Mesh(vec![
                            DeltaMeshKeyform {
                                positions: vec![Vec2::default(); 3],
                                ..Default::default()
                            },
                            DeltaMeshKeyform {
                                positions: vec![Vec2::new(0.02, -0.01); 3],
                                ..Default::default()
                            },
                        ]),
                    ),
                    (
                        4,
                        BlendShapeTargetKind::Warp,
                        DeltaKeyforms::Warp(vec![
                            DeltaWarpKeyform {
                                points: vec![Vec2::default(); 9],
                                ..Default::default()
                            },
                            DeltaWarpKeyform {
                                points: vec![Vec2::new(0.01, 0.02); 9],
                                ..Default::default()
                            },
                        ]),
                    ),
                    (
                        6,
                        BlendShapeTargetKind::Rotation,
                        DeltaKeyforms::Rotation(vec![
                            DeltaRotationKeyform::default(),
                            DeltaRotationKeyform {
                                origin: Some(Vec2::new(0.01, 0.02)),
                                angle: Some(4.),
                                scale: Some(0.1),
                                ..Default::default()
                            },
                        ]),
                    ),
                ] {
                    {
                        let binding = BlendShapeBinding {
                            id: id(20 + target),
                            target_id: id(target),
                            target_kind: kind,
                            key_table_id: id(12),
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
                let prepared = draft.prepare("Warp preservation fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}
fn next(s: &AuthoringSession) -> WarpTransform {
    let mut warp = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .clone();
    for p in &mut warp.points {
        *p = Vec2::new(4. + p.x * 1.1 + p.y * 0.2, -3. + p.x * 0.3 + p.y * 0.8);
    }
    warp
}
fn positions(doc: &zipzi_core::state::DocumentState, x: f32, b: f32) -> Vec<Vec<Vec2>> {
    let mut frame = DrawableFrame::default();
    let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
        doc,
        &std::collections::HashMap::from([(id(10), x), (id(11), b)]),
        &mut frame,
    );
    assert!(status.is_ok(), "{}", status.message);
    frame.drawables.into_iter().map(|d| d.positions).collect()
}

fn rotation_fixture(parent: u8) -> AuthoringSession {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                if (1..=2).contains(&parent) {
                    crate::support::insert_transform(
                        e,
                        Transform {
                            id: id(30),
                            data: if parent == 1 {
                                TransformData::Warp(WarpTransform {
                                    rows: 1,
                                    columns: 1,
                                    quad: true,
                                    bezier: None,
                                    points: vec![
                                        Vec2::new(10., 20.),
                                        Vec2::new(210., 20.),
                                        Vec2::new(10., 320.),
                                        Vec2::new(210., 320.),
                                    ],
                                })
                            } else {
                                TransformData::Rotation(RotationTransform {
                                    base_angle: -12.,
                                    pose: RotationPose {
                                        scale: 1.5,
                                        reflect_x: true,
                                        ..Default::default()
                                    },
                                })
                            },
                            ..Default::default()
                        },
                    )?;
                }
                let mut root = e
                    .read_document()
                    .unwrap()
                    .transform(&id(2))
                    .unwrap()
                    .clone();
                root.data = TransformData::Rotation(RotationTransform {
                    base_angle: 13.,
                    pose: RotationPose {
                        origin: PreciseVec2::new(0.2, 0.4),
                        angle: 23.,
                        scale: 1.2,
                        reflect_x: parent == 3,
                        ..Default::default()
                    },
                });
                root.original_shape = None;
                root.warp_authoring = None;
                root.parent_id = (1..=2).contains(&parent).then(|| id(30).into());
                e.replace_transform(root)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Rotation preservation fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}

#[test]
fn rotation_keep_children_preserves_all_channels_and_nested_deformers_under_static_parents() {
    for parent in 0..4 {
        let mut s = rotation_fixture(parent);
        let before = s.snapshot().document().clone();
        let history = {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        };
        let mut rotation = before
            .transform(&id(2))
            .unwrap()
            .rotation()
            .unwrap()
            .clone();
        rotation.pose.origin.x += 0.125;
        rotation.pose.origin.y -= 0.25;
        rotation.pose.angle += 17.;
        rotation.pose.scale *= 1.25;
        let edit = E::RotationKeepChildren {
            transform_id: id(2),
            rotation,
        };
        let plan = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_scene_content(
                edit.clone(),
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap();
        assert!(plan.0.report.preservation_samples > 0);
        assert!(plan.0.report.preservation_error.unwrap() < 0.001);
        let stale = s.begin_draft();
        s.publish(plan.1).unwrap();
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            (history.0 + 1, 0)
        );
        for x in [-1., -0.73, 0., 0.39, 1.] {
            for blend in [0., 0.27, 0.8, 1.] {
                for (a, b) in positions(&before, x, blend).iter().zip(positions(
                    s.snapshot().document(),
                    x,
                    blend,
                )) {
                    for (a, b) in a.iter().zip(b) {
                        assert!(
                            (a.x - b.x).hypot(a.y - b.y) < 0.001,
                            "parent={parent}: {a:?} vs {b:?}"
                        );
                    }
                }
            }
        }
        for child in [5, 7] {
            assert_eq!(
                before.mesh(&id(child)),
                s.snapshot().document().mesh(&id(child))
            );
        }
        let after = s.snapshot().document().clone();
        let noop = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_scene_content(edit, Default::default(), &Default::default())
        })
        .unwrap();
        assert!(noop.1.writes().is_empty());
        s.publish(noop.1).unwrap();
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            (history.0 + 1, 0)
        );
        s.undo(s.version()).unwrap();
        assert_eq!(s.snapshot().content_id(), before.content_id());
        assert_eq!(
            s.publish(stale.prepare("old source").unwrap())
                .unwrap_err()
                .code
                .as_str(),
            "STALE_SOURCE"
        );
        s.redo(s.version()).unwrap();
        assert_eq!(s.snapshot().content_id(), after.content_id());
    }
}

#[test]
fn rotation_keep_children_rejects_driven_roots_locked_children_and_singular_maps_atomically() {
    for case in 0..3 {
        let mut s = rotation_fixture(0);
        if case == 0 {
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| {
                        e.create_scene_binding(SceneBinding {
                            id: id(31),
                            axes: vec![BindingAxis::new(id(10), vec![-1., 1.])],
                            track: SceneTrack::Rotation {
                                target_id: id(2).into(),
                                keyforms: [-1., 1.]
                                    .into_iter()
                                    .map(|v| RotationKeyform {
                                        keys: vec![v],
                                        rotation: RotationPose::default(),
                                        ..Default::default()
                                    })
                                    .collect(),
                            },
                        })
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("Driven root")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
        } else if case == 1 {
            lock(&mut s, &id(3));
        }
        let before = s.snapshot().document().clone();
        let history = {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        };
        let mut rotation = before
            .transform(&id(2))
            .unwrap()
            .rotation()
            .unwrap()
            .clone();
        rotation.pose.angle += 20.;
        if case == 2 {
            rotation.pose.scale = 0.;
        }
        assert!(
            crate::support::prepared(&s, |candidate_commands| candidate_commands
                .edit_scene_content(
                    E::RotationKeepChildren {
                        transform_id: id(2),
                        rotation
                    },
                    Default::default(),
                    &Default::default()
                ))
            .is_err()
        );
        assert_eq!(s.snapshot().content_id(), before.content_id());
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            history
        );
    }
}

#[test]
fn affine_keep_children_converts_all_channels_nested_warp_and_rotation_in_one_history_step() {
    let mut s = fixture();
    let before = s.snapshot().document().clone();
    let history = s.history_state();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::WarpKeepChildren {
                transform_id: id(2),
                warp: next(&s),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert!(plan.0.report.preservation_samples > 121);
    assert!(plan.0.report.preservation_error.unwrap() < 0.001);
    s.publish(plan.1).unwrap();
    for x in [-1., -0.73, -0.1, 0., 0.39, 1.] {
        for b in [0., 0.27, 0.8, 1.] {
            for (a, b) in
                positions(&before, x, b)
                    .iter()
                    .zip(positions(s.snapshot().document(), x, b))
            {
                for (a, b) in a.iter().zip(b) {
                    assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} {b:?}");
                }
            }
        }
    }
    assert_ne!(before.mesh(&id(3)), s.snapshot().document().mesh(&id(3)));
    assert_ne!(
        before.scene_binding(&id(14)),
        s.snapshot().document().scene_binding(&id(14))
    );
    assert_ne!(
        before.blend_binding(&id(26)),
        s.snapshot().document().blend_binding(&id(26))
    );
    let after = s.snapshot().document().clone();
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(s.history_state().undo_steps, history.undo_steps);
    s.redo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), after.content_id());
}

#[test]
fn keep_children_refuses_nonlinear_singular_driven_locked_and_stale_plans_atomically() {
    for case in ["nonlinear", "singular", "driven", "locked"] {
        let mut s = fixture();
        let mut warp = next(&s);
        match case {
            "nonlinear" => warp.points[4].x += 3.,
            "singular" => {
                for p in &mut warp.points {
                    p.y = 0.;
                }
            }
            "driven" => {
                let points = s
                    .snapshot()
                    .document()
                    .transform(&id(2))
                    .unwrap()
                    .warp()
                    .unwrap()
                    .points
                    .clone();
                {
                    let mut draft = s.begin_draft();
                    draft
                        .batch(draft.source(), |e| {
                            e.create_scene_binding(SceneBinding {
                                id: id(40),
                                axes: vec![BindingAxis::new(id(10), vec![-1., 1.])],
                                track: SceneTrack::Warp {
                                    target_id: id(2).into(),
                                    keyforms: [-1., 1.]
                                        .iter()
                                        .map(|&v| WarpKeyform {
                                            keys: vec![v],
                                            positions: points.clone(),
                                            ..Default::default()
                                        })
                                        .collect(),
                                },
                            })
                        })
                        .and_then(|value| {
                            let prepared = draft.prepare("Drive parent")?;
                            s.publish(prepared).map(|receipt| (value, receipt))
                        })
                }
                .unwrap();
            }
            "locked" => lock(&mut s, &id(14)),
            _ => unreachable!(),
        }
        let before = s.snapshot().document().clone();
        let version = s.version();
        let history = s.history_state();
        assert!(
            crate::support::prepared(&s, |candidate_commands| candidate_commands
                .edit_scene_content(
                    E::WarpKeepChildren {
                        transform_id: id(2),
                        warp
                    },
                    Default::default(),
                    &Default::default()
                ))
            .is_err(),
            "{case}"
        );
        assert_eq!(s.snapshot().content_id(), before.content_id());
        assert_eq!(version, s.version());
        assert_eq!(history, s.history_state());
    }
    let mut s = fixture();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::WarpKeepChildren {
                transform_id: id(2),
                warp: next(&s),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.rename(zipzi_sdk::NameTarget::Mesh(&id(3)), ("new").into())
            })
            .and_then(|value| {
                let prepared = draft.prepare("New version")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(s.publish(plan.1).unwrap_err().code.as_str(), "STALE_SOURCE");
}

#[test]
fn nonlinear_vertex_inverse_does_not_preserve_parameter_midpoints() {
    // G(u,v)=(u*(1+v),v). Exact inverses at two stored mesh
    // keys do not preserve their interpolated midpoint: a concrete refusal case.
    let before = [Vec2::new(0.2, 0.1), Vec2::new(0.8, 0.9)];
    let inverse = |p: Vec2| Vec2::new(p.x / (1. + p.y), p.y);
    let after = before.map(inverse);
    let mid = Vec2::new(
        (after[0].x + after[1].x) * 0.5,
        (after[0].y + after[1].y) * 0.5,
    );
    let mut output = [0.; 2];
    zipzi_core::deformers::warp_points(
        1,
        1,
        true,
        &[0., 0., 1., 0., 0., 1., 2., 1.],
        &[mid.x, mid.y],
        &mut output,
        1,
    );
    let actual = Vec2::new(output[0], output[1]);
    assert!((actual.x - 0.5).abs() > 0.04);
}

#[test]
fn reflected_affine_maps_preserve_both_native_interpolators() {
    for quad in [false, true] {
        let mut s = fixture();
        let mut transform = s.snapshot().document().transform(&id(2)).unwrap().clone();
        transform.warp_mut().unwrap().quad = quad;
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_transform(transform))
                .and_then(|value| {
                    let prepared = draft.prepare("Interpolator")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let before = s.snapshot().document().clone();
        let mut warp = next(&s);
        for p in &mut warp.points {
            p.x = -p.x;
        }
        run(
            &mut s,
            E::WarpKeepChildren {
                transform_id: id(2),
                warp,
            },
        );
        for x in [-1., -0.37, 0.7, 1.] {
            for b in [0., 0.31, 1.] {
                for (a, b) in
                    positions(&before, x, b)
                        .iter()
                        .zip(positions(s.snapshot().document(), x, b))
                {
                    for (a, b) in a.iter().zip(b) {
                        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
                    }
                }
            }
        }
    }
}

#[test]
fn canvas_sized_warp_keeps_children_during_move_rotation_and_scale() {
    for quad in [false, true] {
        for offset in [0., 4096.] {
            for operation in ["move", "rotate", "scale"] {
                let mut s = fixture();
                let mut transform = s.snapshot().document().transform(&id(2)).unwrap().clone();
                let warp = transform.warp_mut().unwrap();
                warp.rows = 5;
                warp.columns = 5;
                warp.quad = quad;
                warp.points = (0..=5)
                    .flat_map(|y| {
                        (0..=5).map(move |x| {
                            Vec2::new(
                                offset - 467. + 946. * x as f32 / 5.,
                                offset - 230. + 819. * y as f32 / 5.,
                            )
                        })
                    })
                    .collect();
                transform.warp_authoring = Some(WarpAuthoringSettings {
                    bezier_grid: Some(WarpBezierGrid {
                        rows: 2,
                        columns: 2,
                    }),
                    ..Default::default()
                });
                {
                    let mut draft = s.begin_draft();
                    draft
                        .batch(draft.source(), |e| e.replace_transform(transform))
                        .and_then(|value| {
                            let prepared = draft.prepare("Canvas-sized Warp")?;
                            s.publish(prepared).map(|receipt| (value, receipt))
                        })
                }
                .unwrap();
                let before = s.snapshot().document().clone();
                let history = s.history_state();
                let mut warp = before.transform(&id(2)).unwrap().warp().unwrap().clone();
                let center = Vec2::new(offset + 6., offset + 179.5);
                for p in &mut warp.points {
                    let (x, y) = (p.x - center.x, p.y - center.y);
                    *p = match operation {
                        "move" => Vec2::new(p.x + 14., p.y - 9.),
                        "rotate" => {
                            let (s, c) = 2f32.to_radians().sin_cos();
                            Vec2::new(center.x + x * c - y * s, center.y + x * s + y * c)
                        }
                        _ => Vec2::new(center.x + x * 1.05, center.y + y * 1.05),
                    };
                }
                let plan = crate::support::prepared(&s, |candidate_commands| {
                    candidate_commands.edit_scene_content(
                        E::WarpKeepChildren {
                            transform_id: id(2),
                            warp,
                        },
                        Default::default(),
                        &Default::default(),
                    )
                })
                .unwrap_or_else(|e| panic!("{operation}, quad={quad}, offset={offset}: {e:?}"));
                assert!(plan.0.report.preservation_error.unwrap() <= 0.01);
                s.publish(plan.1).unwrap();
                for x in [-1., -0.37, 0.9] {
                    for b in [0., 0.42, 1.] {
                        for (a, b) in positions(&before, x, b).iter().zip(positions(
                            s.snapshot().document(),
                            x,
                            b,
                        )) {
                            for (a, b) in a.iter().zip(b) {
                                assert!(
                                    (a.x - b.x).hypot(a.y - b.y) <= 0.01,
                                    "{operation}, quad={quad}, offset={offset}: {a:?} != {b:?}"
                                );
                            }
                        }
                    }
                }
                assert_eq!(s.history_state().undo_steps, history.undo_steps + 1);
                s.undo(s.version()).unwrap();
                assert_eq!(s.snapshot().content_id(), before.content_id());
            }
        }
    }
}

#[test]
fn keep_children_still_rejects_actual_float32_position_loss_atomically() {
    let s = fixture();
    let before = s.snapshot().document().clone();
    let history = s.history_state();
    let version = s.version();
    let mut warp = before.transform(&id(2)).unwrap().warp().unwrap().clone();
    for p in &mut warp.points {
        p.x += 10_000_000.;
        p.y += 10_000_000.;
    }
    let error = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::WarpKeepChildren {
                transform_id: id(2),
                warp,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap_err();
    assert_eq!(error.code.as_str(), "WARP_PRESERVATION_TOLERANCE");
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn preservation_noop_budget_and_cancellation_keep_history_and_missing_controller_intact() {
    let mut s = fixture();
    let old = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .clone();
    let version = s.version();
    let history = s.history_state();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::WarpKeepChildren {
                transform_id: id(2),
                warp: old,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert!(plan.1.writes().is_empty());
    s.publish(plan.1).unwrap();
    assert_eq!(version, s.version());
    assert_eq!(history, s.history_state());
    let cancel = ConversionControl::default();
    cancel.cancel();
    assert!(crate::support::prepared(&s, |c| c.edit_scene_content(
        E::WarpKeepChildren {
            transform_id: id(2),
            warp: next(&s)
        },
        Default::default(),
        &cancel
    ))
    .is_err());
    let desired = next(&s);
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_scene_content(
            E::WarpKeepChildren {
                transform_id: id(2),
                warp: desired
            },
            ConversionLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(history, s.history_state());
}
