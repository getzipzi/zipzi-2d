use super::*;

#[test]
fn static_affine_reparent_preserves_unbound_and_bound_mesh_over_domain() {
    for bound_mesh in [false, true] {
        for strategy in [
            ReparentStrategy::PreserveEditState,
            ReparentStrategy::PreserveParameterDomain,
        ] {
            let mut s = if bound_mesh { bound() } else { fixture() };
            if !bound_mesh {
                s.edit("parameter", None, |e| e.create_parameter(parameter(false)))
                    .unwrap();
            }
            rotation(&mut s, 1.75);
            let original: Vec<_> = (0..=16)
                .map(|i| frame(&s, i as f32 / 16., 0.).drawables[0].positions.clone())
                .collect();
            let plan = s.plan_binding_authoring(parent_edit(strategy, R)).unwrap();
            let BindingConversion::Reparent(p) = plan.conversion() else {
                panic!()
            };
            assert!(p.guarantee().error_bound <= 0.001);
            assert!(!p.guarantee().effects.is_empty());
            let receipt = s.execute_binding_plan(&plan).unwrap();
            assert!(receipt.changed);
            assert_eq!(s.document().get_mesh(MESH_ID).unwrap().deformer_id, R);
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
            s.undo().unwrap();
            assert_eq!(s.document().get_mesh(MESH_ID).unwrap().deformer_id, R);
            s.undo().unwrap();
            assert!(s
                .document()
                .get_mesh(MESH_ID)
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
        s.plan_binding_authoring(parent_edit(ReparentStrategy::PreserveParameterDomain, R))
            .unwrap_err()
            .code
            .as_ref(),
        "SINGULAR_PARENT"
    );
    assert_eq!(s.version(), version);
    let mut t = s.document().get_transform(R).unwrap().clone();
    t.rotation_mut().unwrap().pose.scale = 1.0;
    s.edit("scale", None, |e| e.replace_transform(t)).unwrap();
    s.edit("drive", None, |e| {
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
    .unwrap();
    let version = s.version();
    assert_eq!(
        s.plan_binding_authoring(parent_edit(ReparentStrategy::PreserveParameterDomain, R))
            .unwrap_err()
            .code
            .as_ref(),
        "PARAMETER_DOMAIN_UNPROVEN"
    );
    assert_eq!(s.version(), version);
    apply(&mut s, parent_edit(ReparentStrategy::KeepLocalData, R));
    assert_eq!(
        s.document().get_mesh(MESH_ID).unwrap().base_positions[0].x,
        40.0
    );
}

#[test]
fn parent_cycle_descendant_lock_nonlinear_and_nested_rotation_are_covered() {
    use zipzi_core::*;
    let mut s = bound();
    rotation(&mut s, 1.25);
    s.edit("child rotation", None, |e| {
        e.create_transform(Transform {
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
        })
    })
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
        s.plan_binding_authoring(cycle).unwrap_err().code.as_ref(),
        "RELATION_CYCLE"
    );
    assert_eq!(s.version(), before);
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        MESH_ID.into(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock descendant", None, |e| e.replace_editor_state(flags))
        .unwrap();
    assert_eq!(
        s.plan_binding_authoring(E::Reparent(ReparentEdit {
            placement: None,
            sources: vec![T.into()],
            parent_id: "".into(),
            strategy: ReparentStrategy::KeepLocalData,
            relative_tolerance: 1e-6,
            context: None,
            tolerance: 0.0
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    s.undo().unwrap();
    s.edit("warp", None, |e| {
        e.create_transform(Transform {
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
        })
    })
    .unwrap();
    let before = s.version();
    assert_eq!(
        s.plan_binding_authoring(parent_edit(ReparentStrategy::PreserveEditState, Q))
            .unwrap_err()
            .code
            .as_ref(),
        "NONLINEAR_PARENT_UNPROVEN"
    );
    assert_eq!(s.version(), before);
    apply(&mut s, parent_edit(ReparentStrategy::KeepLocalData, Q));
}

#[test]
fn grid_and_parent_changes_are_directional_on_undo_redo() {
    let mut s = bound();
    rotation(&mut s, 1.0);
    s.drain_events();
    let receipt = apply(
        &mut s,
        parent_edit(ReparentStrategy::PreserveParameterDomain, R),
    );
    let edge = receipt
        .changes
        .binding_references
        .iter()
        .find(|r| r.entity.id == MESH_ID)
        .unwrap();
    assert_eq!(edge.before.as_deref(), Some(""));
    assert_eq!(edge.after.as_deref(), Some(R));
    s.undo().unwrap();
    let events = s.drain_events();
    let edge = events
        .last()
        .unwrap()
        .changes
        .binding_references
        .iter()
        .find(|r| r.entity.id == MESH_ID)
        .unwrap();
    assert_eq!(edge.before.as_deref(), Some(R));
    assert_eq!(edge.after.as_deref(), Some(""));
    s.redo().unwrap();
    let receipt = apply(
        &mut s,
        E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5,
        }),
    );
    assert!(receipt.changes.collections.iter().any(|c| matches!(c, zipzi_sdk::CollectionRef::BindingGrid { binding_id, .. } if binding_id == BINDING_ID)));
    assert!(!receipt.changes.full_invalidation);
}

#[test]
fn keep_local_multi_selection_moves_only_roots_and_undoes_together() {
    let mut s = bound();
    rotation(&mut s, 1.0);
    s.edit("destination", None, |e| {
        e.create_transform(Transform {
            id: T.into(),
            ..Default::default()
        })
    })
    .unwrap();
    apply(&mut s, parent_edit(ReparentStrategy::KeepLocalData, R));
    let base = s
        .document()
        .get_mesh(MESH_ID)
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
    assert_eq!(receipt.after.revision, before.revision + 1);
    assert!(receipt.object_ids.iter().any(|id| id == MESH_ID));
    assert!(receipt.object_ids.iter().any(|id| id == BINDING_ID));
    assert_eq!(s.document().get_transform(R).unwrap().parent(), T);
    assert_eq!(s.document().get_mesh(MESH_ID).unwrap().deformer_id, R);
    assert_eq!(s.document().get_mesh(MESH_ID).unwrap().base_positions, base);
    assert!(receipt.changes.collections.iter().any(|c| matches!(c, zipzi_sdk::CollectionRef::DeformationChildren { parent_id, .. } if parent_id == T)));
    s.undo().unwrap();
    assert!(s.document().get_transform(R).unwrap().parent().is_empty());
    assert_eq!(s.document().get_mesh(MESH_ID).unwrap().deformer_id, R);
    assert!(s.document().validate_structure().is_empty());
}

#[test]
fn pending_rotation_axes_do_not_block_static_reparent() {
    let mut s = bound();
    rotation(&mut s, 1.75);
    s.edit("pending parent binding", None, |e| {
        e.create_scene_binding(zipzi_core::SceneBinding {
            id: T.into(),
            axes: vec![BindingAxis::new("", vec![])],
            track: zipzi_core::SceneTrack::Rotation {
                target_id: R.into(),
                keyforms: vec![],
            },
        })
    })
    .unwrap();
    let mut binding = s.binding(BINDING_ID).unwrap();
    binding
        .axes
        .extend((0..15).map(|_| BindingAxis::new("", vec![])));
    s.edit("pending mesh axes", None, |e| e.replace_binding(binding))
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
    s.undo().unwrap();
    let pending: Vec<_> = s
        .binding(BINDING_ID)
        .unwrap()
        .axes
        .iter()
        .filter(|a| a.parameter_id.is_empty())
        .map(|a| a.id.clone())
        .collect();
    let drivers: Vec<_> = (0..pending.len())
        .map(|i| format!("00000000-0000-4000-8000-{:012x}", 3000 + i))
        .collect();
    s.edit("singleton drivers", None, |e| {
        for id in &drivers {
            e.create_parameter(Parameter {
                id: id.clone(),
                ..Default::default()
            })?;
        }
        Ok(())
    })
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
    s.edit("driver", None, |e| {
        e.create_parameter(Parameter {
            id: Q.into(),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
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
    assert_eq!(
        s.reparent_support(&edit).unwrap(),
        zipzi_sdk::ReparentSupport::RequiresConversionPlan
    );
    let plan = s.plan_binding_authoring(E::Reparent(edit.clone())).unwrap();
    let BindingConversion::Reparent(p) = plan.conversion() else {
        panic!()
    };
    assert_eq!(
        p.guarantee().context.as_ref().unwrap().parameter_values[Q],
        0.37
    );
    assert!(p.guarantee().reference_scale > 1.);
    assert!(
        p.guarantee().error_bound
            <= p.guarantee().tolerance
                + p.guarantee().relative_tolerance * p.guarantee().reference_scale
    );
    s.execute_binding_plan(&plan).unwrap();
    for (a, b) in frame(&s, 0.5, 0.37).drawables[0]
        .positions
        .iter()
        .zip(&original)
    {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
    }
    assert!((frame(&s, 0.5, 0.9).drawables[0].positions[0].x - original[0].x).abs() > 0.1);
    s.undo().unwrap();
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
        s.reparent_support(&edit).unwrap_err().code.as_ref(),
        "PARAMETER_DOMAIN_UNPROVEN"
    );
    assert_eq!(
        s.plan_binding_authoring(E::Reparent(edit))
            .unwrap_err()
            .code
            .as_ref(),
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
        s.reparent_support(&edit).unwrap_err().code.as_ref(),
        "REPARENT_CONTEXT_REQUIRED"
    );
    edit.context = Some(zipzi_sdk::ReparentContext::default());
    assert_eq!(
        s.reparent_support(&edit).unwrap_err().code.as_ref(),
        "MISSING_EDIT_PARAMETER"
    );
    edit.context = context;
    edit.context
        .as_mut()
        .unwrap()
        .parameter_values
        .insert(PARAMETER_ID.into(), f32::NAN);
    assert_eq!(
        s.reparent_support(&edit).unwrap_err().code.as_ref(),
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
        s.plan_binding_authoring(E::Reparent(edit))
            .unwrap_err()
            .code
            .as_ref(),
        "REPARENT_TOLERANCE_EXCEEDED"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn support_matrix_includes_rotation_warp_subtrees_and_same_parent_noop() {
    let mut s = bound();
    rotation(&mut s, 1.0);
    s.edit("warp", None, |e| {
        e.create_transform(Transform {
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
        })
    })
    .unwrap();
    for id in [R, T] {
        let mut edit =
            ReparentEdit::keep_local(vec![id.into()], if id == R { T.into() } else { R.into() });
        assert_eq!(
            s.reparent_support(&edit).unwrap(),
            zipzi_sdk::ReparentSupport::Available
        );
        let receipt = apply(&mut s, E::Reparent(edit.clone()));
        assert!(receipt.changed);
        assert_eq!(
            s.reparent_support(&edit).unwrap(),
            zipzi_sdk::ReparentSupport::NoChange
        );
        let history = s.history_state();
        assert!(!apply(&mut s, E::Reparent(edit.clone())).changed);
        assert_eq!(s.history_state(), history);
        s.undo().unwrap();
        for strategy in [
            ReparentStrategy::PreserveEditState,
            ReparentStrategy::PreserveParameterDomain,
        ] {
            edit.strategy = strategy;
            edit.context = Some(Default::default());
            assert_eq!(
                s.reparent_support(&edit).unwrap_err().code.as_ref(),
                "SUBTREE_CONVERSION_UNPROVEN"
            );
            assert_eq!(
                s.plan_binding_authoring(E::Reparent(edit.clone()))
                    .unwrap_err()
                    .code
                    .as_ref(),
                "SUBTREE_CONVERSION_UNPROVEN"
            );
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
    s.undo().unwrap();
    let mut binding = s.document().get_scene_binding(T).unwrap().clone();
    if let zipzi_core::SceneTrack::Rotation { keyforms, .. } = &mut binding.track {
        keyforms[0].rotation.scale = 0.;
    }
    s.edit("singular sample", None, |e| {
        e.replace_scene_binding(binding)
    })
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
        s.reparent_support(&edit).unwrap(),
        zipzi_sdk::ReparentSupport::RequiresConversionPlan
    );
    assert_eq!(
        s.plan_binding_authoring(E::Reparent(edit))
            .unwrap_err()
            .code
            .as_ref(),
        "SINGULAR_PARENT"
    );
}

#[test]
fn snapshot_uses_runtime_blend_driver_normalization_and_rotation_clamps() {
    use zipzi_core::*;
    let mut s = bound();
    rotation(&mut s, 1.0);
    s.edit("blend parent", None, |e| {
        e.create_parameter(Parameter {
            id: Q.into(),
            kind: ParameterKind::BlendShape,
            repeat: true,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: T.into(),
            parameter_id: Q.into(),
            keys: vec![-1., 0., 1.],
            base_key_idx: 1,
            samples: vec![],
            base_sample_id: String::new(),
        })?;
        e.create_blend_binding(BlendShapeBinding {
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
        })
    })
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
        let plan = s.plan_binding_authoring(E::Reparent(edit)).unwrap();
        assert_eq!(
            plan.report()
                .reparent_guarantee
                .as_ref()
                .unwrap()
                .context
                .as_ref()
                .unwrap()
                .parameter_values[Q],
            value
        );
        s.execute_binding_plan(&plan).unwrap();
        for (a, b) in frame(&s, 0.5, value).drawables[0]
            .positions
            .iter()
            .zip(before)
        {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
        s.undo().unwrap();
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
    let plan = s.plan_binding_authoring(E::Reparent(edit.clone())).unwrap();
    assert_eq!(plan.cost().sample_combinations, 2);
    let version = s.version();
    let history = s.history_state();
    s.set_authoring_limits(AuthoringLimits {
        max_sample_combinations: 1,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        s.reparent_support(&edit).unwrap(),
        zipzi_sdk::ReparentSupport::RequiresConversionPlan
    );
    assert_eq!(
        s.plan_binding_authoring(E::Reparent(edit))
            .unwrap_err()
            .code
            .as_ref(),
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(
        s.execute_binding_plan(&plan).unwrap_err().code.as_ref(),
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn snapshot_scene_reparent_combines_conversion_and_exact_order_in_one_history_entry() {
    use zipzi_sdk::{ConversionControl, ReparentPlacement, SceneCollection as C, SceneEdit};
    let mut s = bound();
    rotation(&mut s, 1.75);
    let mut sibling = s.document().get_mesh(MESH_ID).unwrap().clone();
    sibling.id = T.into();
    sibling.runtime_id = "sibling".into();
    sibling.deformer_id = R.into();
    s.edit("Sibling", None, |e| e.create_mesh(sibling)).unwrap();
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveParameterDomain, R) else {
        panic!()
    };
    edit.placement = Some(ReparentPlacement { before: None });
    let original = (0..=8)
        .map(|i| frame(&s, i as f32 / 8., 0.).drawables[0].positions.clone())
        .collect::<Vec<_>>();
    let snapshot = s.read_snapshot();
    let history = s.history_state();
    let plan = snapshot
        .plan_scene_edit_with_control(
            SceneEdit::Reparent { edit: edit.clone() },
            s.authoring_limits(),
            &ConversionControl::default(),
        )
        .unwrap();
    assert!(plan
        .conversion_report()
        .unwrap()
        .reparent_guarantee
        .is_some());
    let order_before = s.document().hierarchy_order().clone();
    let intent = SceneEdit::Reparent { edit: edit.clone() };
    let encoded = serde_json::to_string(&intent).unwrap();
    assert_eq!(serde_json::from_str::<SceneEdit>(&encoded).unwrap(), intent);
    assert!(s
        .edit("Failed batch", Some(plan.version()), |e| {
            e.apply_scene_plan(&plan)?;
            e.create_parameter(parameter(false))
        })
        .is_err());
    assert_eq!(s.history_state(), history);
    assert_eq!(s.document().hierarchy_order(), &order_before);
    assert!(s
        .document()
        .get_mesh(MESH_ID)
        .unwrap()
        .deformer_id
        .is_empty());
    s.execute_scene_plan(&plan).unwrap();
    let order = || C::Deformation {
        parent_id: Some(R.into()),
    };
    assert_eq!(
        s.scene_collection(&order())
            .unwrap()
            .iter()
            .map(|h| h.id())
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
    s.undo().unwrap();
    assert!(s
        .document()
        .get_mesh(MESH_ID)
        .unwrap()
        .deformer_id
        .is_empty());
    assert_eq!(
        s.scene_collection(&order())
            .unwrap()
            .iter()
            .map(|h| h.id())
            .collect::<Vec<_>>(),
        [T]
    );
    // An immutable old snapshot can still compute, but it cannot publish after Undo.
    let old = snapshot
        .plan_scene_edit_with_control(
            SceneEdit::Reparent { edit },
            s.authoring_limits(),
            &ConversionControl::default(),
        )
        .unwrap();
    let undone = s.history_state();
    assert!(s.execute_scene_plan(&old).is_err());
    assert_eq!(s.history_state(), undone);
    s.redo().unwrap();
    assert_eq!(s.document().get_mesh(MESH_ID).unwrap().deformer_id, R);
    assert!(s.history_state() != history);
    grid_roundtrip(&s);
}

#[test]
fn reparent_placement_rejects_foreign_anchor_and_cancelled_worker_without_discarding_redo() {
    use zipzi_sdk::{ConversionControl, ReparentPlacement, SceneEdit};
    let mut s = bound();
    rotation(&mut s, 1.75);
    let E::Reparent(mut edit) = parent_edit(ReparentStrategy::PreserveParameterDomain, R) else {
        panic!()
    };
    edit.placement = Some(ReparentPlacement { before: None });
    let plan = s
        .plan_scene_edit(SceneEdit::Reparent { edit: edit.clone() })
        .unwrap();
    s.execute_scene_plan(&plan).unwrap();
    s.undo().unwrap();
    let history = s.history_state();
    let version = s.version();
    let control = ConversionControl::default();
    let plan = s
        .read_snapshot()
        .plan_scene_edit_with_control(
            SceneEdit::Reparent { edit: edit.clone() },
            s.authoring_limits(),
            &control,
        )
        .unwrap();
    control.cancel();
    assert!(s.execute_scene_plan(&plan).is_err());
    edit.placement.as_mut().unwrap().before = Some(MESH_ID.into());
    assert_eq!(
        s.reparent_support(&edit).unwrap_err().code.as_ref(),
        "INVALID_PLACEMENT"
    );
    assert!(s.plan_scene_edit(SceneEdit::Reparent { edit }).is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    s.redo().unwrap();
}
