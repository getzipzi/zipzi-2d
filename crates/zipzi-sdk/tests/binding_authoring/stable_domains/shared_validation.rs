use super::*;

#[test]
fn pending_blend_constraint_entries_pair_weights_by_id_and_export_explicitly() {
    let mut s = base();
    let table = id(1710);
    let b = id(1711);
    let c = id(1712);
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: table.clone(),
                parameter_id: String::new(),
                keys: vec![],
                base_key_idx: 0,
                samples: vec![],
                base_sample_id: String::new(),
            },
        }),
    );
    apply(
        &mut s,
        E::Blend(B::CreateBinding {
            id: b.clone(),
            target_id: MESH_ID.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: String::new(),
        }),
    );
    let original = frame(&s, 0.5, 0.).drawables[0].positions.clone();
    assert_eq!(frame(&s, 0.5, 0.).drawables[0].positions, original);
    roundtrip(&s);
    let projection = s.snapshot().document().available_binding_export().unwrap();
    assert!(projection.blend_binding(&b).is_none());
    let out = zipzi_project::encode_document_cmo3(
        s.snapshot().document(),
        s.snapshot().resources(),
        &zipzi_project::Cmo3Options {
            loss_policy: zipzi_project::LossPolicy::Allow,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(out.report.diagnostics.iter().any(|d| d.object_id == b));
    apply(
        &mut s,
        E::Blend(B::CreateConstraint {
            constraint: BlendShapeConstraint {
                id: c.clone(),
                parameter_id: String::new(),
                keys: vec![],
                weights: vec![],
                samples: vec![],
            },
        }),
    );
    apply(
        &mut s,
        E::Blend(B::Constraint {
            id: c.clone(),
            parameter_id: PARAMETER_ID.into(),
            samples: vec![
                ConstraintSample {
                    id: id(1713),
                    value: 0.,
                    weight: 0.2,
                },
                ConstraintSample {
                    id: id(1714),
                    value: 1.,
                    weight: 0.9,
                },
            ],
        }),
    );
    apply(
        &mut s,
        E::Blend(B::Constraint {
            id: c.clone(),
            parameter_id: PARAMETER_ID.into(),
            samples: vec![
                ConstraintSample {
                    id: id(1713),
                    value: 0.4,
                    weight: 0.3,
                },
                ConstraintSample {
                    id: id(1714),
                    value: 1.,
                    weight: 0.9,
                },
            ],
        }),
    );
    let c = s.snapshot().document().blend_constraint(&c).unwrap();
    assert_eq!(c.samples[0].id, id(1713));
    assert_eq!(c.keys, vec![0.4, 1.]);
    assert_eq!(c.weights, vec![0.3, 0.9]);
    roundtrip(&s);
}

#[test]
fn shared_rebase_constraints_locks_limits_and_missing_base_reject_without_history() {
    let mut s = shared();
    let table = id(1701);
    let t = s
        .snapshot()
        .document()
        .blend_key_table(&table)
        .unwrap()
        .clone();
    let before = s.version();
    let h = s.history_state();
    assert_eq!(
        crate::support::binding(
            &s,
            E::Blend(B::Table {
                id: table.clone(),
                operation: BO::Delete {
                    sample_id: t.base_sample_id.clone(),
                    replacement_base: None
                }
            })
        )
        .unwrap_err()
        .code
        .as_str(),
        "BASE_REPLACEMENT_REQUIRED"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
    let p = crate::support::binding(
        &s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: id(1720),
                    value: 0.5,
                },
            },
        }),
    )
    .unwrap();
    let limits = zipzi_sdk::ConversionLimits {
        max_workspace_bytes: p.0.work.workspace_bytes - 1,
        ..Default::default()
    };
    assert_eq!(
        crate::support::binding_with_control(
            &s,
            E::Blend(B::Table {
                id: table.clone(),
                operation: BO::Insert {
                    sample: BindingSample {
                        id: id(1720),
                        value: 0.5
                    }
                }
            }),
            limits,
            &Default::default()
        )
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
    let c = id(1721);
    apply(
        &mut s,
        E::Blend(B::CreateConstraint {
            constraint: BlendShapeConstraint {
                id: c.clone(),
                parameter_id: PARAMETER_ID.into(),
                keys: vec![0., 1.],
                weights: vec![0.5, 1.],
                samples: vec![],
            },
        }),
    );
    let mut b = s
        .snapshot()
        .document()
        .blend_binding(&id(1702))
        .unwrap()
        .clone();
    b.constraint_ids.push(c);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_blend_binding(b))
            .and_then(|value| {
                let prepared = draft.prepare("constrain")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.version();
    let h = s.history_state();
    assert_eq!(
        crate::support::binding(
            &s,
            E::Blend(B::Table {
                id: table.clone(),
                operation: BO::SetBase {
                    sample_id: t.samples[2].id.clone()
                }
            })
        )
        .unwrap_err()
        .code
        .as_str(),
        "CONSTRAINED_REBASE_NOT_REPRESENTABLE"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
}

#[test]
fn noop_shared_edits_preserve_redo_and_native_invalid_ids_are_rejected() {
    let mut s = shared();
    let t = s
        .snapshot()
        .document()
        .blend_key_table(&id(1701))
        .unwrap()
        .clone();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: t.id.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: id(1900),
                    value: 0.5,
                },
            },
        }),
    );
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let receipt = apply(
        &mut s,
        E::Blend(B::Table {
            id: t.id.clone(),
            operation: BO::Move {
                sample_id: t.samples[0].id.clone(),
                value: t.samples[0].value,
            },
        }),
    );
    assert!(receipt.writes.is_empty());
    assert_eq!(s.history_state(), history);
    let bytes = crate::support::archive::native_bytes_with_resources(
        s.snapshot().document(),
        s.snapshot().resources(),
    );
    let mut wire = crate::support::archive::project_json(&bytes);
    wire["document"]["bindings"][0]["axes"][0]["id"] = serde_json::json!("");
    assert!(
        crate::support::archive::load_native(&crate::support::archive::replace_project_json(
            &bytes, &wire
        ))
        .is_err()
    );
}

#[test]
fn conversion_preparation_enforces_limits_and_publishes_one_validated_change() {
    let mut s = base();
    let axis = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .axes[0]
        .id
        .clone();
    let intent = E::Grid(G::Edit {
        owner: BINDING_ID.into(),
        operation: O::InsertSample {
            axis_id: axis.clone(),
            id: id(1980),
            value: 0.5,
        },
    });
    let plan = crate::support::binding(&s, intent.clone()).unwrap();
    let before = s.version();
    let h = s.history_state();
    assert_eq!(
        crate::support::binding_with_control(
            &s,
            intent,
            zipzi_sdk::ConversionLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            &Default::default()
        )
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
    s.publish(plan.1).unwrap();
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        id(1980),
        zipzi_core::document::ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(flags))
            .and_then(|value| {
                let prepared = draft.prepare("hide child")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::DeleteSample {
            sample_id: id(1980),
        },
    );
    assert!(!s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .contains_key(&id(1980)));
    s.undo(s.version()).unwrap();
    assert!(
        s.snapshot()
            .document()
            .editor_state()
            .get(&id(1980))
            .hidden_in_editor
    );
}

#[test]
fn shared_conversion_report_and_partial_work_cancellation_cover_all_users() {
    let mut s = shared();
    let table = s
        .snapshot()
        .document()
        .blend_key_table(&id(1701))
        .unwrap()
        .clone();
    let intent = E::Blend(B::Table {
        id: table.id.clone(),
        operation: BO::SetBase {
            sample_id: table.samples[2].id.clone(),
        },
    });
    let plan = crate::support::binding(&s, intent.clone()).unwrap();
    assert!(plan.0.report.preserved.iter().any(|a| a.owner == id(1702)));
    assert!(plan.0.report.preserved.iter().any(|a| a.owner == id(1703)));
    assert!(plan
        .0
        .report
        .remapped
        .iter()
        .any(|r| r.before.is_base && !r.after.is_base));
    let version = s.version();
    let history = s.history_state();
    s.take_events();
    let control = zipzi_sdk::ConversionControl::with_progress(|n| n < 5);
    assert_eq!(
        crate::support::binding_with_control(&s, intent, Default::default(), &control)
            .unwrap_err()
            .code
            .as_str(),
        "AUTHORING_CANCELLED"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s.take_events().is_empty());
    assert_eq!(
        s.snapshot().document().blend_key_table(&table.id),
        Some(&table)
    );
}
