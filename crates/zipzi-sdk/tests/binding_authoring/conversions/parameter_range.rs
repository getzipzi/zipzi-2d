use super::*;
use zipzi_core::BindingAxisEditing;

#[test]
fn range_conversion_handles_dependency_validation_without_setter_ordering() {
    let mut s = bound();
    let mut new_p = s
        .snapshot()
        .document()
        .parameter(PARAMETER_ID)
        .cloned()
        .unwrap();
    new_p.minimum = 10.0;
    new_p.maximum = 20.0;
    new_p.default_value = 15.0;
    let mut new_b = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    new_b.axes[0].set_values(vec![10.0, 20.0]);
    new_b.keyforms[0].keys = vec![10.0];
    new_b.keyforms[1].keys = vec![20.0];
    let before = s.version();
    for parameter_first in [true, false] {
        let result = crate::support::prepared(&s, |e| {
            if parameter_first {
                e.replace_parameter(new_p.clone())?;
                e.replace_binding(new_b.clone())
            } else {
                e.replace_binding(new_b.clone())?;
                e.replace_parameter(new_p.clone())
            }
        });
        if !parameter_first {
            assert!(result.is_err());
            continue;
        }
        let (_, prepared) = result.unwrap();
        assert_eq!(
            prepared.snapshot().document().parameter(PARAMETER_ID),
            Some(&new_p)
        );
        assert_eq!(
            prepared.snapshot().document().binding(BINDING_ID),
            Some(&new_b)
        );
    }
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
    assert_eq!(receipt.version.revision, before.revision + 1);
    assert_eq!(
        s.snapshot().document().parameter(PARAMETER_ID).cloned(),
        Some(new_p)
    );
    assert_eq!(
        s.snapshot().document().binding(BINDING_ID).cloned(),
        Some(new_b)
    );
    assert_eq!(sample(&s, 15.0).1, 45.0);
    grid_roundtrip(&s);
    s.undo(s.version()).unwrap();
    assert_eq!(sample(&s, 0.5).1, 45.0);
    s.redo(s.version()).unwrap();
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
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
                crate::support::insert_glue(
                    e,
                    Glue {
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
                    },
                )?;
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
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
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
                    },
                )?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateConstraint {
                        constraint: BlendShapeConstraint {
                            samples: Default::default(),
                            id: id(520),
                            parameter_id: PARAMETER_ID.into(),
                            keys: vec![0.0, 1.0],
                            weights: vec![0.2, 1.0],
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(521),
                        minimum: -1.0,
                        maximum: 1.0,
                        kind: ParameterKind::BlendShape,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateTable {
                        table: BlendShapeKeyTable {
                            samples: Default::default(),
                            base_sample_id: Default::default(),
                            id: id(522),
                            parameter_id: id(521),
                            keys: vec![-1.0, 0.0, 1.0],
                            base_key_idx: 1,
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                for n in [523, 524] {
                    {
                        let binding = BlendShapeBinding {
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
                let prepared = draft.prepare("dependencies")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let weight = s
        .snapshot()
        .document()
        .blend_constraint(&id(520))
        .unwrap()
        .weights
        .clone();
    let deltas = s
        .snapshot()
        .document()
        .blend_binding(&id(523))
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
        let plan = crate::support::binding(&s, edit.clone()).unwrap();
        for owner in [MESH_ID.to_string(), R.into(), id(530), id(532)] {
            assert!(plan.0.report.affected.contains(&owner), "missing {owner}");
            assert!(plan.0.report.preserved.iter().any(|a| a.owner == owner));
        }
        let mut flags = s.snapshot().document().editor_state().clone();
        flags.objects.insert(
            id(532),
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
                    let prepared = draft.prepare("lock offscreen")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let version = s.version();
        assert_eq!(
            crate::support::binding(&s, edit).unwrap_err().code.as_str(),
            "OBJECT_LOCKED"
        );
        assert_eq!(s.version(), version);
        s.undo(s.version()).unwrap();
    }
    let receipt = apply(&mut s, intent(PARAMETER_ID));
    assert!(receipt
        .writes
        .records()
        .iter()
        .any(|w| w.id == PARAMETER_ID));
    assert_eq!(
        s.snapshot()
            .document()
            .scene_binding(T)
            .unwrap()
            .track
            .sample(1)
            .keys,
        &[20.0]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .glue(Q)
            .unwrap()
            .binding
            .as_ref()
            .unwrap()
            .axes[0]
            .keys,
        vec![10.0, 20.0]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .blend_constraint(&id(520))
            .unwrap()
            .keys,
        vec![10.0, 20.0]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .blend_constraint(&id(520))
            .unwrap()
            .weights,
        weight
    );
    apply(&mut s, intent(&id(521)));
    let t = s.snapshot().document().blend_key_table(&id(522)).unwrap();
    assert_eq!(t.keys, vec![10.0, 15.0, 20.0]);
    assert_eq!(t.base_key_idx, 1);
    assert_eq!(
        s.snapshot()
            .document()
            .scene_binding(&id(531))
            .unwrap()
            .track
            .sample(1)
            .keys,
        &[20.0]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(532))
            .unwrap()
            .part_keyform_indices,
        vec![1, 0]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .blend_binding(&id(523))
            .unwrap()
            .keyforms,
        deltas
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .scene_binding(T)
            .unwrap()
            .track
            .sample(1)
            .keys,
        &[1.0]
    );
    s.redo(s.version()).unwrap();
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().builder().freeze().is_ok());
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        id(524),
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
                let prepared = draft.prepare("lock shared user")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.version();
    let error = crate::support::binding(
        &s,
        E::ParameterRange(ParameterRangeEdit {
            parameter_id: id(521),
            minimum: 30.0,
            maximum: 40.0,
            default_value: 35.0,
            policy: ParameterRangePolicy::MapKeyValues,
        }),
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "OBJECT_LOCKED");
    assert_eq!(s.version(), before);
}

#[test]
fn joint_range_rejects_collapsed_float_samples() {
    let mut s = bound();
    let mut b = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    b.axes[0].set_values(vec![0.0, f32::EPSILON]);
    b.keyforms[1].keys = vec![f32::EPSILON];
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_binding(b))
            .and_then(|value| {
                let prepared = draft.prepare("close keys")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.version();
    let error = crate::support::binding(
        &s,
        E::ParameterRange(ParameterRangeEdit {
            parameter_id: PARAMETER_ID.into(),
            minimum: 100_000_000.0,
            maximum: 100_000_016.0,
            default_value: 100_000_000.0,
            policy: ParameterRangePolicy::MapKeyValues,
        }),
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "INVALID_KEYS");
    assert_eq!(s.version(), before);
    let doc = s.snapshot().document();
    assert_eq!(doc.parameter(PARAMETER_ID).unwrap().minimum, 0.0);
}
