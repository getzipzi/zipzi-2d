use zipzi_core::document::SceneCollection;
use zipzi_core::document::SceneCreate;
use zipzi_core::document::SceneEdit;
use zipzi_core::{
    BindingAxis, BlendShapeBinding, BlendShapeTargetKind, DeltaKeyforms, DeltaRotationKeyform,
    DeltaWarpKeyform, DrawableFrame, ModelSetup, RotationKeyform, RotationPose, SceneBinding,
    SceneTrack, Transform, TransformData, Vec2, WarpKeyform, WarpTransform,
};
use zipzi_sdk::{AuthoringSession, ReparentContext, ReparentEdit, ReparentStrategy};
#[path = "../../../fixtures/mesh-edit-dependencies.rs"]
mod fixture;
fn setup() -> AuthoringSession {
    let mut s = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(&fixture::id(1), (ModelSetup::default()).into())
                .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), fixture::install)
            .and_then(|value| {
                let prepared = draft.prepare("Fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}
fn creation(id: u32, warp: bool) -> SceneEdit {
    SceneEdit::Create {
        id: fixture::id(id),
        name: "Group".into(),
        object: if warp {
            SceneCreate::Warp { bounds: None }
        } else {
            SceneCreate::Rotation { origin: None }
        },
        part_id: None,
        parent_id: None,
        before: None,
        deformation_before: None,
    }
}
fn samples(s: &mut AuthoringSession) -> Vec<DrawableFrame> {
    [0., 0.3, 1.]
        .into_iter()
        .map(|v| {
            let values = [
                (fixture::id(5), v),
                (fixture::id(10), v),
                (fixture::id(20), 1. - v),
            ]
            .into();
            crate::support::frame(s.snapshot(), &values).unwrap()
        })
        .collect()
}
fn same(before: &[DrawableFrame], after: &[DrawableFrame]) {
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().zip(after) {
        assert_eq!(a.drawables.len(), b.drawables.len());
        for a in &a.drawables {
            let b = b.drawables.iter().find(|b| b.id == a.id).unwrap();
            assert_eq!(a.opacity, b.opacity);
            assert_eq!(a.visible, b.visible);
            assert_eq!(a.uvs, b.uvs);
            assert_eq!(a.positions.len(), b.positions.len());
            for (x, y) in a.positions.iter().zip(&b.positions) {
                assert!(
                    (x.x - y.x).hypot(x.y - y.y) < 0.001,
                    "{}: {x:?} != {y:?}",
                    a.id
                );
            }
        }
    }
}

#[test]
fn rotation_group_centers_default_pose_content_through_rotation_and_warp_parents() {
    for (source, warp_parent) in [(4, false), (3, false), (2, false), (2, true)] {
        let mut s = setup();
        if warp_parent {
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| {
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: fixture::id(500),
                                parent_id: Some(fixture::id(3).into()),
                                data: TransformData::Warp(WarpTransform {
                                    rows: 1,
                                    columns: 1,
                                    quad: false,
                                    bezier: None,
                                    points: vec![
                                        Vec2::new(10., 20.),
                                        Vec2::new(20., 20.),
                                        Vec2::new(10., 40.),
                                        Vec2::new(20., 40.),
                                    ],
                                }),
                                ..Default::default()
                            },
                        )?;
                        e.edit_scene(
                            SceneEdit::Reparent {
                                edit: ReparentEdit::keep_local(
                                    vec![fixture::id(2)],
                                    fixture::id(500),
                                ),
                            },
                            Default::default(),
                            &Default::default(),
                        )?;
                        Ok(())
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("Nested rectangular parent")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
        }
        let before = samples(&mut s);
        let values = Default::default();
        let current = crate::support::frame(s.snapshot(), &values).unwrap();
        let mesh = current
            .drawables
            .iter()
            .find(|m| m.id == fixture::id(if source == 4 { 4 } else { 2 }))
            .unwrap();
        let bounds = mesh.positions.iter().fold(
            [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            |[a, b, c, d], p| [a.min(p.x), b.min(p.y), c.max(p.x), d.max(p.y)],
        );
        let center = Vec2::new((bounds[0] + bounds[2]) / 2., (bounds[1] + bounds[3]) / 2.);
        let history = {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        };
        let plan = crate::support::prepared(&s, |c| {
            c.group_deformer(
                creation(400, false),
                vec![fixture::id(source)],
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap();
        s.publish(plan.1).unwrap();
        let created = crate::support::frame(s.snapshot(), &values).unwrap();
        let origin = created
            .deformers
            .iter()
            .find(|d| d.id == fixture::id(400))
            .unwrap()
            .points[0];
        assert!(
            (origin.x - center.x).hypot(origin.y - center.y) < 0.001,
            "source={source}, warp={warp_parent}: {origin:?} vs {center:?}"
        );
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            (history.0 + 1, 0)
        );
        same(&before, &samples(&mut s));
        s.undo(s.version()).unwrap();
        same(&before, &samples(&mut s));
        s.redo(s.version()).unwrap();
        same(&before, &samples(&mut s));
    }
}

#[test]
fn grouping_converts_ordinary_and_blend_channels_of_selected_warp_and_rotation() {
    for warp in [false, true] {
        let mut s = setup();
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_scene(creation(500, true), Default::default(), &Default::default())?;
                    let model = e.read_document().unwrap();
                    let grid = model.transform(&fixture::id(500)).unwrap().warp().unwrap();
                    let (rows, columns, point_count) = (grid.rows, grid.columns, grid.points.len());
                    e.edit_scene(
                        SceneEdit::Reparent {
                            edit: ReparentEdit::keep_local(vec![fixture::id(4)], fixture::id(500)),
                        },
                        Default::default(),
                        &Default::default(),
                    )?;
                    e.create_scene_binding(SceneBinding {
                        id: fixture::id(501),
                        axes: vec![BindingAxis {
                            parameter_id: fixture::id(5),
                            keys: vec![0., 1.],
                            ..Default::default()
                        }],
                        track: SceneTrack::Warp {
                            target_id: fixture::id(500).into(),
                            keyforms: [0., 1.]
                                .into_iter()
                                .map(|v| WarpKeyform {
                                    keys: vec![v],
                                    positions: (0..=rows)
                                        .flat_map(|row| {
                                            (0..=columns).map(move |column| {
                                                let x = column as f32 / columns as f32;
                                                let y = row as f32 / rows as f32;
                                                Vec2::new(
                                                    (10. * (1. - x) + (50. + v) * x) * (1. - y)
                                                        + (9. * (1. - x) + 48. * x) * y,
                                                    ((20. + v) * (1. - x) + 21. * x) * (1. - y)
                                                        + (60. * (1. - x) + (63. + v) * x) * y,
                                                )
                                            })
                                        })
                                        .collect(),
                                    ..Default::default()
                                })
                                .collect(),
                        },
                    })?;
                    e.create_scene_binding(SceneBinding {
                        id: fixture::id(502),
                        axes: vec![BindingAxis {
                            parameter_id: fixture::id(5),
                            keys: vec![0., 1.],
                            ..Default::default()
                        }],
                        track: SceneTrack::Rotation {
                            target_id: fixture::id(3).into(),
                            keyforms: [0., 1.]
                                .into_iter()
                                .map(|v| RotationKeyform {
                                    keys: vec![v],
                                    rotation: RotationPose {
                                        origin: Vec2::new(13. + v, 9. - v).into(),
                                        angle: 21. + v * 20.,
                                        scale: 1.2 + v * 0.2,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                })
                                .collect(),
                        },
                    })?;
                    for (id, target, kind, keyforms) in [
                        (
                            503,
                            500,
                            BlendShapeTargetKind::Warp,
                            DeltaKeyforms::Warp(vec![
                                DeltaWarpKeyform {
                                    points: vec![Vec2::default(); point_count],
                                    ..Default::default()
                                },
                                DeltaWarpKeyform {
                                    points: vec![Vec2::new(2., -3.); point_count],
                                    ..Default::default()
                                },
                            ]),
                        ),
                        (
                            504,
                            3,
                            BlendShapeTargetKind::Rotation,
                            DeltaKeyforms::Rotation(vec![
                                DeltaRotationKeyform::default(),
                                DeltaRotationKeyform {
                                    origin: Some(Vec2::new(-2., 3.)),
                                    angle: Some(7.),
                                    scale: Some(0.1),
                                    ..Default::default()
                                },
                            ]),
                        ),
                    ] {
                        {
                            let binding = BlendShapeBinding {
                                id: fixture::id(id),
                                target_id: fixture::id(target),
                                target_kind: kind,
                                key_table_id: fixture::id(11),
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
                    let prepared = draft.prepare("Driven deformers")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let before = samples(&mut s);
        let plan = crate::support::prepared(&s, |c| {
            c.group_deformer(
                creation(400, warp),
                [500, 3].map(fixture::id).into(),
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap();
        s.publish(plan.1).unwrap();
        same(&before, &samples(&mut s));
    }
}
#[test]
fn grouping_preserves_parameter_shapes_blends_nested_selection_and_one_history_step() {
    for warp in [false, true] {
        let mut s = setup();
        let original = s.snapshot().document().clone();
        let history = {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        };
        let before = samples(&mut s);
        let plan = crate::support::prepared(&s, |c| {
            c.group_deformer(
                creation(400, warp),
                [2, 3, 4].map(fixture::id).into(),
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap();
        let stale = s.read_snapshot();
        s.publish(plan.1).unwrap();
        assert_eq!(
            s.snapshot()
                .document()
                .mesh(&fixture::id(2))
                .unwrap()
                .deformer_id,
            fixture::id(3)
        );
        assert_eq!(
            s.snapshot()
                .document()
                .transform(&fixture::id(3))
                .unwrap()
                .parent(),
            fixture::id(400)
        );
        assert_eq!(
            s.snapshot()
                .document()
                .mesh(&fixture::id(4))
                .unwrap()
                .deformer_id,
            fixture::id(400)
        );
        same(&before, &samples(&mut s));
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            (history.0 + 1, 0)
        );
        let after = s.snapshot().document().clone();
        s.undo(s.version()).unwrap();
        assert_eq!(
            s.snapshot().document().mesh(&fixture::id(4)),
            original.mesh(&fixture::id(4))
        );
        assert_eq!(
            s.snapshot().document().transform(&fixture::id(3)),
            original.transform(&fixture::id(3))
        );
        s.redo(s.version()).unwrap();
        assert_eq!(
            s.snapshot().document().binding(&fixture::id(6)),
            after.binding(&fixture::id(6))
        );
        assert_ne!(s.read_snapshot().source, stale.source);
    }
}
#[test]
fn grouping_warp_under_driven_non_affine_warp_preserves_the_existing_lattice() {
    let mut s = setup();
    let points = (0..3)
        .flat_map(|y| {
            (0..3).map(move |x| {
                Vec2::new(
                    x as f32 * 30.,
                    y as f32 * 20. + if x == 1 { 8. } else { 0. },
                )
            })
        })
        .collect::<Vec<_>>();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_transform(
                    edit,
                    Transform {
                        id: fixture::id(500),
                        data: TransformData::Warp(WarpTransform {
                            rows: 2,
                            columns: 2,
                            quad: false,
                            bezier: None,
                            points: points.clone(),
                        }),
                        ..Default::default()
                    },
                )?;
                edit.create_scene_binding(SceneBinding {
                    id: fixture::id(501),
                    axes: vec![BindingAxis {
                        parameter_id: fixture::id(5),
                        keys: vec![0., 1.],
                        ..Default::default()
                    }],
                    track: SceneTrack::Warp {
                        target_id: fixture::id(500).into(),
                        keyforms: vec![
                            WarpKeyform {
                                keys: vec![0.],
                                positions: points.clone(),
                                ..Default::default()
                            },
                            WarpKeyform {
                                keys: vec![1.],
                                positions: points
                                    .iter()
                                    .map(|p| Vec2::new(p.x + 2., p.y - 1.))
                                    .collect(),
                                ..Default::default()
                            },
                        ],
                    },
                })?;
                edit.edit_scene(
                    SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            [3, 4].map(fixture::id).into(),
                            fixture::id(500),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = samples(&mut s);
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    assert!(crate::support::prepared(&s, |c| c.group_deformer(
        creation(400, false),
        [3, 4].map(fixture::id).into(),
        Default::default(),
        &Default::default()
    ))
    .is_err());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    assert!(s
        .snapshot()
        .document()
        .transform(&fixture::id(400))
        .is_none());
    let plan = crate::support::prepared(&s, |c| {
        c.group_deformer(
            creation(400, true),
            [3, 4].map(fixture::id).into(),
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    let w = s
        .snapshot()
        .document()
        .transform(&fixture::id(400))
        .unwrap()
        .warp()
        .unwrap();
    assert_eq!((w.rows, w.columns), (2, 2));
    same(&before, &samples(&mut s));
    assert_eq!(
        s.snapshot()
            .document()
            .scene_collection(&SceneCollection::Deformation {
                parent_id: Some(fixture::id(400))
            })
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn grouping_rotation_under_static_affine_warp_preserves_child_coordinates() {
    let mut s = setup();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.edit_scene(creation(500, true), Default::default(), &Default::default())?;
                let mut parent = edit
                    .read_document()
                    .unwrap()
                    .transform(&fixture::id(500))
                    .unwrap()
                    .clone();
                for p in &mut parent.warp_mut().unwrap().points {
                    *p = Vec2::new(10. + p.x, 20. + p.y * 2.);
                }
                edit.replace_transform(parent)?;
                edit.edit_scene(
                    SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            [3, 4].map(fixture::id).into(),
                            fixture::id(500),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = samples(&mut s);
    let plan = crate::support::prepared(&s, |c| {
        c.group_deformer(
            creation(400, false),
            [3, 4].map(fixture::id).into(),
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    same(&before, &samples(&mut s));
}

fn preserving_edit(sources: &[u32], parent: u32) -> ReparentEdit {
    ReparentEdit {
        strategy: ReparentStrategy::PreserveParameterDomain,
        tolerance: 0.01,
        context: Some(ReparentContext {
            parameter_values: [5, 10, 20].map(|id| (fixture::id(id), 0.3)).into(),
            parameter_domain: [5, 10, 20]
                .map(|id| zipzi_core::document::ConversionDomain {
                    parameter_id: fixture::id(id),
                    minimum: 0.,
                    maximum: 1.,
                })
                .into(),
            ..Default::default()
        }),
        ..ReparentEdit::keep_local(
            sources.iter().copied().map(fixture::id).collect(),
            if parent == 0 {
                String::new()
            } else {
                fixture::id(parent)
            },
        )
    }
}
fn preserving_move(s: &mut AuthoringSession, sources: &[u32], parent: u32) {
    let edit = preserving_edit(sources, parent);
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Reparent { edit },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
}

#[test]
fn non_affine_and_collapsed_warp_moves_reject_without_changing_meshes_or_redo() {
    for collapsed in [false, true] {
        let mut s = setup();
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_scene(creation(500, true), Default::default(), &Default::default())?;
                    let mut warp = e
                        .read_document()
                        .unwrap()
                        .transform(&fixture::id(500))
                        .unwrap()
                        .clone();
                    let points = &mut warp.warp_mut().unwrap().points;
                    if collapsed {
                        for p in points {
                            p.x = 0.;
                        }
                    } else {
                        points[3].x += 5.;
                    }
                    e.replace_transform(warp)
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Warp destination")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_scene(
                        creation(501, false),
                        Default::default(),
                        &Default::default(),
                    )
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Redo sentinel")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        s.undo(s.version()).unwrap();
        let history = {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        };
        let version = s.version();
        let before = samples(&mut s);
        let error = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_scene(
                SceneEdit::Reparent {
                    edit: preserving_edit(&[2, 4], 500),
                },
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap_err();
        assert_eq!(
            error.code.as_str(),
            if collapsed {
                "SINGULAR_PARENT"
            } else {
                "NONLINEAR_PARENT_UNPROVEN"
            }
        );
        assert_eq!(
            {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            },
            history
        );
        assert_eq!(s.version(), version);
        same(&before, &samples(&mut s));
        s.redo(s.version()).unwrap();
    }
}

#[test]
fn mesh_moves_into_and_out_of_rectangular_warp_preserve_ordinary_and_blend_shapes() {
    for quad in [false, true] {
        for nested in [false, true] {
            let mut s = setup();
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| {
                        let mut rotation = e
                            .read_document()
                            .unwrap()
                            .transform(&fixture::id(3))
                            .unwrap()
                            .clone();
                        rotation.rotation_mut().unwrap().pose.reflect_x = nested;
                        e.replace_transform(rotation)?;
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: fixture::id(500),
                                parent_id: nested.then(|| fixture::id(3).into()),
                                data: TransformData::Warp(WarpTransform {
                                    rows: 2,
                                    columns: 3,
                                    quad,
                                    bezier: None,
                                    points: (0..=2)
                                        .flat_map(|y| {
                                            (0..=3).map(move |x| {
                                                Vec2::new(
                                                    -30. + 120. * (x as f32 / 3.),
                                                    -40. + 240. * (y as f32 / 2.),
                                                )
                                            })
                                        })
                                        .collect(),
                                }),
                                ..Default::default()
                            },
                        )?;
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: fixture::id(600),
                                parent_id: Some(fixture::id(500).into()),
                                data: TransformData::Rotation(zipzi_core::RotationTransform {
                                    pose: RotationPose {
                                        origin: Vec2::new(0.2, 0.1).into(),
                                        angle: 12.,
                                        scale: 0.8,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                        )
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("Warp destination")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
            let before = samples(&mut s);
            let original = s
                .snapshot()
                .document()
                .mesh(&fixture::id(2))
                .unwrap()
                .clone();
            let history = {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            };
            preserving_move(&mut s, &[2, 4], 500);
            same(&before, &samples(&mut s));
            assert_eq!(
                {
                    let history = s.history_state();
                    (history.undo_steps, history.redo_steps)
                },
                (history.0 + 1, 0)
            );
            assert_eq!(
                s.snapshot()
                    .document()
                    .mesh(&fixture::id(2))
                    .unwrap()
                    .deformer_id,
                fixture::id(500)
            );
            s.undo(s.version()).unwrap();
            assert_eq!(
                s.snapshot().document().mesh(&fixture::id(2)).unwrap(),
                &original
            );
            s.redo(s.version()).unwrap();
            same(&before, &samples(&mut s));
            preserving_move(&mut s, &[2], 600);
            same(&before, &samples(&mut s));
            preserving_move(&mut s, &[2, 4], 0);
            same(&before, &samples(&mut s));
        }
    }
}

#[test]
fn mesh_moves_between_nested_rotations_preserve_shapes_with_scale_pivots_and_reflections() {
    for reflect_x in [false, true] {
        for reflect_y in [false, true] {
            for nested in [false, true] {
                let mut s = setup();
                {
                    let mut draft = s.begin_draft();
                    draft
                        .batch(draft.source(), |e| {
                            for (id, parent, base, angle, scale, origin, rx, ry) in [
                                (
                                    550,
                                    None,
                                    -12.,
                                    93.,
                                    0.8,
                                    Vec2::new(-17., 31.),
                                    reflect_y,
                                    reflect_x,
                                ),
                                (
                                    500,
                                    nested.then_some(550),
                                    31.,
                                    -133.,
                                    1.75,
                                    Vec2::new(23., -19.),
                                    reflect_x,
                                    reflect_y,
                                ),
                                (
                                    600,
                                    None,
                                    17.,
                                    123.,
                                    0.35,
                                    Vec2::new(-41., 27.),
                                    reflect_y,
                                    reflect_x,
                                ),
                            ] {
                                crate::support::insert_transform(
                                    e,
                                    Transform {
                                        id: fixture::id(id),
                                        parent_id: parent.map(|id| fixture::id(id).into()),
                                        data: TransformData::Rotation(
                                            zipzi_core::RotationTransform {
                                                base_angle: base,
                                                pose: RotationPose {
                                                    origin: origin.into(),
                                                    angle,
                                                    scale,
                                                    reflect_x: rx,
                                                    reflect_y: ry,
                                                    ..Default::default()
                                                },
                                            },
                                        ),
                                        ..Default::default()
                                    },
                                )?;
                            }
                            Ok(())
                        })
                        .and_then(|value| {
                            let prepared = draft.prepare("Rotation destinations")?;
                            s.publish(prepared).map(|receipt| (value, receipt))
                        })
                }
                .unwrap();
                let before = samples(&mut s);
                let mesh = s
                    .snapshot()
                    .document()
                    .mesh(&fixture::id(2))
                    .unwrap()
                    .clone();
                let binding = s
                    .snapshot()
                    .document()
                    .binding(&fixture::id(6))
                    .unwrap()
                    .clone();
                let history = {
                    let history = s.history_state();
                    (history.undo_steps, history.redo_steps)
                };
                for parent in [500, 600, 0] {
                    preserving_move(&mut s, &[2, 4], parent);
                    same(&before, &samples(&mut s));
                }
                assert_eq!(
                    {
                        let history = s.history_state();
                        (history.undo_steps, history.redo_steps)
                    },
                    (history.0 + 3, 0)
                );
                for _ in 0..3 {
                    s.undo(s.version()).unwrap();
                }
                assert_eq!(
                    s.snapshot().document().mesh(&fixture::id(2)).unwrap(),
                    &mesh
                );
                assert_eq!(
                    s.snapshot().document().binding(&fixture::id(6)).unwrap(),
                    &binding
                );
                s.redo(s.version()).unwrap();
                same(&before, &samples(&mut s));
            }
        }
    }
}

#[test]
fn whole_rotation_reparent_preserves_animated_subtree_with_nested_warp_and_rotation() {
    for reflect in [false, true] {
        for clamped_angle in [false, true] {
            let mut s = setup();
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| {
                        for (id, parent, scale, angle, origin) in [
                            (550, None, 0.8, -25., Vec2::new(2., 3.)),
                            (500, None, 1.75, -133., Vec2::new(23., -19.)),
                            (600, Some(500), 0.7, 43., Vec2::new(-11., 8.)),
                        ] {
                            crate::support::insert_transform(
                                e,
                                Transform {
                                    id: fixture::id(id),
                                    parent_id: parent.map(|id| fixture::id(id).into()),
                                    data: TransformData::Rotation(zipzi_core::RotationTransform {
                                        base_angle: 13.,
                                        pose: RotationPose {
                                            origin: origin.into(),
                                            angle,
                                            scale,
                                            reflect_x: reflect,
                                            reflect_y: if id == 550 {
                                                reflect
                                            } else {
                                                clamped_angle
                                            },
                                            ..Default::default()
                                        },
                                    }),
                                    ..Default::default()
                                },
                            )?;
                        }
                        e.edit_scene(creation(700, true), Default::default(), &Default::default())?;
                        e.edit_scene(
                            SceneEdit::Reparent {
                                edit: ReparentEdit::keep_local(
                                    vec![fixture::id(3)],
                                    fixture::id(550),
                                ),
                            },
                            Default::default(),
                            &Default::default(),
                        )?;
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: fixture::id(520),
                                parent_id: Some(fixture::id(3).into()),
                                data: TransformData::Warp(WarpTransform {
                                    rows: 1,
                                    columns: 1,
                                    quad: true,
                                    bezier: None,
                                    points: vec![
                                        Vec2::new(0., 0.),
                                        Vec2::new(20., 1.),
                                        Vec2::new(-1., 20.),
                                        Vec2::new(23., 26.),
                                    ],
                                }),
                                ..Default::default()
                            },
                        )?;
                        crate::support::insert_transform(
                            e,
                            Transform {
                                id: fixture::id(530),
                                parent_id: Some(fixture::id(520).into()),
                                data: TransformData::Rotation(zipzi_core::RotationTransform {
                                    base_angle: 7.,
                                    pose: RotationPose {
                                        origin: Vec2::new(0.3, 0.4).into(),
                                        angle: 17.,
                                        scale: 0.5,
                                        reflect_y: reflect,
                                        ..Default::default()
                                    },
                                }),
                                ..Default::default()
                            },
                        )?;
                        let points = e
                            .read_document()
                            .unwrap()
                            .transform(&fixture::id(520))
                            .unwrap()
                            .warp()
                            .unwrap()
                            .points
                            .clone();
                        e.create_scene_binding(SceneBinding {
                            id: fixture::id(521),
                            axes: vec![BindingAxis {
                                parameter_id: fixture::id(5),
                                keys: vec![0., 1.],
                                ..Default::default()
                            }],
                            track: SceneTrack::Warp {
                                target_id: fixture::id(520).into(),
                                keyforms: [0., 1.]
                                    .into_iter()
                                    .map(|v| WarpKeyform {
                                        keys: vec![v],
                                        positions: points
                                            .iter()
                                            .enumerate()
                                            .map(|(i, p)| {
                                                Vec2::new(p.x + i as f32 * v, p.y - 2. * v)
                                            })
                                            .collect(),
                                        ..Default::default()
                                    })
                                    .collect(),
                            },
                        })?;
                        e.create_scene_binding(SceneBinding {
                            id: fixture::id(531),
                            axes: vec![BindingAxis {
                                parameter_id: fixture::id(5),
                                keys: vec![0., 1.],
                                ..Default::default()
                            }],
                            track: SceneTrack::Rotation {
                                target_id: fixture::id(530).into(),
                                keyforms: [0., 1.]
                                    .into_iter()
                                    .map(|v| RotationKeyform {
                                        keys: vec![v],
                                        rotation: RotationPose {
                                            origin: Vec2::new(0.3 + 0.1 * v, 0.4).into(),
                                            angle: 17. + 12. * v,
                                            scale: 0.5 + 0.1 * v,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .collect(),
                            },
                        })?;
                        e.edit_scene(
                            SceneEdit::Reparent {
                                edit: ReparentEdit::keep_local(
                                    vec![fixture::id(4)],
                                    fixture::id(530),
                                ),
                            },
                            Default::default(),
                            &Default::default(),
                        )?;
                        e.create_scene_binding(SceneBinding {
                            id: fixture::id(501),
                            axes: vec![BindingAxis {
                                parameter_id: fixture::id(5),
                                keys: vec![0., 1.],
                                ..Default::default()
                            }],
                            track: SceneTrack::Rotation {
                                target_id: fixture::id(3).into(),
                                keyforms: [0., 1.]
                                    .into_iter()
                                    .map(|v| RotationKeyform {
                                        keys: vec![v],
                                        rotation: RotationPose {
                                            origin: Vec2::new(11. + 2. * v, 7. - v).into(),
                                            angle: if clamped_angle {
                                                3500. + 200. * v
                                            } else {
                                                -35. + 76. * v
                                            },
                                            scale: 0.7 + 1.1 * v,
                                            reflect_x: reflect,
                                            reflect_y: v == 1. && reflect,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .collect(),
                            },
                        })?;
                        {
                            let binding = BlendShapeBinding {
                                id: fixture::id(502),
                                target_id: fixture::id(3),
                                target_kind: BlendShapeTargetKind::Rotation,
                                key_table_id: fixture::id(11),
                                constraint_ids: vec![],
                                keyforms: DeltaKeyforms::Rotation(vec![
                                    DeltaRotationKeyform::default(),
                                    DeltaRotationKeyform {
                                        origin: Some(Vec2::new(-2., 3.)),
                                        angle: Some(if clamped_angle { 200. } else { 15. }),
                                        scale: Some(0.1),
                                        ..Default::default()
                                    },
                                ]),
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
                        Ok(())
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("Animated subtree")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
            let sample = |s: &mut AuthoringSession| {
                let mut frames = Vec::new();
                for p in [0., 0.3, 1.] {
                    for b in [0., 0.4, 1.] {
                        let values = [
                            (fixture::id(5), p),
                            (fixture::id(10), b),
                            (fixture::id(20), 1. - b),
                        ]
                        .into();
                        frames.push(crate::support::frame(s.snapshot(), &values).unwrap());
                    }
                }
                frames
            };
            let before = sample(&mut s);
            let mesh = s
                .snapshot()
                .document()
                .mesh(&fixture::id(2))
                .unwrap()
                .clone();
            let ordinary = s
                .snapshot()
                .document()
                .binding(&fixture::id(6))
                .unwrap()
                .clone();
            let original_root = s
                .snapshot()
                .document()
                .transform(&fixture::id(3))
                .unwrap()
                .clone();
            let original_binding = s
                .snapshot()
                .document()
                .scene_binding(&fixture::id(501))
                .unwrap()
                .clone();
            let history = {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            };
            for parent in [500, 600, 700, 0] {
                preserving_move(&mut s, &[3, 2, 4], parent);
                same(&before, &sample(&mut s));
                assert_eq!(
                    s.snapshot()
                        .document()
                        .transform(&fixture::id(3))
                        .unwrap()
                        .parent(),
                    if parent == 0 {
                        String::new()
                    } else {
                        fixture::id(parent)
                    }
                );
                assert_eq!(
                    s.snapshot().document().mesh(&fixture::id(2)).unwrap(),
                    &mesh
                );
                assert_eq!(
                    s.snapshot().document().binding(&fixture::id(6)).unwrap(),
                    &ordinary
                );
                assert_eq!(
                    s.snapshot()
                        .document()
                        .transform(&fixture::id(520))
                        .unwrap()
                        .parent(),
                    fixture::id(3)
                );
            }
            assert_eq!(
                {
                    let history = s.history_state();
                    (history.undo_steps, history.redo_steps)
                },
                (history.0 + 4, 0)
            );
            for _ in 0..4 {
                s.undo(s.version()).unwrap();
            }
            assert_eq!(
                s.snapshot().document().transform(&fixture::id(3)).unwrap(),
                &original_root
            );
            assert_eq!(
                s.snapshot()
                    .document()
                    .scene_binding(&fixture::id(501))
                    .unwrap(),
                &original_binding
            );
            s.redo(s.version()).unwrap();
            same(&before, &sample(&mut s));
        }
    }
}

#[test]
fn rotation_scale_clamp_conflict_rejects_only_the_incompatible_conversion() {
    let mut s = setup();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_scene(
                    creation(500, false),
                    Default::default(),
                    &Default::default(),
                )?;
                let mut parent = e
                    .read_document()
                    .unwrap()
                    .transform(&fixture::id(500))
                    .unwrap()
                    .clone();
                parent.rotation_mut().unwrap().pose.scale = 2.;
                e.replace_transform(parent)?;
                {
                    let binding = BlendShapeBinding {
                        id: fixture::id(501),
                        target_id: fixture::id(3),
                        target_kind: BlendShapeTargetKind::Rotation,
                        key_table_id: fixture::id(11),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Rotation(vec![
                            DeltaRotationKeyform::default(),
                            DeltaRotationKeyform {
                                scale: Some(100.),
                                ..Default::default()
                            },
                        ]),
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
                let prepared = draft.prepare("Clamped rotation")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = samples(&mut s);
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let original = s
        .snapshot()
        .document()
        .transform(&fixture::id(3))
        .unwrap()
        .clone();
    let error = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Reparent {
                edit: preserving_edit(&[3], 500),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap_err();
    assert_eq!(error.code.as_str(), "ROTATION_SCALE_CLAMP_UNPROVEN");
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    assert_eq!(
        s.snapshot().document().transform(&fixture::id(3)).unwrap(),
        &original
    );
    same(&before, &samples(&mut s));
    // A different parent with the same inherited scale does not alter clamping.
    let mut parent = s
        .snapshot()
        .document()
        .transform(&fixture::id(500))
        .unwrap()
        .clone();
    parent.rotation_mut().unwrap().pose.scale = 1.;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_transform(parent))
            .and_then(|value| {
                let prepared = draft.prepare("Compatible parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    preserving_move(&mut s, &[3], 500);
    same(&before, &samples(&mut s));
}

#[test]
fn rotation_group_rejects_static_and_keyframed_reflections_without_consuming_redo() {
    for animated in [false, true] {
        for reflect_x in [false, true] {
            let mut s = setup();
            let reflected = RotationPose {
                reflect_x,
                reflect_y: !reflect_x,
                ..Default::default()
            };
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |edit| {
                        if animated {
                            edit.create_scene_binding(SceneBinding {
                                id: fixture::id(501),
                                axes: vec![BindingAxis {
                                    parameter_id: fixture::id(5),
                                    keys: vec![0., 1.],
                                    ..Default::default()
                                }],
                                track: SceneTrack::Rotation {
                                    target_id: fixture::id(3).into(),
                                    keyforms: vec![
                                        RotationKeyform {
                                            keys: vec![0.],
                                            ..Default::default()
                                        },
                                        RotationKeyform {
                                            keys: vec![1.],
                                            rotation: reflected,
                                            ..Default::default()
                                        },
                                    ],
                                },
                            })
                        } else {
                            let mut parent = edit
                                .read_document()
                                .unwrap()
                                .transform(&fixture::id(3))
                                .unwrap()
                                .clone();
                            parent.rotation_mut().unwrap().pose = reflected;
                            edit.replace_transform(parent)
                        }
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("Mirror parent")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |edit| {
                        edit.edit_scene(
                            creation(600, true),
                            Default::default(),
                            &Default::default(),
                        )
                    })
                    .and_then(|value| {
                        let prepared = draft.prepare("Redo sentinel")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
            s.undo(s.version()).unwrap();
            let history = {
                let history = s.history_state();
                (history.undo_steps, history.redo_steps)
            };
            let before = samples(&mut s);
            let mesh = s
                .snapshot()
                .document()
                .mesh(&fixture::id(2))
                .unwrap()
                .clone();
            let error = crate::support::prepared(&s, |c| {
                c.group_deformer(
                    creation(400, false),
                    vec![fixture::id(2)],
                    Default::default(),
                    &Default::default(),
                )
            })
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("ROTATION_GROUP_REFLECTION_UNPROVEN"),
                "{error}"
            );
            assert_eq!(
                {
                    let history = s.history_state();
                    (history.undo_steps, history.redo_steps)
                },
                history
            );
            assert!(s
                .snapshot()
                .document()
                .transform(&fixture::id(400))
                .is_none());
            assert_eq!(s.snapshot().document().mesh(&fixture::id(2)), Some(&mesh));
            same(&before, &samples(&mut s));
            // A Warp still carries the reflected mapping and remains supported.
            let plan = crate::support::prepared(&s, |c| {
                c.group_deformer(
                    creation(400, true),
                    vec![fixture::id(2)],
                    Default::default(),
                    &Default::default(),
                )
            })
            .unwrap();
            s.publish(plan.1).unwrap();
            same(&before, &samples(&mut s));
        }
    }
}
