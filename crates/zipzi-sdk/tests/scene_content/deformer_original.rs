use super::*;

fn create(s: &mut AuthoringSession, n: u32, parent: Option<u32>) {
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Create {
                id: id(n),
                name: "Warp".into(),
                object: SceneCreate::Warp {
                    bounds: Some([4., 6., 44., 36.]),
                },
                part_id: None,
                parent_id: parent.map(id),
                before: None,
                deformation_before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
}
fn original(s: &AuthoringSession, n: u32) -> DeformerOriginal {
    s.snapshot()
        .document()
        .transform(&id(n))
        .unwrap()
        .original_shape
        .clone()
        .unwrap()
}
fn apply(s: &mut AuthoringSession, n: u32, target: ShapeTarget, action: DeformerOriginalAction) {
    run(
        s,
        E::DeformerOriginal {
            transform_id: id(n),
            target,
            action,
            context: Default::default(),
        },
    );
}
fn offset(s: &mut AuthoringSession, n: u32, dx: f32) {
    let mut t = s.snapshot().document().transform(&id(n)).unwrap().clone();
    for p in &mut t.warp_mut().unwrap().points {
        p.x += dx;
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_transform(t))
            .and_then(|value| {
                let prepared = draft.prepare("Move")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn bind(s: &mut AuthoringSession, n: u32, b: u32) {
    let points = s
        .snapshot()
        .document()
        .transform(&id(n))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                if e.read_document().unwrap().parameter(&id(100)).is_none() {
                    e.create_parameter({
                        let mut value = Parameter {
                            id: id(100),
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                }
                e.create_scene_binding(SceneBinding {
                    id: id(b),
                    axes: vec![BindingAxis::new(id(100), vec![-1., 1.])],
                    track: SceneTrack::Warp {
                        target_id: id(n).into(),
                        keyforms: [-1., 1.]
                            .iter()
                            .map(|&v| WarpKeyform {
                                keys: vec![v],
                                positions: points.iter().map(|p| Vec2::new(p.x + v, p.y)).collect(),
                                ..Default::default()
                            })
                            .collect(),
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("First keys")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}

#[test]
fn creation_original_is_independent_of_edits_history_and_current_parameter_defaults() {
    let mut s = session();
    create(&mut s, 2, None);
    let record = original(&s, 2);
    assert_eq!(
        record.snapshot.as_ref().unwrap().source,
        DeformerOriginalSource::Created
    );
    let initial = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .clone();
    offset(&mut s, 2, 9.);
    assert_eq!(original(&s, 2), record);
    apply(
        &mut s,
        2,
        ShapeTarget::Base,
        DeformerOriginalAction::Restore {
            allow_resize: false,
        },
    );
    assert_eq!(
        s.snapshot().document().transform(&id(2)).unwrap().warp(),
        Some(&initial)
    );
    assert_eq!(original(&s, 2), record);
    s.undo(s.version()).unwrap();
    assert_ne!(
        s.snapshot().document().transform(&id(2)).unwrap().warp(),
        Some(&initial)
    );
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().transform(&id(2)).unwrap().warp(),
        Some(&initial)
    );
}

#[test]
fn first_key_auto_update_happens_once_and_manual_update_disables_it() {
    let mut s = session();
    create(&mut s, 2, None);
    offset(&mut s, 2, 8.);
    let created = original(&s, 2);
    bind(&mut s, 2, 3);
    let first = original(&s, 2);
    assert!(first.first_key_seen);
    assert_eq!(
        first.snapshot.as_ref().unwrap().source,
        DeformerOriginalSource::FirstKey
    );
    assert_ne!(first.snapshot, created.snapshot);
    s.undo(s.version()).unwrap();
    assert_eq!(original(&s, 2), created);
    s.redo(s.version()).unwrap();
    assert_eq!(original(&s, 2), first);
    offset(&mut s, 2, 4.);
    assert_eq!(original(&s, 2), first);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_binding_grid(
                    BindingGridEdit::Delete { owner: id(3) },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Remove ordinary binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    bind(&mut s, 2, 4);
    assert_eq!(original(&s, 2), first);
    create(&mut s, 8, None);
    offset(&mut s, 8, 12.);
    apply(&mut s, 8, ShapeTarget::Base, DeformerOriginalAction::Update);
    let manual = original(&s, 8);
    assert!(!manual.auto_update);
    assert_eq!(
        manual.snapshot.as_ref().unwrap().source,
        DeformerOriginalSource::Manual
    );
    bind(&mut s, 8, 9);
    assert_eq!(original(&s, 8).snapshot, manual.snapshot);
    assert!(original(&s, 8).first_key_seen);
    create(&mut s, 10, None);
    let created = original(&s, 10).snapshot;
    apply(
        &mut s,
        10,
        ShapeTarget::Base,
        DeformerOriginalAction::AutoUpdate { enabled: false },
    );
    offset(&mut s, 10, 4.);
    bind(&mut s, 10, 11);
    assert_eq!(original(&s, 10).snapshot, created);
    assert!(original(&s, 10).first_key_seen);
}

#[test]
fn shared_binding_grid_first_axis_updates_the_original_once() {
    let mut s = session();
    create(&mut s, 2, None);
    offset(&mut s, 2, 5.);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(100),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_binding_grid(
                    BindingGridEdit::Create {
                        id: id(3),
                        target: BindingGridTarget::Scene(id(2)),
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Binding shell")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(!original(&s, 2).first_key_seen);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_binding_grid(
                    BindingGridEdit::Edit {
                        owner: id(3),
                        operation: GridOperation::AddAxis {
                            axis: BindingAxis::new(id(100), vec![-1., 1.]),
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("First axis")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(original(&s, 2).first_key_seen);
    assert_eq!(
        original(&s, 2).snapshot.unwrap().source,
        DeformerOriginalSource::FirstKey
    );
}

#[test]
fn nonlinear_capture_keeps_baked_canvas_shape_but_refuses_unrepresentable_controllers() {
    for controller in [false, true] {
        let mut s = session();
        create(&mut s, 20, None);
        let mut parent = s.snapshot().document().transform(&id(20)).unwrap().clone();
        parent.warp_mut().unwrap().points[3].x += 7.;
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_transform(parent))
                .and_then(|value| {
                    let prepared = draft.prepare("Nonlinear parent")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let mut warp = WarpTransform {
            rows: 2,
            columns: 2,
            quad: true,
            bezier: None,
            points: (0..=2)
                .flat_map(|y| (0..=2).map(move |x| Vec2::new(x as f32 * 0.5, y as f32 * 0.5)))
                .collect(),
        };
        if controller {
            warp.bezier = Some(WarpBezier::from_lattice(2, 2, true, &warp.points));
        }
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    crate::support::insert_transform(
                        e,
                        Transform {
                            id: id(2),
                            parent_id: Some(id(20).into()),
                            data: TransformData::Warp(warp),
                            ..Default::default()
                        },
                    )
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Nested Warp")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let record = original(&s, 2);
        let DeformerOriginalShape::Warp {
            warp: saved,
            controller_canvas_exact,
        } = &record.snapshot.as_ref().unwrap().shape
        else {
            panic!()
        };
        assert!(!controller_canvas_exact);
        let plan = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_scene(
                SceneEdit::Reparent {
                    edit: ReparentEdit::keep_local(vec![id(2)], String::new()),
                },
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap();
        s.publish(plan.1).unwrap();
        let plan = crate::support::prepared(&s, |candidate_commands| {
            candidate_commands.edit_scene_content(
                E::DeformerOriginal {
                    transform_id: id(2),
                    target: ShapeTarget::Base,
                    action: DeformerOriginalAction::Restore {
                        allow_resize: false,
                    },
                    context: Default::default(),
                },
                Default::default(),
                &Default::default(),
            )
        });
        if controller {
            assert!(plan.is_err());
        } else {
            s.publish(plan.unwrap().1).unwrap();
            assert_eq!(
                s.snapshot()
                    .document()
                    .transform(&id(2))
                    .unwrap()
                    .warp()
                    .unwrap()
                    .points,
                saved.points
            );
        }
    }
}

#[test]
fn restore_under_changed_rotation_parent_keeps_its_current_pose_and_uses_canvas_coordinates() {
    let mut s = session();
    create(&mut s, 2, None);
    let record = original(&s, 2);
    let DeformerOriginalShape::Warp { warp: canvas, .. } = &record.snapshot.as_ref().unwrap().shape
    else {
        panic!()
    };
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(3),
                        data: TransformData::Rotation(RotationTransform {
                            base_angle: 11.,
                            pose: RotationPose {
                                origin: PreciseVec2::new(12., 9.),
                                angle: 27.,
                                scale: 1.7,
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Reparent {
                edit: ReparentEdit::keep_local(vec![id(2)], id(3)),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    let parent = s.snapshot().document().transform(&id(3)).unwrap().clone();
    apply(
        &mut s,
        2,
        ShapeTarget::Base,
        DeformerOriginalAction::Restore {
            allow_resize: false,
        },
    );
    let mut frame = DrawableFrame::default();
    assert!(zipzi_core::evaluation::evaluate_frame_including_hidden(
        s.snapshot().document(),
        &Default::default(),
        &mut frame
    )
    .is_ok());
    let restored = &frame
        .deformers
        .iter()
        .find(|d| d.id == id(2))
        .unwrap()
        .points;
    for (a, b) in canvas.points.iter().zip(restored) {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.0001);
    }
    assert_eq!(s.snapshot().document().transform(&id(3)), Some(&parent));
    assert_eq!(original(&s, 2), record);
}

#[test]
fn ordinary_restore_changes_only_the_explicit_stored_shape() {
    let mut s = session();
    create(&mut s, 2, None);
    bind(&mut s, 2, 3);
    let base = s.snapshot().document().transform(&id(2)).unwrap().clone();
    let binding = s
        .snapshot()
        .document()
        .scene_binding(&id(3))
        .unwrap()
        .clone();
    let SceneTrack::Warp { keyforms, .. } = &binding.track else {
        panic!()
    };
    let target = ShapeTarget::Ordinary {
        binding_id: id(3),
        samples: keyforms[0].samples.clone(),
    };
    let other = keyforms[1].clone();
    apply(
        &mut s,
        2,
        target,
        DeformerOriginalAction::Restore {
            allow_resize: false,
        },
    );
    assert_eq!(s.snapshot().document().transform(&id(2)), Some(&base));
    let SceneTrack::Warp { keyforms, .. } =
        &s.snapshot().document().scene_binding(&id(3)).unwrap().track
    else {
        panic!()
    };
    assert_eq!(keyforms[1], other);
    assert_eq!(keyforms[0].positions, base.warp().unwrap().points);
}

#[test]
fn topology_restore_requires_review_and_resamples_every_stored_shape_and_controller_atomically() {
    let mut s = session();
    create(&mut s, 2, None);
    run(
        &mut s,
        E::ResizeWarp {
            transform_id: id(2),
            rows: 1,
            columns: 1,
            quad: false,
        },
    );
    run(
        &mut s,
        E::WarpAuthoring {
            transform_id: id(2),
            settings: WarpAuthoringSettings {
                bezier_grid: Some(WarpBezierGrid {
                    rows: 1,
                    columns: 1,
                }),
                ..Default::default()
            },
        },
    );
    run(
        &mut s,
        E::WarpShape {
            transform_id: id(2),
            target: ShapeTarget::Base,
            action: WarpShapeAction::ResetController,
        },
    );
    apply(&mut s, 2, ShapeTarget::Base, DeformerOriginalAction::Update);
    let saved = original(&s, 2);
    bind(&mut s, 2, 3);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(101),
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
                            id: id(102),
                            parameter_id: id(101),
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
                        id: id(103),
                        target_id: id(2),
                        target_kind: BlendShapeTargetKind::Warp,
                        key_table_id: id(102),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Warp(vec![
                            DeltaWarpKeyform {
                                points: vec![Vec2::default(); 4],
                                ..Default::default()
                            },
                            DeltaWarpKeyform {
                                points: vec![Vec2::new(2., 3.); 4],
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
                let prepared = draft.prepare("Blend")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    run(
        &mut s,
        E::ResizeWarp {
            transform_id: id(2),
            rows: 4,
            columns: 3,
            quad: true,
        },
    );
    run(
        &mut s,
        E::WarpAuthoring {
            transform_id: id(2),
            settings: WarpAuthoringSettings {
                bezier_grid: Some(WarpBezierGrid {
                    rows: 2,
                    columns: 2,
                }),
                ..Default::default()
            },
        },
    );
    let before = s.snapshot().document().clone();
    let version = s.version();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::DeformerOriginal {
                    transform_id: id(2),
                    target: ShapeTarget::Base,
                    action: DeformerOriginalAction::Restore {
                        allow_resize: false
                    },
                    context: Default::default()
                },
                Default::default(),
                &Default::default()
            ))
        .is_err()
    );
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(s.version(), version);
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::DeformerOriginal {
                transform_id: id(2),
                target: ShapeTarget::Base,
                action: DeformerOriginalAction::Restore { allow_resize: true },
                context: Default::default(),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert!(plan.0.report.resampled_shapes >= 5);
    s.publish(plan.1).unwrap();
    let warp = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap();
    assert_eq!((warp.rows, warp.columns), (1, 1));
    assert_eq!(warp.bezier.as_ref().unwrap().rows, 1);
    let SceneTrack::Warp { keyforms, .. } =
        &s.snapshot().document().scene_binding(&id(3)).unwrap().track
    else {
        panic!()
    };
    assert!(keyforms.iter().all(|f| f.positions.len() == 4));
    let DeltaKeyforms::Warp(forms) = &s
        .snapshot()
        .document()
        .blend_binding(&id(103))
        .unwrap()
        .keyforms
    else {
        panic!()
    };
    assert!(forms.iter().all(|f| f.points.len() == 4));
    assert!(forms[1].points.iter().all(|p| *p == Vec2::new(2., 3.)));
    assert_eq!(original(&s, 2).snapshot, saved.snapshot);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    lock(&mut s, &id(3));
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::DeformerOriginal {
                    transform_id: id(2),
                    target: ShapeTarget::Base,
                    action: DeformerOriginalAction::Restore { allow_resize: true },
                    context: Default::default()
                },
                Default::default(),
                &Default::default()
            ))
        .is_err()
    );
}

#[test]
fn rotation_original_retains_f64_origin_and_restores_against_the_current_reference_angle() {
    let mut s = session();
    let origin = PreciseVec2::new(12.1234567890123, 4.987654321098);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(2),
                        data: TransformData::Rotation(RotationTransform {
                            base_angle: 17.,
                            pose: RotationPose {
                                origin,
                                angle: 721.25,
                                scale: 0.75,
                                reflect_x: true,
                                ..Default::default()
                            },
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Rotation")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let record = original(&s, 2);
    let mut t = s.snapshot().document().transform(&id(2)).unwrap().clone();
    let r = t.rotation_mut().unwrap();
    r.base_angle = 37.;
    r.pose.angle = -49.;
    r.pose.origin = PreciseVec2::new(20., 30.);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_transform(t))
            .and_then(|value| {
                let prepared = draft.prepare("Change pose and reference")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    apply(
        &mut s,
        2,
        ShapeTarget::Base,
        DeformerOriginalAction::Restore {
            allow_resize: false,
        },
    );
    let r = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .rotation()
        .unwrap();
    assert_eq!(r.pose.origin, origin);
    assert_eq!(r.base_angle, 37.);
    assert_eq!(r.pose.angle, 701.25);
    assert!(r.pose.reflect_x);
    assert_eq!(original(&s, 2), record);
}

#[test]
fn native_save_reopen_duplicate_and_history_retain_independent_canvas_originals() {
    let mut s = session();
    create(&mut s, 2, None);
    offset(&mut s, 2, 6.);
    apply(&mut s, 2, ShapeTarget::Base, DeformerOriginalAction::Update);
    let record = original(&s, 2);
    let directory = std::env::temp_dir().join(format!("zipzi-original-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let mut reopened = AuthoringSession::new(crate::support::roundtrip(s.snapshot()));
    assert_eq!(original(&reopened, 2), record);
    let plan = crate::support::prepared(&reopened, |c| {
        c.edit_scene(
            SceneEdit::Duplicate {
                ids: vec![id(2)],
                scope: SceneCopyScope::Selected,
                destination: None,
                before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    let copied = plan.0.copied_ids[&id(2)].clone();
    reopened.publish(plan.1).unwrap();
    assert_eq!(
        reopened
            .snapshot()
            .document()
            .transform(&copied)
            .unwrap()
            .original_shape
            .as_ref(),
        Some(&record)
    );
    reopened.undo(reopened.version()).unwrap();
    assert!(reopened.snapshot().document().transform(&copied).is_none());
    reopened.redo(reopened.version()).unwrap();
    assert_eq!(
        reopened
            .snapshot()
            .document()
            .transform(&copied)
            .unwrap()
            .original_shape
            .as_ref(),
        Some(&record)
    );
    drop(reopened);
    drop(s);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn creation_and_first_key_capture_the_frozen_authoring_parent_pose() {
    let mut s = session();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(30),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(20),
                        ..Default::default()
                    },
                )?;
                e.create_scene_binding(SceneBinding {
                    id: id(21),
                    axes: vec![BindingAxis::new(id(30), vec![-1., 1.])],
                    track: SceneTrack::Rotation {
                        target_id: id(20).into(),
                        keyforms: [-1., 1.]
                            .iter()
                            .map(|&v| RotationKeyform {
                                keys: vec![v],
                                rotation: RotationPose {
                                    angle: v * 45.,
                                    ..Default::default()
                                },
                                ..Default::default()
                            })
                            .collect(),
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Driven parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let values = std::collections::HashMap::from([(id(30), 0.)]);

    create(&mut s, 2, Some(20));
    let compare = |s: &AuthoringSession| {
        let record = original(s, 2);
        let DeformerOriginalShape::Warp { warp, .. } = &record.snapshot.as_ref().unwrap().shape
        else {
            panic!()
        };
        let mut frame = DrawableFrame::default();
        assert!(zipzi_core::evaluation::evaluate_frame_including_hidden(
            s.snapshot().document(),
            &values,
            &mut frame
        )
        .is_ok());
        let points = &frame
            .deformers
            .iter()
            .find(|d| d.id == id(2))
            .unwrap()
            .points;
        for (a, b) in warp.points.iter().zip(points) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.0001);
        }
    };
    compare(&s);
    offset(&mut s, 2, 3.);
    bind(&mut s, 2, 3);
    compare(&s);
}

#[test]
fn nonlinear_or_singular_current_parent_missing_record_type_changes_and_blends_refuse_atomically() {
    for case in ["nonlinear", "singular", "missing", "type", "blend"] {
        let mut s = session();
        create(&mut s, 2, None);
        let mut parent = None;
        if case == "nonlinear" || case == "singular" {
            create(&mut s, 3, None);
            let mut t = s.snapshot().document().transform(&id(3)).unwrap().clone();
            if case == "nonlinear" {
                t.warp_mut().unwrap().points[3].x += 4.;
            } else {
                for p in &mut t.warp_mut().unwrap().points {
                    p.y = 0.;
                }
            }
            {
                let mut draft = s.begin_draft();
                draft
                    .batch(draft.source(), |e| e.replace_transform(t))
                    .and_then(|value| {
                        let prepared = draft.prepare("Parent map")?;
                        s.publish(prepared).map(|receipt| (value, receipt))
                    })
            }
            .unwrap();
            parent = Some(id(3));
        }
        let mut t = s.snapshot().document().transform(&id(2)).unwrap().clone();
        t.parent_id = parent.map(Into::into);
        if case == "missing" {
            t.original_shape = None;
        }
        if case == "type" {
            t.data = TransformData::Rotation(Default::default());
            t.warp_authoring = None;
        }
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_transform(t))
                .and_then(|value| {
                    let prepared = draft.prepare("Context change")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let before = s.snapshot().document().clone();
        let history = s.history_state();
        let version = s.version();
        let target = if case == "blend" {
            ShapeTarget::Blend {
                binding_id: id(9),
                sample_id: id(10),
            }
        } else {
            ShapeTarget::Base
        };
        assert!(
            crate::support::prepared(&s, |candidate_commands| candidate_commands
                .edit_scene_content(
                    E::DeformerOriginal {
                        transform_id: id(2),
                        target,
                        action: DeformerOriginalAction::Restore {
                            allow_resize: false
                        },
                        context: Default::default()
                    },
                    Default::default(),
                    &Default::default()
                ))
            .is_err(),
            "{case}"
        );
        assert_eq!(s.snapshot().content_id(), before.content_id());
        assert_eq!(s.history_state(), history);
        assert_eq!(s.version(), version);
    }
}
