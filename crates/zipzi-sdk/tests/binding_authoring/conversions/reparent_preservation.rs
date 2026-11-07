use super::*;
use zipzi_core::document::DeletionRequest;
use zipzi_core::{
    BindingAxisEditing, SceneBinding, SceneTrack, WarpBezier, WarpBezierEvaluation, WarpKeyform,
    WarpTransform,
};

use zipzi_core::document::DeletionTarget;

const BLEND_PARAMETER: &str = "00000000-0000-4000-8000-000000000430";
fn add_blend(s: &mut AuthoringSession, target: &str, warp: bool) {
    use zipzi_core::{
        BlendShapeBinding, BlendShapeKeyTable, BlendShapeTargetKind, DeltaKeyforms,
        DeltaMeshKeyform, DeltaWarpKeyform, ParameterKind,
    };
    let count = if warp {
        s.snapshot()
            .document()
            .transform(target)
            .unwrap()
            .warp()
            .unwrap()
            .points
            .len()
    } else {
        s.snapshot()
            .document()
            .mesh(target)
            .unwrap()
            .base_positions
            .len()
    };
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: BLEND_PARAMETER.into(),
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
                            id: "00000000-0000-4000-8000-000000000431".into(),
                            parameter_id: BLEND_PARAMETER.into(),
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
                {
                    let binding = BlendShapeBinding {
                        id: "00000000-0000-4000-8000-000000000432".into(),
                        target_id: target.into(),
                        target_kind: if warp {
                            BlendShapeTargetKind::Warp
                        } else {
                            BlendShapeTargetKind::Mesh
                        },
                        key_table_id: "00000000-0000-4000-8000-000000000431".into(),
                        constraint_ids: vec![],
                        keyforms: if warp {
                            DeltaKeyforms::Warp(
                                [0., 1.]
                                    .map(|v| DeltaWarpKeyform {
                                        points: vec![Vec2::new(0.01 * v, -0.02 * v); count],
                                        ..Default::default()
                                    })
                                    .into(),
                            )
                        } else {
                            DeltaKeyforms::Mesh(
                                [0., 1.]
                                    .map(|v| DeltaMeshKeyform {
                                        positions: vec![Vec2::new(0.01 * v, 0.02 * v); count],
                                        ..Default::default()
                                    })
                                    .into(),
                            )
                        },
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
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("BlendShape")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn blended_frame(s: &AuthoringSession, p: f32, blend: f32) -> zipzi_core::DrawableFrame {
    crate::support::frame(
        s.snapshot(),
        &[(PARAMETER_ID.into(), p), (BLEND_PARAMETER.into(), blend)].into(),
    )
    .unwrap()
}

fn assert_geometry(a: &zipzi_core::DrawableFrame, b: &zipzi_core::DrawableFrame) {
    for a in &a.drawables {
        let b = b.drawables.iter().find(|b| b.id == a.id).unwrap();
        for (a, b) in a.positions.iter().zip(&b.positions) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} != {b:?}");
        }
    }
}

fn regular_warp() -> WarpTransform {
    let (x0, x1, y0, y1) = (-467.0967f32, 478.4808f32, -229.98079f32, 588.63635f32);
    let points = (0..=5)
        .flat_map(|row| {
            (0..=5).map(move |col| {
                let (u, v) = (col as f32 / 5., row as f32 / 5.);
                Vec2::new(x0 * (1. - u) + x1 * u, y0 * (1. - v) + y1 * v)
            })
        })
        .collect();
    WarpTransform {
        rows: 5,
        columns: 5,
        quad: true,
        points,
        bezier: None,
    }
}

fn normalize_warp_child(s: &mut AuthoringSession) {
    let mut mesh = s.snapshot().document().mesh(MESH_ID).unwrap().clone();
    let mut binding = s
        .snapshot()
        .document()
        .binding_for_mesh(MESH_ID)
        .unwrap()
        .clone();
    for p in mesh
        .base_positions
        .iter_mut()
        .chain(binding.keyforms.iter_mut().flat_map(|f| &mut f.positions))
    {
        *p = Vec2::new(0.5 + p.x * 0.002, 0.5 + p.y * 0.002);
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_mesh(mesh)?;
                e.replace_binding(binding)
            })
            .and_then(|value| {
                let prepared = draft.prepare("normalized Warp coordinates")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}

fn rounded_warp_fixture(into_warp: bool, end: f32, quad: bool) -> AuthoringSession {
    let mut s = bound();
    let points = (0..=1)
        .flat_map(|row| {
            (0..=2).map(move |col| {
                Vec2::new(
                    col as f32 * 1000. + if col == 1 { 0.0018 } else { 0. },
                    row as f32,
                )
            })
        })
        .collect();
    let mut mesh = s.snapshot().document().mesh(MESH_ID).unwrap().clone();
    let mut binding = s
        .snapshot()
        .document()
        .binding_for_mesh(MESH_ID)
        .unwrap()
        .clone();
    let positions = |x| {
        (0..mesh.base_positions.len())
            .map(|i| {
                Vec2::new(
                    (x + (i % 2) as f32 * 0.1) * if into_warp { 2000. } else { 1. },
                    0.2 + (i / 2) as f32 * 0.1,
                )
            })
            .collect::<Vec<_>>()
    };
    binding.keyforms[0].positions = positions(0.1);
    binding.keyforms[1].positions = positions(end);
    mesh.base_positions = binding.keyforms[0].positions.clone();
    mesh.deformer_id = if into_warp { "" } else { R }.into();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: R.into(),
                        data: TransformData::Warp(WarpTransform {
                            rows: 1,
                            columns: 2,
                            quad,
                            points,
                            bezier: None,
                        }),
                        ..Default::default()
                    },
                )?;
                e.replace_mesh(mesh)?;
                e.replace_binding(binding)
            })
            .and_then(|value| {
                let prepared = draft.prepare("rounded affine lattice")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s.take_events();
    s
}

fn rounded_warp_edit(into_warp: bool) -> ReparentEdit {
    let E::Reparent(mut edit) = parent_edit(
        ReparentStrategy::PreserveParameterDomain,
        if into_warp { R } else { "" },
    ) else {
        unreachable!()
    };
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .insert(PARAMETER_ID.into(), 0.);
    edit
}

fn geometry_error(a: &zipzi_core::DrawableFrame, b: &zipzi_core::DrawableFrame) -> f64 {
    a.drawables
        .iter()
        .flat_map(|a| {
            let b = b.drawables.iter().find(|b| a.id == b.id).unwrap();
            a.positions.iter().zip(&b.positions).map(|(a, b)| {
                (f64::from(a.x) - f64::from(b.x)).hypot(f64::from(a.y) - f64::from(b.y))
            })
        })
        .fold(0., f64::max)
}

#[test]
fn rounded_warp_residual_covers_unseen_keyforms_and_interior_poses_in_both_directions() {
    for into_warp in [false, true] {
        for quad in [false, true] {
            // The first case reaches the lattice residual at the other keyform;
            // the second reaches it only inside the ordinary interpolation cell.
            for end in [0.5, 0.9] {
                let mut s = rounded_warp_fixture(into_warp, end, quad);
                let before: Vec<_> = (0..=16).map(|i| frame(&s, i as f32 / 16., 0.)).collect();
                let mut edit = rounded_warp_edit(into_warp);
                if end == 0.9 {
                    edit.tolerance = 0.0016;
                    edit.relative_tolerance = 0.;
                }
                let version = s.version();
                assert_eq!(
                    crate::support::binding(&s, E::Reparent(edit.clone()))
                        .unwrap_err()
                        .code
                        .as_str(),
                    "REPARENT_TOLERANCE_EXCEEDED"
                );
                assert_eq!(s.version(), version);
                assert!(s.take_events().is_empty());
                assert_eq!(geometry_error(&before[0], &frame(&s, 0., 0.)), 0.);
                edit.tolerance = 0.01;
                let plan = crate::support::binding(&s, E::Reparent(edit)).unwrap();
                let p = plan.0.report.reparent_guarantee.as_ref().unwrap();
                let bound = p.error_bound;
                s.publish(plan.1).unwrap();
                let maximum = before
                    .iter()
                    .enumerate()
                    .map(|(i, a)| geometry_error(a, &frame(&s, i as f32 / 16., 0.)))
                    .fold(0., f64::max);
                assert!(
                    maximum > 0.0016,
                    "fixture must expose the discarded residual: {maximum}"
                );
                assert!(maximum <= bound, "{maximum} > {bound}");
                s.undo(s.version()).unwrap();
                assert_eq!(geometry_error(&before[8], &frame(&s, 0.5, 0.)), 0.);
                s.redo(s.version()).unwrap();
                assert!(geometry_error(&before[8], &frame(&s, 0.5, 0.)) <= bound);
            }
        }
    }
}

#[test]
fn rounded_warp_residual_includes_active_blendshape_coordinate_envelopes() {
    let mut s = rounded_warp_fixture(false, 0.1, true);
    add_blend(&mut s, MESH_ID, false);
    let mut blend = s
        .snapshot()
        .document()
        .blend_binding("00000000-0000-4000-8000-000000000432")
        .unwrap()
        .clone();
    if let zipzi_core::DeltaKeyforms::Mesh(forms) = &mut blend.keyforms {
        forms[1].positions.fill(Vec2::new(0.4, 0.));
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_blend_binding(blend))
            .and_then(|value| {
                let prepared = draft.prepare("large blend delta")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before: Vec<_> = (0..=8)
        .map(|i| blended_frame(&s, 0., i as f32 / 8.))
        .collect();
    let mut edit = rounded_warp_edit(false);
    let context = edit.context.as_mut().unwrap();
    context.parameter_values.insert(BLEND_PARAMETER.into(), 0.);
    context
        .parameter_domain
        .push(zipzi_core::document::ConversionDomain {
            parameter_id: BLEND_PARAMETER.into(),
            minimum: 0.,
            maximum: 1.,
        });
    let version = s.version();
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "REPARENT_TOLERANCE_EXCEEDED"
    );
    assert_eq!(s.version(), version);
    edit.tolerance = 0.01;
    let plan = crate::support::binding(&s, E::Reparent(edit)).unwrap();
    let p = plan.0.report.reparent_guarantee.as_ref().unwrap();
    let bound = p.error_bound;
    s.publish(plan.1).unwrap();
    for (i, a) in before.iter().enumerate() {
        assert!(geometry_error(a, &blended_frame(&s, 0., i as f32 / 8.)) <= bound);
    }
    assert!(geometry_error(&before[8], &blended_frame(&s, 0., 1.)) > 0.0016);
}

#[test]
fn rounded_warp_subtree_residual_accounts_for_descendant_extrapolation() {
    let mut s = rounded_warp_fixture(false, 0.1, true);
    let mut binding = s
        .snapshot()
        .document()
        .binding_for_mesh(MESH_ID)
        .unwrap()
        .clone();
    for p in &mut binding.keyforms[1].positions {
        p.x += 8.;
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: T.into(),
                        parent_id: Some(R.into()),
                        data: TransformData::Warp(WarpTransform {
                            rows: 1,
                            columns: 1,
                            quad: true,
                            points: vec![
                                Vec2::new(0.1, 0.),
                                Vec2::new(0.5, 0.),
                                Vec2::new(0.1, 1.),
                                Vec2::new(0.5, 1.),
                            ],
                            bezier: None,
                        }),
                        ..Default::default()
                    },
                )?;
                e.edit_scene(
                    zipzi_core::document::SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            vec![(MESH_ID).to_string()],
                            (T).to_string(),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                e.replace_binding(binding)
            })
            .and_then(|value| {
                let prepared = draft.prepare("extrapolating subtree")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before: Vec<_> = (0..=16).map(|i| frame(&s, i as f32 / 16., 0.)).collect();
    let mut edit = rounded_warp_edit(false);
    edit.sources = vec![T.into()];
    // The fitted root lattice alone is below this tolerance; the unseen
    // descendant pose extrapolates its lattice error beyond the tolerance.
    edit.tolerance = 0.005;
    edit.relative_tolerance = 0.;
    let version = s.version();
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "REPARENT_TOLERANCE_EXCEEDED"
    );
    assert_eq!(s.version(), version);
    edit.tolerance = 0.1;
    let plan = crate::support::binding(&s, E::Reparent(edit)).unwrap();
    let p = plan.0.report.reparent_guarantee.as_ref().unwrap();
    let bound = p.error_bound;
    s.publish(plan.1).unwrap();
    let maximum = before
        .iter()
        .enumerate()
        .map(|(i, a)| geometry_error(a, &frame(&s, i as f32 / 16., 0.)))
        .fold(0., f64::max);
    assert!(maximum > 0.005, "{maximum}");
    assert!(maximum <= bound, "{maximum} > {bound}");
    s.undo(s.version()).unwrap();
    assert_eq!(geometry_error(&before[16], &frame(&s, 1., 0.)), 0.);
}

#[test]
fn rounded_warp_rotation_direction_requires_a_proof_for_domain_preservation() {
    let mut s = rounded_warp_fixture(false, 0.5, true);
    let mut parent = s.snapshot().document().transform(R).unwrap().clone();
    // Different residuals on the two rows change the local probe direction.
    parent.warp_mut().unwrap().points[4].x -= 0.0036;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_transform(parent)?;
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: T.into(),
                        parent_id: Some(R.into()),
                        data: TransformData::Rotation(RotationTransform {
                            base_angle: 0.,
                            pose: RotationPose {
                                origin: zipzi_core::PreciseVec2::new(0.1, 0.2),
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                )?;
                e.edit_scene(
                    zipzi_core::document::SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            vec![(MESH_ID).to_string()],
                            (T).to_string(),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("rotation child")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut edit = rounded_warp_edit(false);
    edit.sources = vec![T.into()];
    let version = s.version();
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "PARAMETER_DOMAIN_UNPROVEN"
    );
    assert_eq!(s.version(), version);
    let before = frame(&s, 0., 0.);
    edit.strategy = ReparentStrategy::PreserveEditState;
    edit.context.as_mut().unwrap().parameter_domain.clear();
    let plan = crate::support::binding(&s, E::Reparent(edit)).unwrap();
    s.publish(plan.1).unwrap();
    assert_geometry(&before, &frame(&s, 0., 0.));
}

#[test]
fn created_rectangular_warp_deletion_preserves_world_geometry_with_default_tolerance() {
    let mut s = bound();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: R.into(),
                        data: TransformData::Warp(regular_warp()),
                        ..Default::default()
                    },
                )?;
                e.edit_scene(
                    zipzi_core::document::SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            vec![(MESH_ID).to_string()],
                            (R).to_string(),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    normalize_warp_child(&mut s);
    let before: Vec<_> = (0..=8).map(|i| frame(&s, i as f32 / 8., 0.)).collect();
    let plan = crate::support::prepared(&s, |c| {
        c.edit_deletion(
            &DeletionRequest {
                roots: vec![DeletionTarget::RemoveDeformer(R.into())],
                ..Default::default()
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    for (i, a) in before.iter().enumerate() {
        assert_geometry(a, &frame(&s, i as f32 / 8., 0.));
    }
    s.undo(s.version()).unwrap();
    assert_geometry(&before[4], &frame(&s, 0.5, 0.));
    s.redo(s.version()).unwrap();
    assert_geometry(&before[4], &frame(&s, 0.5, 0.));
}

#[test]
fn translated_rotation_deletion_preserves_geometry_with_default_tolerance() {
    let mut s = bound();
    rotation(&mut s, 1.);
    let mut rotation = s.snapshot().document().transform(R).unwrap().clone();
    rotation.rotation_mut().unwrap().base_angle = 0.;
    rotation.rotation_mut().unwrap().pose = RotationPose {
        origin: zipzi_core::PreciseVec2::new(81.04255676269531, -333.5),
        ..Default::default()
    };
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_transform(rotation)?;
                e.edit_scene(
                    zipzi_core::document::SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            vec![(MESH_ID).to_string()],
                            (R).to_string(),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("large offset")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = frame(&s, 0.5, 0.);
    let plan = crate::support::prepared(&s, |c| {
        c.edit_deletion(
            &DeletionRequest {
                roots: vec![DeletionTarget::RemoveDeformer(R.into())],
                ..Default::default()
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    assert_geometry(&before, &frame(&s, 0.5, 0.));
}

#[test]
fn warp_subtree_reparent_and_parent_deletion_preserve_driven_children_and_rotation_scale() {
    for delete in [false, true] {
        let mut s = bound();
        rotation(&mut s, 2.);
        let mut warp = regular_warp();
        warp.bezier = Some(WarpBezier::from_lattice(
            warp.rows,
            warp.columns,
            true,
            &warp.points,
        ));
        let peer = "00000000-0000-4000-8000-000000000420";
        let mut mesh = s.snapshot().document().mesh(MESH_ID).unwrap().clone();
        mesh.id = peer.into();
        mesh.runtime_id = "rotation-child-mesh".into();
        mesh.deformer_id = Q.into();
        let controller = warp.bezier.clone();
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    crate::support::insert_transform(
                        e,
                        Transform {
                            id: T.into(),
                            parent_id: Some(R.into()),
                            data: TransformData::Warp(warp.clone()),
                            ..Default::default()
                        },
                    )?;
                    e.create_scene_binding(SceneBinding {
                        id: "00000000-0000-4000-8000-000000000421".into(),
                        axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
                        track: SceneTrack::Warp {
                            target_id: T.into(),
                            keyforms: [0., 1.]
                                .map(|key| WarpKeyform {
                                    keys: vec![key],
                                    positions: warp
                                        .points
                                        .iter()
                                        .map(|p| Vec2::new(p.x + key * 10., p.y - key * 4.))
                                        .collect(),
                                    bezier: controller.clone(),
                                    ..Default::default()
                                })
                                .into(),
                        },
                    })?;
                    crate::support::insert_transform(
                        e,
                        Transform {
                            id: Q.into(),
                            parent_id: Some(T.into()),
                            data: TransformData::Rotation(RotationTransform {
                                base_angle: 0.,
                                pose: RotationPose {
                                    origin: zipzi_core::PreciseVec2::new(0.4, 0.6),
                                    scale: 0.7,
                                    angle: 20.,
                                    ..Default::default()
                                },
                            }),
                            ..Default::default()
                        },
                    )?;
                    e.create_mesh(mesh)?;
                    e.edit_scene(
                        zipzi_core::document::SceneEdit::Reparent {
                            edit: ReparentEdit::keep_local(
                                vec![(MESH_ID).to_string()],
                                (T).to_string(),
                            ),
                        },
                        Default::default(),
                        &Default::default(),
                    )
                    .map(|_| ())
                })
                .and_then(|value| {
                    let prepared = draft.prepare("subtree")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        normalize_warp_child(&mut s);
        let before: Vec<_> = (0..=8).map(|i| frame(&s, i as f32 / 8., 0.)).collect();
        if delete {
            let plan = crate::support::prepared(&s, |c| {
                c.edit_deletion(
                    &DeletionRequest {
                        roots: vec![DeletionTarget::RemoveDeformer(R.into())],
                        ..Default::default()
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .unwrap();
            s.publish(plan.1).unwrap();
        } else {
            let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveParameterDomain, "")
            else {
                unreachable!()
            };
            edit.sources = vec![T.into()];
            apply(&mut s, E::Reparent(edit));
        }
        for (i, a) in before.iter().enumerate() {
            assert_geometry(a, &frame(&s, i as f32 / 8., 0.));
        }
        assert_eq!(
            s.snapshot()
                .document()
                .transform(Q)
                .unwrap()
                .rotation()
                .unwrap()
                .pose
                .scale,
            1.4
        );
        assert!(s
            .snapshot()
            .document()
            .transform(T)
            .unwrap()
            .warp()
            .unwrap()
            .bezier
            .is_some());
        s.undo(s.version()).unwrap();
        assert_geometry(&before[4], &frame(&s, 0.5, 0.));
        s.redo(s.version()).unwrap();
        assert_geometry(&before[4], &frame(&s, 0.5, 0.));
        grid_roundtrip(&s);
    }
}

#[test]
fn nonlinear_mesh_baking_requires_explicit_edit_state_and_retains_binding_identity() {
    let mut s = bound();
    let mut warp = regular_warp();
    warp.points[14].x += 120.;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: R.into(),
                        data: TransformData::Warp(warp),
                        ..Default::default()
                    },
                )?;
                e.edit_scene(
                    zipzi_core::document::SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            vec![(MESH_ID).to_string()],
                            (R).to_string(),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("nonlinear parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    normalize_warp_child(&mut s);
    let before = frame(&s, 0.5, 0.);
    assert!(crate::support::binding(
        &s,
        parent_edit(ReparentStrategy::PreserveParameterDomain, "")
    )
    .is_err());
    let plan =
        crate::support::binding(&s, parent_edit(ReparentStrategy::PreserveEditState, "")).unwrap();
    let p = plan.0.report.reparent_guarantee.as_ref().unwrap();
    assert!(p.effects.iter().any(|e| e.contains("Baked nonlinear")));
    s.publish(plan.1).unwrap();
    assert_geometry(&before, &frame(&s, 0.5, 0.));
    assert_eq!(
        s.snapshot()
            .document()
            .binding_for_mesh(MESH_ID)
            .unwrap()
            .id,
        BINDING_ID
    );
    s.undo(s.version()).unwrap();
    assert_geometry(&before, &frame(&s, 0.5, 0.));
    s.redo(s.version()).unwrap();
    assert_geometry(&before, &frame(&s, 0.5, 0.));
}

#[test]
fn nonlinear_subtrees_bake_current_pose_without_flattening_on_delete_and_reparent() {
    for warp_child in [false, true] {
        for delete in [false, true] {
            let mut s = bound();
            let mut outer = regular_warp();
            outer.points[14].x += 120.;
            let points: Vec<_> = (0..=2)
                .flat_map(|y| {
                    (0..=2).map(move |x| Vec2::new(0.2 + x as f32 * 0.3, 0.2 + y as f32 * 0.3))
                })
                .collect();
            let data = if warp_child {
                TransformData::Warp(WarpTransform {
                    rows: 2,
                    columns: 2,
                    quad: true,
                    bezier: Some(WarpBezier::from_lattice(2, 2, true, &points)),
                    points: points.clone(),
                })
            } else {
                TransformData::Rotation(RotationTransform {
                    base_angle: 10.,
                    pose: RotationPose {
                        origin: zipzi_core::PreciseVec2::new(0.4, 0.6),
                        scale: 0.7,
                        angle: 20.,
                        ..Default::default()
                    },
                })
            };
            let binding_id = "00000000-0000-4000-8000-000000000422";
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| {
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: Q.into(),
                                data: TransformData::Rotation(RotationTransform {
                                    base_angle: 0.,
                                    pose: RotationPose {
                                        scale: 2.,
                                        ..Default::default()
                                    },
                                }),
                                ..Default::default()
                            },
                        )?;
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: R.into(),
                                parent_id: Some(Q.into()),
                                data: TransformData::Warp(outer),
                                ..Default::default()
                            },
                        )?;
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: T.into(),
                                parent_id: Some(R.into()),
                                data: data.clone(),
                                ..Default::default()
                            },
                        )?;
                        let track = if warp_child {
                            SceneTrack::Warp {
                                target_id: T.into(),
                                keyforms: [0., 1.]
                                    .map(|key| WarpKeyform {
                                        keys: vec![key],
                                        positions: points
                                            .iter()
                                            .map(|p| Vec2::new(p.x + key * 0.04, p.y))
                                            .collect(),
                                        ..Default::default()
                                    })
                                    .into(),
                            }
                        } else {
                            SceneTrack::Rotation {
                                target_id: T.into(),
                                keyforms: [0., 1.]
                                    .map(|key| zipzi_core::RotationKeyform {
                                        keys: vec![key],
                                        rotation: RotationPose {
                                            origin: zipzi_core::PreciseVec2::new(
                                                0.4 + f64::from(key) * 0.1,
                                                0.6,
                                            ),
                                            angle: 20. + key * 5.,
                                            scale: 0.7 + key * 0.1,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .into(),
                            }
                        };
                        e.create_scene_binding(SceneBinding {
                            id: binding_id.into(),
                            axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
                            track,
                        })?;
                        e.edit_scene(
                            zipzi_core::document::SceneEdit::Reparent {
                                edit: ReparentEdit::keep_local(
                                    vec![(MESH_ID).to_string()],
                                    (T).to_string(),
                                ),
                            },
                            Default::default(),
                            &Default::default(),
                        )
                        .map(|_| ())
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("nonlinear subtree")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
            if warp_child {
                normalize_warp_child(&mut s);
            }
            let before = frame(&s, 0.5, 0.);
            let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveEditState, "") else {
                unreachable!()
            };
            edit.sources = vec![T.into()];
            if delete {
                let mut request = DeletionRequest {
                    roots: vec![DeletionTarget::RemoveDeformer(R.into())],
                    conversion_context: edit.context.clone(),
                    ..Default::default()
                };
                let report = {
                    let mut d = s.begin_draft();
                    d.batch(d.source(), |c| {
                        c.review_deletion(&request, Default::default(), &Default::default())
                    })
                    .map(|o| o.report)
                }
                .unwrap();
                assert!(report.decisions.iter().any(|d| {
                    d.choices
                        .contains(&zipzi_core::document::DeletionChoice::PreserveEditState)
                }));
                request
                    .reparent
                    .insert(T.into(), ReparentStrategy::PreserveEditState);
                let plan = crate::support::prepared(&s, |c| {
                    c.edit_deletion(&request, Default::default(), &Default::default())
                })
                .unwrap();
                s.publish(plan.1).unwrap();
            } else {
                apply(&mut s, E::Reparent(edit));
            }
            assert_geometry(&before, &frame(&s, 0.5, 0.));
            assert_eq!(
                s.snapshot().document().binding_for_scene(T).unwrap().id,
                binding_id
            );
            assert_eq!(
                s.snapshot().document().mesh(MESH_ID).unwrap().deformer_id,
                T
            );
            s.undo(s.version()).unwrap();
            assert_geometry(&before, &frame(&s, 0.5, 0.));
            s.redo(s.version()).unwrap();
            assert_geometry(&before, &frame(&s, 0.5, 0.));
        }
    }
}

#[test]
fn ordinary_interpolation_and_active_blendshapes_survive_affine_conversion_and_current_baking() {
    for (warp_child, nonlinear) in [(true, false), (true, true), (false, true)] {
        let mut s = bound();
        let mut outer = regular_warp();
        if nonlinear {
            outer.points[14].x += 120.;
        }
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    crate::support::insert_transform(
                        e,
                        Transform {
                            id: R.into(),
                            data: TransformData::Warp(outer),
                            ..Default::default()
                        },
                    )?;
                    if warp_child {
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: T.into(),
                                parent_id: Some(R.into()),
                                data: TransformData::Warp(WarpTransform {
                                    rows: 1,
                                    columns: 1,
                                    quad: true,
                                    points: vec![
                                        Vec2::new(0.2, 0.2),
                                        Vec2::new(0.8, 0.2),
                                        Vec2::new(0.2, 0.8),
                                        Vec2::new(0.8, 0.8),
                                    ],
                                    bezier: None,
                                }),
                                ..Default::default()
                            },
                        )?;
                    }
                    e.edit_scene(
                        zipzi_core::document::SceneEdit::Reparent {
                            edit: ReparentEdit::keep_local(
                                vec![(MESH_ID).to_string()],
                                (if warp_child { T } else { R }).to_string(),
                            ),
                        },
                        Default::default(),
                        &Default::default(),
                    )
                    .map(|_| ())
                })
                .and_then(|value| {
                    let prepared = draft.prepare("parent and child")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        normalize_warp_child(&mut s);
        let source = if warp_child { T } else { MESH_ID };
        add_blend(&mut s, source, warp_child);
        let before = blended_frame(&s, 0.5, 0.35);
        let domain_frames: Vec<_> = (0..=16)
            .map(|i| blended_frame(&s, i as f32 / 16., 1. - i as f32 / 16.))
            .collect();
        let strategy = if nonlinear {
            ReparentStrategy::PreserveEditState
        } else {
            ReparentStrategy::PreserveParameterDomain
        };
        let E::Reparent(mut edit) = parent_edit(strategy, "") else {
            unreachable!()
        };
        edit.sources = vec![source.into()];
        let context = edit.context.as_mut().unwrap();
        context
            .parameter_values
            .insert(BLEND_PARAMETER.into(), 0.35);
        if !nonlinear {
            context
                .parameter_domain
                .push(zipzi_core::document::ConversionDomain {
                    parameter_id: BLEND_PARAMETER.into(),
                    minimum: 0.,
                    maximum: 1.,
                });
        }
        apply(&mut s, E::Reparent(edit));
        assert_geometry(&before, &blended_frame(&s, 0.5, 0.35));
        if !nonlinear {
            for (i, before) in domain_frames.iter().enumerate() {
                assert_geometry(
                    before,
                    &blended_frame(&s, i as f32 / 16., 1. - i as f32 / 16.),
                );
            }
        }
        assert_eq!(
            s.snapshot()
                .document()
                .blend_binding("00000000-0000-4000-8000-000000000432")
                .unwrap()
                .target_id,
            source
        );
        s.undo(s.version()).unwrap();
        assert_geometry(&before, &blended_frame(&s, 0.5, 0.35));
        s.redo(s.version()).unwrap();
        assert_geometry(&before, &blended_frame(&s, 0.5, 0.35));
    }
}
