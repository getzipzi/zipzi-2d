use super::*;

fn fixture() -> AuthoringSession {
    let mut s = session();
    s.edit("Warp preservation fixture", None, |e| {
        e.create_transform(Transform {
            id: id(2),
            data: TransformData::Warp(WarpTransform {
                rows: 2,
                columns: 2,
                quad: true,
                bezier: None,
                points: (0..=2)
                    .flat_map(|y| (0..=2).map(move |x| Vec2::new(x as f32 * 25., y as f32 * 25.)))
                    .collect(),
            }),
            ..Default::default()
        })?;
        let mut child = WarpTransform {
            rows: 2,
            columns: 2,
            quad: false,
            bezier: None,
            points: (0..=2)
                .flat_map(|y| (0..=2).map(move |x| Vec2::new(x as f32 * 0.5, y as f32 * 0.5)))
                .collect(),
        };
        child.points[4].y += 0.1;
        child.bezier = Some(WarpBezier::from_lattice(2, 2, false, &child.points));
        e.create_transform(Transform {
            id: id(4),
            parent_id: Some(id(2).into()),
            data: TransformData::Warp(child.clone()),
            ..Default::default()
        })?;
        e.create_transform(Transform {
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
        })?;
        for (n, parent) in [(3, 2), (5, 4), (7, 6)] {
            e.create_mesh(Mesh {
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
            })?;
        }
        e.create_parameter(Parameter {
            id: id(10),
            ..Default::default()
        })?;
        e.create_parameter(Parameter {
            id: id(11),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(12),
            parameter_id: id(11),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: vec![],
            base_sample_id: String::new(),
        })?;
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
            e.create_blend_binding(BlendShapeBinding {
                id: id(20 + target),
                target_id: id(target),
                target_kind: kind,
                key_table_id: id(12),
                constraint_ids: vec![],
                keyforms,
            })?;
        }
        Ok(())
    })
    .unwrap();
    s
}
fn next(s: &AuthoringSession) -> WarpTransform {
    let mut warp = s
        .document()
        .get_transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .clone();
    for p in &mut warp.points {
        *p = Vec2::new(4. + p.x * 1.1 + p.y * 0.2, -3. + p.x * 0.3 + p.y * 0.8);
    }
    warp
}
fn positions(doc: &zipzi_core::Document, x: f32, b: f32) -> Vec<Vec<Vec2>> {
    let mut frame = DrawableFrame::default();
    let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
        doc,
        &std::collections::HashMap::from([(id(10), x), (id(11), b)]),
        &mut frame,
    );
    assert!(status.is_ok(), "{}", status.message);
    frame.drawables.into_iter().map(|d| d.positions).collect()
}

#[test]
fn affine_keep_children_converts_all_channels_nested_warp_and_rotation_in_one_history_step() {
    let mut s = fixture();
    let before = s.document().clone();
    let history = s.history_state();
    let plan = s
        .plan_scene_content(E::WarpKeepChildren {
            transform_id: id(2),
            warp: next(&s),
        })
        .unwrap();
    assert!(plan.report().preservation_samples > 121);
    assert!(plan.report().preservation_error.unwrap() < 0.001);
    s.execute_scene_content_plan(&plan).unwrap();
    for x in [-1., -0.73, -0.1, 0., 0.39, 1.] {
        for b in [0., 0.27, 0.8, 1.] {
            for (a, b) in positions(&before, x, b)
                .iter()
                .zip(positions(s.document(), x, b))
            {
                for (a, b) in a.iter().zip(b) {
                    assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} {b:?}");
                }
            }
        }
    }
    assert_ne!(before.get_mesh(&id(3)), s.document().get_mesh(&id(3)));
    assert_ne!(
        before.get_scene_binding(&id(14)),
        s.document().get_scene_binding(&id(14))
    );
    assert_ne!(
        before.get_blend_binding(&id(26)),
        s.document().get_blend_binding(&id(26))
    );
    let after = s.document().clone();
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    assert_eq!(s.history_state().undo_steps, history.undo_steps);
    s.redo().unwrap();
    assert!(s.document().same_content(&after));
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
                    .document()
                    .get_transform(&id(2))
                    .unwrap()
                    .warp()
                    .unwrap()
                    .points
                    .clone();
                s.edit("Drive parent", None, |e| {
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
                .unwrap();
            }
            "locked" => lock(&mut s, &id(14)),
            _ => unreachable!(),
        }
        let before = s.document().clone();
        let version = s.version();
        let history = s.history_state();
        assert!(
            s.plan_scene_content(E::WarpKeepChildren {
                transform_id: id(2),
                warp
            })
            .is_err(),
            "{case}"
        );
        assert!(s.document().same_content(&before));
        assert_eq!(version, s.version());
        assert_eq!(history, s.history_state());
    }
    let mut s = fixture();
    let plan = s
        .plan_scene_content(E::WarpKeepChildren {
            transform_id: id(2),
            warp: next(&s),
        })
        .unwrap();
    s.edit("New version", None, |e| e.rename_mesh(&id(3), "new"))
        .unwrap();
    assert!(s.execute_scene_content_plan(&plan).is_err());
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
        let mut transform = s.document().get_transform(&id(2)).unwrap().clone();
        transform.warp_mut().unwrap().quad = quad;
        s.edit("Interpolator", None, |e| e.replace_transform(transform))
            .unwrap();
        let before = s.document().clone();
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
                for (a, b) in positions(&before, x, b)
                    .iter()
                    .zip(positions(s.document(), x, b))
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
                let mut transform = s.document().get_transform(&id(2)).unwrap().clone();
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
                s.edit("Canvas-sized Warp", None, |e| {
                    e.replace_transform(transform)
                })
                .unwrap();
                let before = s.document().clone();
                let history = s.history_state();
                let mut warp = before
                    .get_transform(&id(2))
                    .unwrap()
                    .warp()
                    .unwrap()
                    .clone();
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
                let plan = s
                    .plan_scene_content(E::WarpKeepChildren {
                        transform_id: id(2),
                        warp,
                    })
                    .unwrap_or_else(|e| panic!("{operation}, quad={quad}, offset={offset}: {e:?}"));
                assert!(plan.report().preservation_error.unwrap() <= 0.01);
                s.execute_scene_content_plan(&plan).unwrap();
                for x in [-1., -0.37, 0.9] {
                    for b in [0., 0.42, 1.] {
                        for (a, b) in
                            positions(&before, x, b)
                                .iter()
                                .zip(positions(s.document(), x, b))
                        {
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
                s.undo().unwrap();
                assert!(s.document().same_content(&before));
            }
        }
    }
}

#[test]
fn keep_children_still_rejects_actual_float32_position_loss_atomically() {
    let s = fixture();
    let before = s.document().clone();
    let history = s.history_state();
    let version = s.version();
    let mut warp = before
        .get_transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .clone();
    for p in &mut warp.points {
        p.x += 10_000_000.;
        p.y += 10_000_000.;
    }
    let error = s
        .plan_scene_content(E::WarpKeepChildren {
            transform_id: id(2),
            warp,
        })
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "WARP_PRESERVATION_TOLERANCE");
    assert!(s.document().same_content(&before));
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn preservation_noop_budget_and_cancellation_keep_history_and_missing_controller_intact() {
    let mut s = fixture();
    let old = s
        .document()
        .get_transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .clone();
    let version = s.version();
    let history = s.history_state();
    let plan = s
        .plan_scene_content(E::WarpKeepChildren {
            transform_id: id(2),
            warp: old,
        })
        .unwrap();
    assert!(plan.is_noop());
    s.execute_scene_content_plan(&plan).unwrap();
    assert_eq!(version, s.version());
    assert_eq!(history, s.history_state());
    let cancel = ConversionControl::default();
    cancel.cancel();
    assert!(
        s.plan_scene_content_with_control(
            E::WarpKeepChildren {
                transform_id: id(2),
                warp: next(&s)
            },
            &cancel
        )
        .is_err()
    );
    let desired = next(&s);
    s.set_authoring_limits(AuthoringLimits {
        max_conversion_bytes: 1,
        ..Default::default()
    })
    .unwrap();
    assert!(
        s.plan_scene_content(E::WarpKeepChildren {
            transform_id: id(2),
            warp: desired
        })
        .is_err()
    );
    assert_eq!(version, s.version());
    assert_eq!(history, s.history_state());
}
