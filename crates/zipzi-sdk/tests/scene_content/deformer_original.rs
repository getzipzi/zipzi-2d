use super::*;
use zipzi_sdk::BindingAuthoringEdit;

fn create(s: &mut AuthoringSession, n: u32, parent: Option<u32>) {
    let plan = s
        .plan_scene_edit(SceneEdit::Create {
            id: id(n),
            name: "Warp".into(),
            object: SceneCreate::Warp {
                bounds: Some([4., 6., 44., 36.]),
            },
            part_id: None,
            parent_id: parent.map(id),
            before: None,
            deformation_before: None,
        })
        .unwrap();
    s.execute_scene_plan(&plan).unwrap();
}
fn original(s: &AuthoringSession, n: u32) -> DeformerOriginal {
    s.document()
        .get_transform(&id(n))
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
    let mut t = s.document().get_transform(&id(n)).unwrap().clone();
    for p in &mut t.warp_mut().unwrap().points {
        p.x += dx;
    }
    s.edit("Move", None, |e| e.replace_transform(t)).unwrap();
}
fn bind(s: &mut AuthoringSession, n: u32, b: u32) {
    let points = s
        .document()
        .get_transform(&id(n))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    s.edit("First keys", None, |e| {
        if e.candidate_document().get_parameter(&id(100)).is_none() {
            e.create_parameter(Parameter {
                id: id(100),
                ..Default::default()
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
        .document()
        .get_transform(&id(2))
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
        s.document().get_transform(&id(2)).unwrap().warp(),
        Some(&initial)
    );
    assert_eq!(original(&s, 2), record);
    s.undo().unwrap();
    assert_ne!(
        s.document().get_transform(&id(2)).unwrap().warp(),
        Some(&initial)
    );
    s.redo().unwrap();
    assert_eq!(
        s.document().get_transform(&id(2)).unwrap().warp(),
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
    s.undo().unwrap();
    assert_eq!(original(&s, 2), created);
    s.redo().unwrap();
    assert_eq!(original(&s, 2), first);
    offset(&mut s, 2, 4.);
    assert_eq!(original(&s, 2), first);
    s.edit("Remove ordinary binding", None, |e| e.erase_object(&id(3)))
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
    s.edit("Binding shell", None, |e| {
        e.create_parameter(Parameter {
            id: id(100),
            ..Default::default()
        })?;
        e.binding_authoring(BindingAuthoringEdit::Grid(BindingGridEdit::Create {
            id: id(3),
            target: BindingGridTarget::Scene(id(2)),
        }))
    })
    .unwrap();
    assert!(!original(&s, 2).first_key_seen);
    s.edit("First axis", None, |e| {
        e.binding_authoring(BindingAuthoringEdit::Grid(BindingGridEdit::Edit {
            owner: id(3),
            operation: GridOperation::AddAxis {
                axis: BindingAxis::new(id(100), vec![-1., 1.]),
            },
        }))
    })
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
        let mut parent = s.document().get_transform(&id(20)).unwrap().clone();
        parent.warp_mut().unwrap().points[3].x += 7.;
        s.edit("Nonlinear parent", None, |e| e.replace_transform(parent))
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
        s.edit("Nested Warp", None, |e| {
            e.create_transform(Transform {
                id: id(2),
                parent_id: Some(id(20).into()),
                data: TransformData::Warp(warp),
                ..Default::default()
            })
        })
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
        let plan = s
            .plan_scene_edit(SceneEdit::Reparent {
                edit: ReparentEdit::keep_local(vec![id(2)], String::new()),
            })
            .unwrap();
        s.execute_scene_plan(&plan).unwrap();
        let plan = s.plan_scene_content(E::DeformerOriginal {
            transform_id: id(2),
            target: ShapeTarget::Base,
            action: DeformerOriginalAction::Restore {
                allow_resize: false,
            },
            context: Default::default(),
        });
        if controller {
            assert!(plan.is_err());
        } else {
            s.execute_scene_content_plan(&plan.unwrap()).unwrap();
            assert_eq!(
                s.document()
                    .get_transform(&id(2))
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
    s.edit("Parent", None, |e| {
        e.create_transform(Transform {
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
        })
    })
    .unwrap();
    let plan = s
        .plan_scene_edit(SceneEdit::Reparent {
            edit: ReparentEdit::keep_local(vec![id(2)], id(3)),
        })
        .unwrap();
    s.execute_scene_plan(&plan).unwrap();
    let parent = s.document().get_transform(&id(3)).unwrap().clone();
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
        s.document(),
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
    assert_eq!(s.document().get_transform(&id(3)), Some(&parent));
    assert_eq!(original(&s, 2), record);
}

#[test]
fn ordinary_restore_changes_only_the_explicit_stored_shape() {
    let mut s = session();
    create(&mut s, 2, None);
    bind(&mut s, 2, 3);
    let base = s.document().get_transform(&id(2)).unwrap().clone();
    let binding = s.document().get_scene_binding(&id(3)).unwrap().clone();
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
    assert_eq!(s.document().get_transform(&id(2)), Some(&base));
    let SceneTrack::Warp { keyforms, .. } = &s.document().get_scene_binding(&id(3)).unwrap().track
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
    s.edit("Blend", None, |e| {
        e.create_parameter(Parameter {
            id: id(101),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(102),
            parameter_id: id(101),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: vec![],
            base_sample_id: String::new(),
        })?;
        e.create_blend_binding(BlendShapeBinding {
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
        })
    })
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
    let before = s.document().clone();
    let version = s.version();
    assert!(s
        .plan_scene_content(E::DeformerOriginal {
            transform_id: id(2),
            target: ShapeTarget::Base,
            action: DeformerOriginalAction::Restore {
                allow_resize: false
            },
            context: Default::default()
        })
        .is_err());
    assert!(s.document().same_content(&before));
    assert_eq!(s.version(), version);
    let plan = s
        .plan_scene_content(E::DeformerOriginal {
            transform_id: id(2),
            target: ShapeTarget::Base,
            action: DeformerOriginalAction::Restore { allow_resize: true },
            context: Default::default(),
        })
        .unwrap();
    assert!(plan.report().resampled_shapes >= 5);
    s.execute_scene_content_plan(&plan).unwrap();
    let warp = s.document().get_transform(&id(2)).unwrap().warp().unwrap();
    assert_eq!((warp.rows, warp.columns), (1, 1));
    assert_eq!(warp.bezier.as_ref().unwrap().rows, 1);
    let SceneTrack::Warp { keyforms, .. } = &s.document().get_scene_binding(&id(3)).unwrap().track
    else {
        panic!()
    };
    assert!(keyforms.iter().all(|f| f.positions.len() == 4));
    let DeltaKeyforms::Warp(forms) = &s.document().get_blend_binding(&id(103)).unwrap().keyforms
    else {
        panic!()
    };
    assert!(forms.iter().all(|f| f.points.len() == 4));
    assert!(forms[1].points.iter().all(|p| *p == Vec2::new(2., 3.)));
    assert_eq!(original(&s, 2).snapshot, saved.snapshot);
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    lock(&mut s, &id(3));
    assert!(s
        .plan_scene_content(E::DeformerOriginal {
            transform_id: id(2),
            target: ShapeTarget::Base,
            action: DeformerOriginalAction::Restore { allow_resize: true },
            context: Default::default()
        })
        .is_err());
}

#[test]
fn rotation_original_retains_f64_origin_and_restores_against_the_current_reference_angle() {
    let mut s = session();
    let origin = PreciseVec2::new(12.1234567890123, 4.987654321098);
    s.edit("Rotation", None, |e| {
        e.create_transform(Transform {
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
        })
    })
    .unwrap();
    let record = original(&s, 2);
    let mut t = s.document().get_transform(&id(2)).unwrap().clone();
    let r = t.rotation_mut().unwrap();
    r.base_angle = 37.;
    r.pose.angle = -49.;
    r.pose.origin = PreciseVec2::new(20., 30.);
    s.edit("Change pose and reference", None, |e| {
        e.replace_transform(t)
    })
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
        .document()
        .get_transform(&id(2))
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
    let saved = s.save_project(&directory, None).unwrap();
    let mut reopened = session();
    reopened.open_project(&saved.manifest, None).unwrap();
    assert_eq!(original(&reopened, 2), record);
    let plan = reopened
        .plan_scene_edit(SceneEdit::Duplicate {
            ids: vec![id(2)],
            scope: SceneCopyScope::Selected,
            destination: None,
            before: None,
        })
        .unwrap();
    let copied = plan.copied_ids()[&id(2)].clone();
    reopened.execute_scene_plan(&plan).unwrap();
    assert_eq!(
        reopened
            .document()
            .get_transform(&copied)
            .unwrap()
            .original_shape
            .as_ref(),
        Some(&record)
    );
    reopened.undo().unwrap();
    assert!(reopened.document().get_transform(&copied).is_none());
    reopened.redo().unwrap();
    assert_eq!(
        reopened
            .document()
            .get_transform(&copied)
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
    s.edit("Driven parent", None, |e| {
        e.create_parameter(Parameter {
            id: id(30),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(20),
            ..Default::default()
        })?;
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
    .unwrap();
    let values = std::collections::HashMap::from([(id(30), 0.5)]);
    s.set_preview_values(values.clone()).unwrap();
    create(&mut s, 2, Some(20));
    let compare = |s: &AuthoringSession| {
        let record = original(s, 2);
        let DeformerOriginalShape::Warp { warp, .. } = &record.snapshot.as_ref().unwrap().shape
        else {
            panic!()
        };
        let mut frame = DrawableFrame::default();
        assert!(zipzi_core::evaluation::evaluate_frame_including_hidden(
            s.document(),
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
            let mut t = s.document().get_transform(&id(3)).unwrap().clone();
            if case == "nonlinear" {
                t.warp_mut().unwrap().points[3].x += 4.;
            } else {
                for p in &mut t.warp_mut().unwrap().points {
                    p.y = 0.;
                }
            }
            s.edit("Parent map", None, |e| e.replace_transform(t))
                .unwrap();
            parent = Some(id(3));
        }
        let mut t = s.document().get_transform(&id(2)).unwrap().clone();
        t.parent_id = parent.map(Into::into);
        if case == "missing" {
            t.original_shape = None;
        }
        if case == "type" {
            t.data = TransformData::Rotation(Default::default());
            t.warp_authoring = None;
        }
        s.edit("Context change", None, |e| e.replace_transform(t))
            .unwrap();
        let before = s.document().clone();
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
            s.plan_scene_content(E::DeformerOriginal {
                transform_id: id(2),
                target,
                action: DeformerOriginalAction::Restore {
                    allow_resize: false
                },
                context: Default::default()
            })
            .is_err(),
            "{case}"
        );
        assert!(s.document().same_content(&before));
        assert_eq!(s.history_state(), history);
        assert_eq!(s.version(), version);
    }
}
