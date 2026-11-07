use super::*;
use zipzi_core::BindingAxisEditing;

#[test]
fn static_affine_reparent_preserves_unbound_and_bound_mesh_over_domain() {
    for bound_mesh in [false, true] {
        for strategy in [
            ReparentStrategy::PreserveEditState,
            ReparentStrategy::PreserveParameterDomain,
        ] {
            let mut s = if bound_mesh { bound() } else { fixture() };
            if !bound_mesh {
                {
                    let mut draft = s.begin_draft();
                    draft
                        .batch(draft.source(), |e| e.create_parameter(parameter(false)))
                        .and_then(|value| {
                            let prepared = draft.prepare("parameter")?;
                            s.publish(prepared).map(|receipt| (value, receipt))
                        })
                }
                .unwrap();
            }
            rotation(&mut s, 1.75);
            let original: Vec<_> = (0..=16)
                .map(|i| frame(&s, i as f32 / 16., 0.).drawables[0].positions.clone())
                .collect();
            let plan = crate::support::binding(&s, parent_edit(strategy, R)).unwrap();
            let p = plan.0.report.reparent_guarantee.as_ref().unwrap();
            assert!(p.error_bound <= 0.001);
            assert!(!p.effects.is_empty());
            let receipt = s.publish(plan.1).unwrap();
            assert!(!receipt.writes.is_empty());
            assert_eq!(
                s.snapshot().document().mesh(MESH_ID).unwrap().deformer_id,
                R
            );
            for (i, positions) in original.iter().enumerate() {
                for (a, b) in frame(&s, i as f32 / 16., 0.).drawables[0]
                    .positions
                    .iter()
                    .zip(positions)
                {
                    assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
                }
            }
            apply(&mut s, parent_edit(strategy, ""));
            for (a, b) in frame(&s, 0.5, 0.).drawables[0]
                .positions
                .iter()
                .zip(&original[8])
            {
                assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
            }
            s.undo(s.version()).unwrap();
            assert_eq!(
                s.snapshot().document().mesh(MESH_ID).unwrap().deformer_id,
                R
            );
            s.undo(s.version()).unwrap();
            assert!(s
                .snapshot()
                .document()
                .mesh(MESH_ID)
                .unwrap()
                .deformer_id
                .is_empty());
            grid_roundtrip(&s);
        }
    }
}

#[test]
fn singular_driven_and_nonlinear_parents_reject_without_mutation() {
    let mut s = bound();
    rotation(&mut s, 0.0);
    let version = s.version();
    assert_eq!(
        crate::support::binding(
            &s,
            parent_edit(ReparentStrategy::PreserveParameterDomain, R)
        )
        .unwrap_err()
        .code
        .as_str(),
        "SINGULAR_PARENT"
    );
    assert_eq!(s.version(), version);
    let mut t = s.snapshot().document().transform(R).unwrap().clone();
    t.rotation_mut().unwrap().pose.scale = 1.0;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_transform(t))
            .and_then(|value| {
                let prepared = draft.prepare("scale")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_scene_binding(zipzi_core::SceneBinding {
                    id: T.into(),
                    axes: vec![BindingAxis {
                        id: Default::default(),
                        samples: Default::default(),
                        parameter_id: PARAMETER_ID.into(),
                        keys: vec![0.0, 1.0],
                    }],
                    track: zipzi_core::SceneTrack::Rotation {
                        target_id: R.into(),
                        keyforms: vec![
                            zipzi_core::RotationKeyform {
                                keys: vec![0.0],
                                ..Default::default()
                            },
                            zipzi_core::RotationKeyform {
                                keys: vec![1.0],
                                rotation: RotationPose {
                                    angle: 30.0,
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        ],
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("drive")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = s.version();
    assert_eq!(
        crate::support::binding(
            &s,
            parent_edit(ReparentStrategy::PreserveParameterDomain, R)
        )
        .unwrap_err()
        .code
        .as_str(),
        "PARAMETER_DOMAIN_UNPROVEN"
    );
    assert_eq!(s.version(), version);
    apply(&mut s, parent_edit(ReparentStrategy::KeepLocalData, R));
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(MESH_ID)
            .unwrap()
            .base_positions[0]
            .x,
        40.0
    );
}

#[test]
fn parent_cycle_descendant_lock_nonlinear_and_nested_rotation_are_covered() {
    use zipzi_core::*;
    let mut s = bound();
    rotation(&mut s, 1.25);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: T.into(),
                        parent_id: Some(R.into()),
                        data: TransformData::Rotation(RotationTransform {
                            pose: RotationPose {
                                angle: -32.0,
                                scale: 0.75,
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("child rotation")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = frame(&s, 0.3, 0.0).drawables[0].positions.clone();
    apply(
        &mut s,
        parent_edit(ReparentStrategy::PreserveParameterDomain, T),
    );
    for (a, b) in frame(&s, 0.3, 0.0).drawables[0]
        .positions
        .iter()
        .zip(&original)
    {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
    }
    let before = s.version();
    let cycle = E::Reparent(ReparentEdit {
        placement: None,
        sources: vec![R.into()],
        parent_id: T.into(),
        strategy: ReparentStrategy::KeepLocalData,
        relative_tolerance: 1e-6,
        context: None,
        tolerance: 0.0,
    });
    assert_eq!(
        crate::support::binding(&s, cycle)
            .unwrap_err()
            .code
            .as_str(),
        "RELATION_CYCLE"
    );
    assert_eq!(s.version(), before);
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        MESH_ID.into(),
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
                let prepared = draft.prepare("lock descendant")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        crate::support::binding(
            &s,
            E::Reparent(ReparentEdit {
                placement: None,
                sources: vec![T.into()],
                parent_id: "".into(),
                strategy: ReparentStrategy::KeepLocalData,
                relative_tolerance: 1e-6,
                context: None,
                tolerance: 0.0
            })
        )
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    s.undo(s.version()).unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: Q.into(),
                        data: TransformData::Warp(WarpTransform {
                            rows: 1,
                            columns: 1,
                            quad: true,
                            points: vec![
                                Vec2::new(0.0, 0.0),
                                Vec2::new(1.0, 0.0),
                                Vec2::new(0.0, 1.0),
                                Vec2::new(1.0, 1.1),
                            ],
                            bezier: None,
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("warp")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.version();
    assert_eq!(
        crate::support::binding(&s, parent_edit(ReparentStrategy::PreserveEditState, Q))
            .unwrap_err()
            .code
            .as_str(),
        "NONLINEAR_PARENT_UNPROVEN"
    );
    assert_eq!(s.version(), before);
    apply(&mut s, parent_edit(ReparentStrategy::KeepLocalData, Q));
}

#[test]
fn grid_and_parent_changes_are_directional_on_undo_redo() {
    let mut s = bound();
    rotation(&mut s, 1.);
    s.take_events();
    let receipt = apply(
        &mut s,
        parent_edit(ReparentStrategy::PreserveParameterDomain, R),
    );
    assert!(receipt.writes.records().iter().any(|w| w.id == MESH_ID));
    assert_eq!(
        s.snapshot().document().mesh(MESH_ID).unwrap().deformer_id,
        R
    );
    s.undo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .unwrap()
        .deformer_id
        .is_empty());
    let events = s.take_events();
    assert_eq!(
        events.back().unwrap().kind,
        zipzi_sdk::PublicationKind::Undo
    );
    s.redo(s.version()).unwrap();
    let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
        owner: BINDING_ID.into(),
        operation: zipzi_sdk::GridOperation::InsertSample {
            axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
            id: uuid::Uuid::new_v4().to_string(),
            value: 0.5,
        },
    });
    let receipt = apply(&mut s, intent);
    assert!(receipt.writes.records().iter().any(|w| w.id == BINDING_ID));
}

#[test]
fn keep_local_multi_selection_moves_only_roots_and_undoes_together() {
    let mut s = bound();
    rotation(&mut s, 1.0);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: T.into(),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("destination")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    apply(&mut s, parent_edit(ReparentStrategy::KeepLocalData, R));
    let base = s
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .unwrap()
        .base_positions
        .clone();
    let before = s.version();
    let receipt = apply(
        &mut s,
        E::Reparent(ReparentEdit {
            placement: None,
            sources: vec![MESH_ID.into(), R.into()],
            parent_id: T.into(),
            strategy: ReparentStrategy::KeepLocalData,
            relative_tolerance: 1e-6,
            context: None,
            tolerance: 0.0,
        }),
    );
    assert_eq!(receipt.version.revision, before.revision + 1);
    assert!(receipt.writes.records().iter().any(|w| w.id == R));
    assert_eq!(s.snapshot().document().transform(R).unwrap().parent(), T);
    assert_eq!(
        s.snapshot().document().mesh(MESH_ID).unwrap().deformer_id,
        R
    );
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(MESH_ID)
            .unwrap()
            .base_positions,
        base
    );

    s.undo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .transform(R)
        .unwrap()
        .parent()
        .is_empty());
    assert_eq!(
        s.snapshot().document().mesh(MESH_ID).unwrap().deformer_id,
        R
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
}

#[test]
fn pending_rotation_axes_do_not_block_static_reparent() {
    let mut s = bound();
    rotation(&mut s, 1.75);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_scene_binding(zipzi_core::SceneBinding {
                    id: T.into(),
                    axes: vec![BindingAxis::new("", vec![])],
                    track: zipzi_core::SceneTrack::Rotation {
                        target_id: R.into(),
                        keyforms: vec![],
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("pending parent binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut binding = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    binding
        .axes
        .extend((0..15).map(|_| BindingAxis::new("", vec![])));
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_binding(binding))
            .and_then(|value| {
                let prepared = draft.prepare("pending mesh axes")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    apply(
        &mut s,
        E::Blend(zipzi_sdk::BlendAuthoringEdit::CreateBinding {
            id: "00000000-0000-4000-8000-000000000409".into(),
            target_id: R.into(),
            target_kind: zipzi_core::BlendShapeTargetKind::Rotation,
            key_table_id: String::new(),
        }),
    );
    let before = frame(&s, 0.5, 0.).drawables[0].positions.clone();
    apply(
        &mut s,
        parent_edit(ReparentStrategy::PreserveParameterDomain, R),
    );
    for (a, b) in frame(&s, 0.5, 0.).drawables[0]
        .positions
        .iter()
        .zip(&before)
    {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
    }
    // Configured singleton dimensions do not create interpolation corners or
    // inflate its error envelope, even at the native 16-axis boundary.
    s.undo(s.version()).unwrap();
    let pending: Vec<_> = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .axes
        .iter()
        .filter(|a| a.parameter_id.is_empty())
        .map(|a| a.id.clone())
        .collect();
    let drivers: Vec<_> = (0..pending.len())
        .map(|i| format!("00000000-0000-4000-8000-{:012x}", 3000 + i))
        .collect();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                for id in &drivers {
                    e.create_parameter({
                        let mut value = Parameter {
                            id: id.clone(),
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("singleton drivers")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    for (i, (axis, driver)) in pending.iter().zip(&drivers).enumerate() {
        for operation in [
            zipzi_sdk::GridOperation::InsertSample {
                axis_id: axis.clone(),
                id: format!("00000000-0000-4000-8000-{:012x}", 3100 + i),
                value: 0.,
            },
            zipzi_sdk::GridOperation::SetParameter {
                axis_id: axis.clone(),
                parameter_id: driver.clone(),
                reduction: None,
            },
        ] {
            apply(
                &mut s,
                E::Grid(zipzi_sdk::BindingGridEdit::Edit {
                    owner: BINDING_ID.into(),
                    operation,
                }),
            );
        }
    }
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveParameterDomain, R) else {
        panic!()
    };
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .extend(drivers.into_iter().map(|id| (id, 0.)));
    apply(&mut s, E::Reparent(edit));
    for (a, b) in frame(&s, 0.5, 0.).drawables[0]
        .positions
        .iter()
        .zip(&before)
    {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
    }
}

fn driven_rotation(s: &mut AuthoringSession) {
    rotation(s, 1.0);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: Q.into(),
                        minimum: 0.,
                        maximum: 1.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_scene_binding(zipzi_core::SceneBinding {
                    id: T.into(),
                    axes: vec![BindingAxis::new(Q, vec![0., 1.])],
                    track: zipzi_core::SceneTrack::Rotation {
                        target_id: R.into(),
                        keyforms: vec![
                            zipzi_core::RotationKeyform {
                                keys: vec![0.],
                                rotation: RotationPose {
                                    angle: -15.,
                                    scale: 0.8,
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                            zipzi_core::RotationKeyform {
                                keys: vec![1.],
                                rotation: RotationPose {
                                    angle: 35.,
                                    scale: 1.4,
                                    ..Default::default()
                                },
                                ..Default::default()
                            },
                        ],
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("driver")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}

#[test]
fn driven_edit_snapshot_succeeds_without_claiming_a_varying_domain() {
    let mut s = bound();
    driven_rotation(&mut s);
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveEditState, R) else {
        panic!()
    };
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .insert(Q.into(), 0.37);
    let original = frame(&s, 0.5, 0.37).drawables[0].positions.clone();

    let plan = crate::support::binding(&s, E::Reparent(edit.clone())).unwrap();
    let p = plan.0.report.reparent_guarantee.as_ref().unwrap();
    assert_eq!(p.context.as_ref().unwrap().parameter_values[Q], 0.37);
    assert!(p.reference_scale > 1.);
    assert!(p.error_bound <= p.tolerance + p.relative_tolerance * p.reference_scale);
    s.publish(plan.1).unwrap();
    for (a, b) in frame(&s, 0.5, 0.37).drawables[0]
        .positions
        .iter()
        .zip(&original)
    {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
    }
    assert!((frame(&s, 0.5, 0.9).drawables[0].positions[0].x - original[0].x).abs() > 0.1);
    s.undo(s.version()).unwrap();
    edit.strategy = ReparentStrategy::PreserveParameterDomain;
    edit.context
        .as_mut()
        .unwrap()
        .parameter_domain
        .push(zipzi_core::document::ConversionDomain {
            parameter_id: Q.into(),
            minimum: 0.,
            maximum: 1.,
        });
    let version = s.version();
    let history = s.history_state();
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "PARAMETER_DOMAIN_UNPROVEN"
    );
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit))
            .unwrap_err()
            .code
            .as_str(),
        "PARAMETER_DOMAIN_UNPROVEN"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn preservation_requires_complete_finite_context_and_does_not_relax_tolerance() {
    let mut s = bound();
    rotation(&mut s, 1.75);
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveEditState, R) else {
        panic!()
    };
    let context = edit.context.take();
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "REPARENT_CONTEXT_REQUIRED"
    );
    edit.context = Some(zipzi_sdk::ReparentContext::default());
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "MISSING_EDIT_PARAMETER"
    );
    edit.context = context;
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .insert(PARAMETER_ID.into(), f32::NAN);
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "INVALID_EDIT_PARAMETER"
    );
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .insert(PARAMETER_ID.into(), 0.5);
    edit.tolerance = 1e-15;
    edit.relative_tolerance = 0.;
    let version = s.version();
    let history = s.history_state();
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit))
            .unwrap_err()
            .code
            .as_str(),
        "REPARENT_TOLERANCE_EXCEEDED"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn support_matrix_includes_rotation_warp_subtrees_and_same_parent_noop() {
    let mut s = bound();
    rotation(&mut s, 1.0);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: T.into(),
                        data: TransformData::Warp(zipzi_core::WarpTransform {
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
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("warp")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    for id in [R, T] {
        let mut edit =
            ReparentEdit::keep_local(vec![id.into()], if id == R { T.into() } else { R.into() });

        let receipt = apply(&mut s, E::Reparent(edit.clone()));
        assert!(!receipt.writes.is_empty());

        let history = s.history_state();
        assert!(apply(&mut s, E::Reparent(edit.clone())).writes.is_empty());
        assert_eq!(s.history_state(), history);
        s.undo(s.version()).unwrap();
        for strategy in [
            ReparentStrategy::PreserveEditState,
            ReparentStrategy::PreserveParameterDomain,
        ] {
            edit.strategy = strategy;
            edit.context = Some(Default::default());
            edit.tolerance = 0.001;

            let plan = crate::support::binding(&s, E::Reparent(edit.clone())).unwrap();
            s.publish(plan.1).unwrap();
            s.undo(s.version()).unwrap();
        }
    }
}

#[test]
fn domain_can_fix_an_ancestor_driver_and_dynamic_singularity_requires_a_plan() {
    let mut s = bound();
    driven_rotation(&mut s);
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveParameterDomain, R) else {
        panic!()
    };
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .insert(Q.into(), 0.37);
    let original: Vec<_> = (0..=8)
        .map(|i| {
            frame(&s, i as f32 / 8., 0.37).drawables[0]
                .positions
                .clone()
        })
        .collect();
    apply(&mut s, E::Reparent(edit));
    for (i, before) in original.iter().enumerate() {
        for (a, b) in frame(&s, i as f32 / 8., 0.37).drawables[0]
            .positions
            .iter()
            .zip(before)
        {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
    }
    s.undo(s.version()).unwrap();
    let mut binding = s.snapshot().document().scene_binding(T).unwrap().clone();
    if let zipzi_core::SceneTrack::Rotation { keyforms, .. } = &mut binding.track {
        keyforms[0].rotation.scale = 0.;
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_scene_binding(binding))
            .and_then(|value| {
                let prepared = draft.prepare("singular sample")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveEditState, R) else {
        panic!()
    };
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .insert(Q.into(), 0.);

    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit))
            .unwrap_err()
            .code
            .as_str(),
        "SINGULAR_PARENT"
    );
}

#[test]
fn snapshot_uses_runtime_blend_driver_normalization_and_rotation_clamps() {
    use zipzi_core::*;
    let mut s = bound();
    rotation(&mut s, 1.0);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: Q.into(),
                        kind: ParameterKind::BlendShape,
                        repeat: true,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateTable {
                        table: BlendShapeKeyTable {
                            id: T.into(),
                            parameter_id: Q.into(),
                            keys: vec![-1., 0., 1.],
                            base_key_idx: 1,
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
                        id: "00000000-0000-4000-8000-000000000499".into(),
                        target_id: R.into(),
                        target_kind: BlendShapeTargetKind::Rotation,
                        key_table_id: T.into(),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Rotation(
                            [-0.5, 0., 0.5]
                                .into_iter()
                                .map(|v| DeltaRotationKeyform {
                                    angle: Some(v * 30.),
                                    scale: Some(v),
                                    origin: Some(Vec2::new(v, v)),
                                    ..Default::default()
                                })
                                .collect(),
                        ),
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
                let prepared = draft.prepare("blend parent")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    for value in [0.37, 1.] {
        let before = frame(&s, 0.5, value).drawables[0].positions.clone();
        let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveEditState, R) else {
            panic!()
        };
        edit.context
            .as_mut()
            .unwrap()
            .parameter_values
            .insert(Q.into(), value);
        let plan = crate::support::binding(&s, E::Reparent(edit)).unwrap();
        assert_eq!(
            plan.0
                .report
                .reparent_guarantee
                .as_ref()
                .unwrap()
                .context
                .as_ref()
                .unwrap()
                .parameter_values[Q],
            value
        );
        s.publish(plan.1).unwrap();
        for (a, b) in frame(&s, 0.5, value).drawables[0]
            .positions
            .iter()
            .zip(before)
        {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
        s.undo(s.version()).unwrap();
    }
}

#[test]
fn reparent_budget_includes_driven_ancestor_sampling() {
    let mut s = fixture();
    driven_rotation(&mut s);
    let edit = ReparentEdit {
        placement: None,
        strategy: ReparentStrategy::PreserveEditState,
        tolerance: 0.001,
        context: Some(zipzi_sdk::ReparentContext {
            parameter_values: [(Q.into(), 0.37)].into_iter().collect(),
            ..Default::default()
        }),
        ..ReparentEdit::keep_local(vec![MESH_ID.into()], R.into())
    };
    let plan = crate::support::binding(&s, E::Reparent(edit.clone())).unwrap();
    assert_eq!(plan.0.work.sample_combinations, 2);
    let version = s.version();
    let history = s.history_state();
    assert_eq!(
        crate::support::binding_with_control(
            &s,
            E::Reparent(edit),
            ConversionLimits {
                max_sample_combinations: 1,
                ..Default::default()
            },
            &Default::default()
        )
        .unwrap_err()
        .code,
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    s.publish(plan.1).unwrap();
}

#[test]
fn snapshot_scene_reparent_combines_conversion_and_exact_order_in_one_history_entry() {
    use zipzi_core::document::SceneCollection as C;
    use zipzi_core::document::SceneEdit;
    use zipzi_sdk::ReparentPlacement;
    let mut s = bound();
    rotation(&mut s, 1.75);
    let mut sibling = s.snapshot().document().mesh(MESH_ID).unwrap().clone();
    sibling.id = T.into();
    sibling.runtime_id = "sibling".into();
    sibling.deformer_id = R.into();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.create_mesh(sibling))
            .and_then(|value| {
                let prepared = draft.prepare("Sibling")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveParameterDomain, R) else {
        panic!()
    };
    edit.placement = Some(ReparentPlacement { before: None });
    let original = (0..=8)
        .map(|i| frame(&s, i as f32 / 8., 0.).drawables[0].positions.clone())
        .collect::<Vec<_>>();
    let history = s.history_state();
    let plan = crate::support::prepared(&s, |c| {
        c.edit_scene(
            SceneEdit::Reparent { edit: edit.clone() },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert!(plan
        .0
        .conversion
        .as_ref()
        .unwrap()
        .report
        .reparent_guarantee
        .is_some());
    let stale = s.begin_draft();
    let order_before = s.snapshot().document().hierarchy_order().clone();
    let intent = SceneEdit::Reparent { edit: edit.clone() };
    let encoded = serde_json::to_string(&intent).unwrap();
    assert_eq!(serde_json::from_str::<SceneEdit>(&encoded).unwrap(), intent);
    assert!(crate::support::prepared(&s, |c| {
        c.edit_scene(intent.clone(), Default::default(), &Default::default())?;
        c.create_parameter(parameter(false))
    })
    .is_err());
    assert_eq!(s.history_state(), history);
    assert_eq!(s.snapshot().document().hierarchy_order(), &order_before);
    assert!(s
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .unwrap()
        .deformer_id
        .is_empty());
    s.publish(plan.1).unwrap();
    let order = || C::Deformation {
        parent_id: Some(R.into()),
    };
    assert_eq!(
        s.snapshot()
            .document()
            .scene_collection(&order())
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [T, MESH_ID]
    );
    for (i, positions) in original.iter().enumerate() {
        let after = frame(&s, i as f32 / 8., 0.);
        for (a, b) in after
            .drawables
            .iter()
            .find(|d| d.id == MESH_ID)
            .unwrap()
            .positions
            .iter()
            .zip(positions)
        {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
    }
    s.undo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .unwrap()
        .deformer_id
        .is_empty());
    assert_eq!(
        s.snapshot()
            .document()
            .scene_collection(&order())
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [T]
    );
    // An immutable old snapshot can still compute, but it cannot publish after Undo.
    let old = stale.prepare("old snapshot").unwrap();
    let undone = s.history_state();
    assert!(s.publish(old).is_err());
    assert_eq!(s.history_state(), undone);
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().mesh(MESH_ID).unwrap().deformer_id,
        R
    );
    assert!(s.history_state() != history);
    grid_roundtrip(&s);
}

#[test]
fn reparent_placement_rejects_foreign_anchor_and_cancelled_worker_without_discarding_redo() {
    use zipzi_core::document::SceneEdit;
    use zipzi_sdk::{ConversionControl, ReparentPlacement};
    let mut s = bound();
    rotation(&mut s, 1.75);
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveParameterDomain, R) else {
        panic!()
    };
    edit.placement = Some(ReparentPlacement { before: None });
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Reparent { edit: edit.clone() },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let version = s.version();
    let control = ConversionControl::default();
    let plan = crate::support::prepared(&s, |c| {
        c.edit_scene(
            SceneEdit::Reparent { edit: edit.clone() },
            Default::default(),
            &control,
        )
    })
    .unwrap();
    control.cancel();
    assert!(s
        .publish_with_control(plan.1, || control.check().is_ok())
        .is_err());
    edit.placement.as_mut().unwrap().before = Some(MESH_ID.into());
    assert_eq!(
        crate::support::binding(&s, E::Reparent(edit.clone()))
            .unwrap_err()
            .code
            .as_str(),
        "INVALID_PLACEMENT"
    );
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            SceneEdit::Reparent { edit },
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    s.redo(s.version()).unwrap();
}
