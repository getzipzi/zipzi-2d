use super::*;

#[test]
fn static_affine_reparent_preserves_unbound_and_bound_mesh_over_domain() {
    for bound_mesh in [false, true] {
        for strategy in [
            ReparentStrategy::PreserveEditState,
            ReparentStrategy::PreserveParameterDomain,
        ] {
            let mut s = if bound_mesh { bound() } else { fixture() };
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
        e.create_scene_binding(kasane_core::SceneBinding {
            id: T.into(),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: PARAMETER_ID.into(),
                keys: vec![0.0, 1.0],
            }],
            track: kasane_core::SceneTrack::Rotation {
                target_id: R.into(),
                keyforms: vec![
                    kasane_core::RotationKeyform {
                        keys: vec![0.0],
                        ..Default::default()
                    },
                    kasane_core::RotationKeyform {
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
    use kasane_core::*;
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
        sources: vec![R.into()],
        parent_id: T.into(),
        strategy: ReparentStrategy::KeepLocalData,
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
        kasane_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock descendant", None, |e| e.replace_editor_state(flags))
        .unwrap();
    assert_eq!(
        s.plan_binding_authoring(E::Reparent(ReparentEdit {
            sources: vec![T.into()],
            parent_id: "".into(),
            strategy: ReparentStrategy::KeepLocalData,
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
    assert!(receipt.changes.collections.iter().any(|c| matches!(c, kasane_sdk::CollectionRef::BindingGrid { binding_id, .. } if binding_id == BINDING_ID)));
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
            sources: vec![MESH_ID.into(), R.into()],
            parent_id: T.into(),
            strategy: ReparentStrategy::KeepLocalData,
            tolerance: 0.0,
        }),
    );
    assert_eq!(receipt.after.revision, before.revision + 1);
    assert_eq!(s.document().get_transform(R).unwrap().parent(), T);
    assert_eq!(s.document().get_mesh(MESH_ID).unwrap().deformer_id, R);
    assert_eq!(s.document().get_mesh(MESH_ID).unwrap().base_positions, base);
    assert!(receipt.changes.collections.iter().any(|c| matches!(c, kasane_sdk::CollectionRef::DeformationChildren { parent_id, .. } if parent_id == T)));
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
        e.create_scene_binding(kasane_core::SceneBinding {
            id: T.into(),
            axes: vec![BindingAxis::new("", vec![])],
            track: kasane_core::SceneTrack::Rotation {
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
        E::Blend(kasane_sdk::BlendAuthoringEdit::CreateBinding {
            id: "00000000-0000-4000-8000-000000000409".into(),
            target_id: R.into(),
            target_kind: kasane_core::BlendShapeTargetKind::Rotation,
            key_table_id: String::new(),
        }),
    );
    let before = frame(&s, 0.5, 0.).drawables[0].positions.clone();
    apply(
        &mut s,
        parent_edit(ReparentStrategy::PreserveParameterDomain, R),
    );
    for (a, b) in frame(&s, 0.5, 0.).drawables[0].positions.iter().zip(before) {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
    }
}
