use super::*;
use kasane_core::{RotationPose, RotationTransform, Transform, TransformData};
use kasane_sdk::{
    AuthoringLimits, AxisReduction, BindingAuthoringEdit as E, BindingConversion,
    MeshGridEdit as G, ParameterRangeEdit, ParameterRangePolicy, ReparentEdit, ReparentStrategy,
};
const Q: &str = "00000000-0000-4000-8000-000000000406";
const R: &str = "00000000-0000-4000-8000-000000000407";
const T: &str = "00000000-0000-4000-8000-000000000408";
fn apply(s: &mut AuthoringSession, e: E) -> kasane_sdk::EditReceipt {
    let p = s.plan_binding_authoring(e).unwrap();
    s.execute_binding_plan(&p).unwrap()
}
fn bound() -> AuthoringSession {
    let mut s = fixture();
    let b = binding(&s);
    s.edit("bind", None, |e| {
        e.create_parameter(parameter(false))?;
        e.create_binding(b)
    })
    .unwrap();
    s.drain_events();
    s
}
fn frame(s: &AuthoringSession, p: f32, q: f32) -> kasane_core::DrawableFrame {
    s.evaluate(
        &[(PARAMETER_ID, p), (Q, q)]
            .into_iter()
            .filter(|(id, _)| s.parameter(id).is_some())
            .map(|(id, v)| (id.into(), v))
            .collect(),
    )
    .unwrap()
}
fn grid_roundtrip(s: &AuthoringSession) {
    let mut document = s.document().fork_candidate();
    let mut asset = document.get_asset(ASSET_ID).unwrap().clone();
    asset.source = "assets/texture.png".into();
    assert!(document.replace_asset(asset).status.is_ok());
    let text = kasane_project::encode_project(&document).unwrap();
    let restored = kasane_project::decode_project(&text).unwrap();
    assert!(restored.validate_structure().is_empty());
    assert_eq!(
        restored.get_binding(BINDING_ID),
        s.document().get_binding(BINDING_ID)
    );
    assert_eq!(restored.get_mesh(MESH_ID), s.document().get_mesh(MESH_ID));
}
#[test]
fn zero_axis_uses_latest_base_and_last_axis_writes_selected_shape_back() {
    let mut s = fixture();
    apply(
        &mut s,
        E::MeshGrid(G::Create {
            id: BINDING_ID.into(),
            mesh_id: MESH_ID.into(),
        }),
    );
    let b = s.binding(BINDING_ID).unwrap();
    assert!(b.axes.is_empty() && b.keyforms.is_empty());
    let mut m = s.document().get_mesh(MESH_ID).unwrap().clone();
    m.base_positions.iter_mut().for_each(|p| p.x += 3.0);
    m.appearance.opacity = 0.6;
    s.edit("base", None, |e| {
        e.replace_mesh(m)?;
        e.create_parameter(parameter(false))
    })
    .unwrap();
    assert_eq!(frame(&s, 0.0, 0.0).drawables[0].positions[0].x, 43.0);
    grid_roundtrip(&s);
    apply(
        &mut s,
        E::MeshGrid(G::AddAxis {
            binding_id: BINDING_ID.into(),
            axis: BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: PARAMETER_ID.into(),
                keys: vec![0.0, 1.0],
            },
        }),
    );
    assert!(s
        .binding(BINDING_ID)
        .unwrap()
        .keyforms
        .iter()
        .all(|f| f.positions[0].x == 43.0 && f.appearance.opacity == 0.6));
    let mut f = s.binding(BINDING_ID).unwrap().keyforms[1].clone();
    f.positions.iter_mut().for_each(|p| p.y += 7.0);
    s.edit("shape", None, |e| e.set_mesh_keyform(BINDING_ID, f))
        .unwrap();
    let expected = frame(&s, 0.25, 0.0).drawables[0].clone();
    apply(
        &mut s,
        E::MeshGrid(G::RemoveAxis {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            reduction: AxisReduction::Resample(0.25),
        }),
    );
    let actual = frame(&s, 0.0, 0.0).drawables[0].clone();
    assert_eq!(actual.positions, expected.positions);
    assert_eq!(actual.opacity, expected.opacity);
    assert!(s.binding(BINDING_ID).unwrap().keyforms.is_empty());
    grid_roundtrip(&s);
    s.undo().unwrap();
    assert_eq!(s.binding(BINDING_ID).unwrap().keyforms.len(), 2);
    s.redo().unwrap();
    assert_eq!(
        frame(&s, 1.0, 0.0).drawables[0].positions,
        expected.positions
    );
}
#[test]
fn expand_three_distinct_shapes_and_reorder_preserve_old_domain() {
    let mut s = bound();
    apply(
        &mut s,
        E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5,
        }),
    );
    let mut f = s.binding(BINDING_ID).unwrap().keyforms[1].clone();
    f.positions.iter_mut().for_each(|p| p.y += 8.0);
    f.appearance.opacity = 0.25;
    f.draw_order = Some(8.0);
    s.edit("middle shape", None, |e| e.set_mesh_keyform(BINDING_ID, f))
        .unwrap();
    s.edit("new driver", None, |e| {
        e.create_parameter(Parameter {
            id: Q.into(),
            ..Parameter::default()
        })
    })
    .unwrap();
    let before: Vec<_> = (0..=20)
        .map(|i| frame(&s, i as f32 / 20., 0.).drawables[0].clone())
        .collect();
    let old_forms = s.binding(BINDING_ID).unwrap().keyforms;
    apply(
        &mut s,
        E::MeshGrid(G::AddAxis {
            binding_id: BINDING_ID.into(),
            axis: BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: Q.into(),
                keys: vec![-1.0, 0.0, 1.0],
            },
        }),
    );
    let b = s.binding(BINDING_ID).unwrap();
    assert_eq!(b.keyforms.len(), 9);
    for chunk in b.keyforms.chunks(3) {
        for (f, old) in chunk.iter().zip(&old_forms) {
            assert_eq!(f.positions, old.positions);
            assert_eq!(f.appearance, old.appearance);
            assert_eq!(f.draw_order, old.draw_order);
        }
    }
    apply(
        &mut s,
        E::MeshGrid(G::ReorderAxes {
            binding_id: BINDING_ID.into(),
            parameter_ids: vec![Q.into(), PARAMETER_ID.into()],
        }),
    );
    for (i, expected) in before.iter().enumerate() {
        for q in [-1.0, -0.65, 0.0, 0.23, 1.0] {
            let actual = &frame(&s, i as f32 / 20., q).drawables[0];
            for (a, b) in actual.positions.iter().zip(&expected.positions) {
                assert!((a.x - b.x).abs() < 2e-5 && (a.y - b.y).abs() < 2e-5);
            }
            assert!((actual.opacity - expected.opacity).abs() < 1e-6);
            assert_eq!(actual.draw_order, expected.draw_order);
        }
    }
    grid_roundtrip(&s);
}
#[test]
fn insert_move_delete_and_slice_are_explicit_and_atomic() {
    let mut s = bound();
    let expected: Vec<_> = (0..=10).map(|i| sample(&s, i as f32 / 10.)).collect();
    apply(
        &mut s,
        E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.4,
        }),
    );
    for (i, expected) in expected.iter().enumerate() {
        assert!((sample(&s, i as f32 / 10.).1 - expected.1).abs() < 1e-5);
    }
    apply(
        &mut s,
        E::MeshGrid(G::MoveSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            from: 0.4,
            to: 0.6,
        }),
    );
    assert_eq!(
        s.binding(BINDING_ID).unwrap().keyforms[1].positions[0].x,
        44.0
    );
    assert_eq!(s.binding(BINDING_ID).unwrap().keyforms[1].keys, vec![0.6]);
    let before = s.version();
    let bad = s
        .plan_binding_authoring(E::MeshGrid(G::MoveSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            from: 0.6,
            to: 1.0,
        }))
        .unwrap_err();
    assert_eq!(bad.code.as_ref(), "INVALID_KEYS");
    assert_eq!(s.version(), before);
    apply(
        &mut s,
        E::MeshGrid(G::DeleteSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.6,
        }),
    );
    assert_eq!(s.binding(BINDING_ID).unwrap().keyforms.len(), 2);
    let before = s.version();
    assert!(s
        .plan_binding_authoring(E::MeshGrid(G::RemoveAxis {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            reduction: AxisReduction::Slice(0.5)
        }))
        .is_err());
    assert_eq!(s.version(), before);
    apply(
        &mut s,
        E::MeshGrid(G::RemoveAxis {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            reduction: AxisReduction::Slice(1.0),
        }),
    );
    assert_eq!(frame(&s, 0.0, 0.0).drawables[0].positions[0].x, 50.0);
}
#[test]
fn range_and_grid_final_state_succeeds_when_both_setter_orders_fail() {
    let mut s = bound();
    let mut new_p = s.parameter(PARAMETER_ID).unwrap();
    new_p.minimum = 10.0;
    new_p.maximum = 20.0;
    new_p.default_value = 15.0;
    let mut new_b = s.binding(BINDING_ID).unwrap();
    new_b.axes[0].set_values(vec![10.0, 20.0]);
    new_b.keyforms[0].keys = vec![10.0];
    new_b.keyforms[1].keys = vec![20.0];
    let before = s.version();
    assert!(s
        .edit("parameter first", None, |e| {
            e.replace_parameter(new_p.clone())?;
            e.replace_binding(new_b.clone())
        })
        .is_err());
    assert!(s
        .edit("grid first", None, |e| {
            e.replace_binding(new_b.clone())?;
            e.replace_parameter(new_p.clone())
        })
        .is_err());
    assert_eq!(s.version(), before);
    let receipt = apply(
        &mut s,
        E::ParameterRange(ParameterRangeEdit {
            parameter_id: PARAMETER_ID.into(),
            minimum: 10.0,
            maximum: 20.0,
            default_value: 15.0,
            policy: ParameterRangePolicy::MapKeyValues,
        }),
    );
    assert_eq!(receipt.after.revision, before.revision + 1);
    assert_eq!(s.parameter(PARAMETER_ID), Some(new_p));
    assert_eq!(s.binding(BINDING_ID), Some(new_b));
    assert_eq!(sample(&s, 15.0).1, 45.0);
    grid_roundtrip(&s);
    s.undo().unwrap();
    assert_eq!(sample(&s, 0.5).1, 45.0);
    s.redo().unwrap();
    assert_eq!(sample(&s, 15.0).1, 45.0);
}
#[test]
fn limits_stale_plans_cancel_and_noop_leave_history_unchanged() {
    let mut s = bound();
    s.edit("another parameter", None, |e| {
        e.create_parameter(Parameter {
            id: Q.into(),
            ..Default::default()
        })
    })
    .unwrap();
    s.undo().unwrap();
    let history = s.history_state();
    let noop = E::MeshGrid(G::ReorderAxes {
        binding_id: BINDING_ID.into(),
        parameter_ids: vec![PARAMETER_ID.into()],
    });
    assert!(!apply(&mut s, noop.clone()).changed);
    assert_eq!(s.history_state(), history);
    let version = s.version();
    drop(s.plan_binding_authoring(noop.clone()).unwrap());
    assert_eq!(s.version(), version);
    let old_plan = s.plan_binding_authoring(noop).unwrap();
    let mut limits = AuthoringLimits::default();
    limits.max_sample_combinations = 1;
    s.set_authoring_limits(limits).unwrap();
    assert_eq!(
        s.execute_binding_plan(&old_plan).unwrap_err().code.as_ref(),
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert_eq!(
        AuthoringLimits::default()
            .check_grid(&[usize::MAX, 2], 1, 0)
            .unwrap_err()
            .code,
        "AUTHORING_CAPACITY_OVERFLOW"
    );
    s.set_authoring_limits(AuthoringLimits::default()).unwrap();
    s.redo().unwrap();
    assert!(s.execute_binding_plan(&old_plan).is_err());
    let p = s
        .plan_binding_authoring(E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5,
        }))
        .unwrap();
    let mut edit = s.begin_edit("changed candidate baseline", None).unwrap();
    let mut q = edit.candidate_document().get_parameter(Q).unwrap().clone();
    q.name = "rename".into();
    edit.replace_parameter(q).unwrap();
    assert_eq!(
        edit.apply_binding_plan(&p).unwrap_err().code.as_ref(),
        "STALE_PLAN"
    );
    assert!(edit.commit().is_err());
}
fn rotation(s: &mut AuthoringSession, scale: f32) {
    s.edit("rotation", None, |e| {
        e.create_transform(Transform {
            id: R.into(),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 12.0,
                pose: RotationPose {
                    angle: 23.0,
                    scale,
                    origin: kasane_core::PreciseVec2::new(5.0, -3.0),
                    reflect_x: true,
                    ..Default::default()
                },
            }),
            ..Default::default()
        })
    })
    .unwrap();
}
fn parent_edit(strategy: ReparentStrategy, parent: &str) -> E {
    E::Reparent(ReparentEdit {
        sources: vec![MESH_ID.into()],
        parent_id: parent.into(),
        strategy,
        tolerance: 0.001,
    })
}
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
fn locked_target_blocks_grid_range_and_direct_replacement() {
    let mut s = bound();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        MESH_ID.into(),
        kasane_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let version = s.version();
    assert_eq!(
        s.plan_binding_authoring(E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(
        s.plan_binding_authoring(E::ParameterRange(ParameterRangeEdit {
            parameter_id: PARAMETER_ID.into(),
            minimum: 10.0,
            maximum: 20.0,
            default_value: 15.0,
            policy: ParameterRangePolicy::MapKeyValues
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    let b = s.binding(BINDING_ID).unwrap();
    assert!(s.edit("bypass", None, |e| e.replace_binding(b)).is_err());
    assert_eq!(s.version(), version);
}
#[test]
fn range_maps_scene_glue_constraints_and_all_shared_blend_users() {
    use kasane_core::*;
    fn id(n: u128) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }
    let mut s = bound();
    rotation(&mut s, 1.0);
    s.edit("dependencies", None, |e| {
        e.create_scene_binding(SceneBinding {
            id: T.into(),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: PARAMETER_ID.into(),
                keys: vec![0.0, 1.0],
            }],
            track: SceneTrack::Rotation {
                target_id: R.into(),
                keyforms: vec![
                    RotationKeyform {
                        keys: vec![0.0],
                        ..Default::default()
                    },
                    RotationKeyform {
                        keys: vec![1.0],
                        ..Default::default()
                    },
                ],
            },
        })?;
        e.create_glue(Glue {
            id: Q.into(),
            runtime_id: "glue".into(),
            name: "Glue".into(),
            mesh_a_id: MESH_ID.into(),
            mesh_b_id: MESH_ID.into(),
            pairs: vec![],
            intensity: 0.0,
            binding: Some(GlueBinding {
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: PARAMETER_ID.into(),
                    keys: vec![0.0, 1.0],
                }],
                keyforms: vec![
                    GlueKeyform {
                        samples: Default::default(),
                        intensity: 0.1,
                    },
                    GlueKeyform {
                        samples: Default::default(),
                        intensity: 0.6,
                    },
                ],
            }),
        })?;
        e.create_part(Part {
            id: id(530),
            runtime_id: "part".into(),
            ..Default::default()
        })?;
        e.create_scene_binding(SceneBinding {
            id: id(531),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: PARAMETER_ID.into(),
                keys: vec![0.0, 1.0],
            }],
            track: SceneTrack::Part {
                target_id: id(530).into(),
                keyforms: vec![
                    PartKeyform {
                        samples: Default::default(),
                        keys: vec![0.0],
                        draw_order: 0.0,
                    },
                    PartKeyform {
                        samples: Default::default(),
                        keys: vec![1.0],
                        draw_order: 10.0,
                    },
                ],
            },
        })?;
        e.create_offscreen(Offscreen {
            id: id(532),
            runtime_id: "offscreen".into(),
            part_id: id(530),
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
        })?;
        e.create_blend_constraint(BlendShapeConstraint {
            samples: Default::default(),
            id: id(520),
            parameter_id: PARAMETER_ID.into(),
            keys: vec![0.0, 1.0],
            weights: vec![0.2, 1.0],
        })?;
        e.create_parameter(Parameter {
            id: id(521),
            minimum: -1.0,
            maximum: 1.0,
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: id(522),
            parameter_id: id(521),
            keys: vec![-1.0, 0.0, 1.0],
            base_key_idx: 1,
        })?;
        for n in [523, 524] {
            e.create_blend_binding(BlendShapeBinding {
                id: id(n),
                target_id: MESH_ID.into(),
                target_kind: BlendShapeTargetKind::Mesh,
                key_table_id: id(522),
                constraint_ids: vec![id(520)],
                keyforms: DeltaKeyforms::Mesh(vec![
                    DeltaMeshKeyform {
                        positions: vec![Vec2::default(); 4],
                        ..Default::default()
                    };
                    3
                ]),
            })?;
        }
        Ok(())
    })
    .unwrap();
    let weight = s
        .document()
        .get_blend_constraint(&id(520))
        .unwrap()
        .weights
        .clone();
    let deltas = s
        .document()
        .get_blend_binding(&id(523))
        .unwrap()
        .keyforms
        .clone();
    let intent = |parameter_id: &str| {
        E::ParameterRange(ParameterRangeEdit {
            parameter_id: parameter_id.into(),
            minimum: 10.0,
            maximum: 20.0,
            default_value: 15.0,
            policy: ParameterRangePolicy::MapKeyValues,
        })
    };
    let receipt = apply(&mut s, intent(PARAMETER_ID));
    assert!(receipt.object_ids.contains(&id(523)) && receipt.object_ids.contains(&id(524)));
    assert_eq!(
        s.document()
            .get_scene_binding(T)
            .unwrap()
            .track
            .sample(1)
            .keys,
        &[20.0]
    );
    assert_eq!(
        s.document()
            .get_glue(Q)
            .unwrap()
            .binding
            .as_ref()
            .unwrap()
            .axes[0]
            .keys,
        vec![10.0, 20.0]
    );
    assert_eq!(
        s.document().get_blend_constraint(&id(520)).unwrap().keys,
        vec![10.0, 20.0]
    );
    assert_eq!(
        s.document().get_blend_constraint(&id(520)).unwrap().weights,
        weight
    );
    apply(&mut s, intent(&id(521)));
    let t = s.document().get_blend_key_table(&id(522)).unwrap();
    assert_eq!(t.keys, vec![10.0, 15.0, 20.0]);
    assert_eq!(t.base_key_idx, 1);
    assert_eq!(
        s.document()
            .get_scene_binding(&id(531))
            .unwrap()
            .track
            .sample(1)
            .keys,
        &[20.0]
    );
    assert_eq!(
        s.document()
            .get_offscreen(&id(532))
            .unwrap()
            .part_keyform_indices,
        vec![1, 0]
    );
    assert_eq!(
        s.document().get_blend_binding(&id(523)).unwrap().keyforms,
        deltas
    );
    assert!(s.document().validate_structure().is_empty());
    s.undo().unwrap();
    s.undo().unwrap();
    assert_eq!(
        s.document()
            .get_scene_binding(T)
            .unwrap()
            .track
            .sample(1)
            .keys,
        &[1.0]
    );
    s.redo().unwrap();
    s.redo().unwrap();
    assert!(s.document().validate_structure().is_empty());
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        id(524),
        kasane_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock shared user", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let before = s.version();
    let error = s
        .plan_binding_authoring(E::ParameterRange(ParameterRangeEdit {
            parameter_id: id(521),
            minimum: 30.0,
            maximum: 40.0,
            default_value: 35.0,
            policy: ParameterRangePolicy::MapKeyValues,
        }))
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "OBJECT_LOCKED");
    assert_eq!(s.version(), before);
}
#[test]
fn joint_range_rejects_collapsed_float_samples() {
    let mut s = bound();
    let mut b = s.binding(BINDING_ID).unwrap();
    b.axes[0].set_values(vec![0.0, f32::EPSILON]);
    b.keyforms[1].keys = vec![f32::EPSILON];
    s.edit("close keys", None, |e| e.replace_binding(b))
        .unwrap();
    let before = s.version();
    let error = s
        .plan_binding_authoring(E::ParameterRange(ParameterRangeEdit {
            parameter_id: PARAMETER_ID.into(),
            minimum: 100_000_000.0,
            maximum: 100_000_016.0,
            default_value: 100_000_000.0,
            policy: ParameterRangePolicy::MapKeyValues,
        }))
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "INVALID_KEYS");
    assert_eq!(s.version(), before);
    let doc = s.document();
    assert_eq!(doc.get_parameter(PARAMETER_ID).unwrap().minimum, 0.0);
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
fn each_budget_boundary_is_checked_without_allocating_a_grid() {
    let cost = AuthoringLimits::default()
        .check_grid(&[5, 20], 64, 512)
        .unwrap();
    assert_eq!(cost.sample_combinations, 100);
    assert_eq!(cost.payload_bytes, 6400);
    let exact = AuthoringLimits {
        max_sample_combinations: 100,
        max_conversion_bytes: cost.temporary_bytes,
        max_history_bytes: cost.history_bytes,
    };
    assert_eq!(exact.check_grid(&[5, 20], 64, 512).unwrap(), cost);
    assert_eq!(
        AuthoringLimits {
            max_sample_combinations: 99,
            ..exact
        }
        .check_grid(&[5, 20], 64, 512)
        .unwrap_err()
        .code,
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(
        AuthoringLimits {
            max_conversion_bytes: cost.temporary_bytes - 1,
            ..exact
        }
        .check_grid(&[5, 20], 64, 512)
        .unwrap_err()
        .code,
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(
        AuthoringLimits {
            max_history_bytes: cost.history_bytes - 1,
            ..exact
        }
        .check_grid(&[5, 20], 64, 512)
        .unwrap_err()
        .code,
        "AUTHORING_HISTORY_LIMIT"
    );
    assert_eq!(
        exact.check_grid(&[0], 64, 512).unwrap_err().code,
        "INVALID_KEYS"
    );
}
#[test]
fn zero_axis_external_export_requires_explicit_acceptance_of_omission() {
    let mut s = fixture();
    apply(
        &mut s,
        E::MeshGrid(G::Create {
            id: BINDING_ID.into(),
            mesh_id: MESH_ID.into(),
        }),
    );
    let error = s
        .encode_cmo3(&kasane_sdk::Cmo3Options::default())
        .unwrap_err();
    let kasane_sdk::Cmo3Error::Unsupported(report) = error else {
        panic!("{error:?}");
    };
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.object_id == BINDING_ID && d.code == "ZERO_AXIS_BINDING"));
    let artifact = s
        .encode_cmo3(&kasane_sdk::Cmo3Options {
            loss_policy: kasane_sdk::LossPolicy::Allow,
            ..Default::default()
        })
        .unwrap();
    assert!(!artifact.bytes.is_empty());
    assert!(artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "ZERO_AXIS_BINDING"));
    // The native authoring object remains present after either export path.
    assert!(s.binding(BINDING_ID).unwrap().keyforms.is_empty());
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
