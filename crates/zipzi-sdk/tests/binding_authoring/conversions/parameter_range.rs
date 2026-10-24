use super::*;

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
fn range_maps_scene_glue_constraints_and_all_shared_blend_users() {
    use zipzi_core::*;
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
    // Both policies affect the owning scene and Offscreen selection, even if
    // no numeric key needs rewriting. All such dependencies honor locks.
    for policy in [
        ParameterRangePolicy::KeepKeyValues,
        ParameterRangePolicy::MapKeyValues,
    ] {
        let edit = E::ParameterRange(ParameterRangeEdit {
            parameter_id: PARAMETER_ID.into(),
            minimum: -1.,
            maximum: 2.,
            default_value: 0.,
            policy,
        });
        let plan = s.plan_binding_authoring(edit.clone()).unwrap();
        for owner in [MESH_ID.to_string(), R.into(), id(530), id(532)] {
            assert!(plan.report().affected.contains(&owner), "missing {owner}");
            assert!(plan.report().preserved.iter().any(|a| a.owner == owner));
        }
        let mut flags = s.document().editor_state().clone();
        flags.objects.insert(
            id(532),
            zipzi_core::document::ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        s.edit("lock offscreen", None, |e| e.replace_editor_state(flags))
            .unwrap();
        let version = s.version();
        assert_eq!(
            s.plan_binding_authoring(edit).unwrap_err().code.as_ref(),
            "OBJECT_LOCKED"
        );
        assert_eq!(s.version(), version);
        s.undo().unwrap();
    }
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
        zipzi_core::document::ObjectEditorState {
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
