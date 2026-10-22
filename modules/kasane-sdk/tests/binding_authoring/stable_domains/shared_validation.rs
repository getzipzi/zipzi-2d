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
    let plan = s.plan_available_binding_export();
    assert!(plan.omitted.iter().any(|o| o.object_id == b));
    let out = s
        .export_binding_plan(&plan, &kasane_sdk::Cmo3Options::default())
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
    let a = s.binding_field_address(&id(1713)).unwrap();
    let p = s
        .plan_binding_field_patch(
            &a,
            BindingFieldPatch::ConstraintSample {
                value: 0.4,
                weight: 0.3,
            },
        )
        .unwrap();
    s.execute_binding_plan(&p).unwrap();
    let c = s.document().get_blend_constraint(&c).unwrap();
    assert_eq!(c.samples[0].id, id(1713));
    assert_eq!(c.keys, vec![0.4, 1.]);
    assert_eq!(c.weights, vec![0.3, 0.9]);
    roundtrip(&s);
}

#[test]
fn shared_rebase_constraints_locks_limits_and_missing_base_reject_without_history() {
    let mut s = shared();
    let table = id(1701);
    let t = s.document().get_blend_key_table(&table).unwrap().clone();
    let before = s.version();
    let h = s.history_state();
    assert_eq!(
        s.plan_binding_authoring(E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Delete {
                sample_id: t.base_sample_id.clone(),
                replacement_base: None
            }
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "BASE_REPLACEMENT_REQUIRED"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
    let p = s
        .plan_binding_authoring(E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: id(1720),
                    value: 0.5,
                },
            },
        }))
        .unwrap();
    let mut limits = s.authoring_limits();
    limits.max_conversion_bytes = p.cost().temporary_bytes - 1;
    s.set_authoring_limits(limits).unwrap();
    assert!(s.execute_binding_plan(&p).is_err());
    assert_eq!(s.version(), before);
    s.set_authoring_limits(Default::default()).unwrap();
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
    let mut b = s.document().get_blend_binding(&id(1702)).unwrap().clone();
    b.constraint_ids.push(c);
    s.edit("constrain", None, |e| e.replace_blend_binding(b))
        .unwrap();
    let before = s.version();
    let h = s.history_state();
    assert_eq!(
        s.plan_binding_authoring(E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetBase {
                sample_id: t.samples[2].id.clone()
            }
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "CONSTRAINED_REBASE_NOT_REPRESENTABLE"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
}

#[test]
fn noop_shared_edits_preserve_redo_and_native_invalid_ids_are_rejected() {
    let mut s = shared();
    let t = s.document().get_blend_key_table(&id(1701)).unwrap().clone();
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
    s.undo().unwrap();
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
    assert!(!receipt.changed);
    assert_eq!(s.history_state(), history);
    let mut d = s.document().fork_candidate();
    let mut a = d.get_asset(ASSET_ID).unwrap().clone();
    a.source = "assets/texture.png".into();
    assert!(d.replace_asset(a).status.is_ok());
    let mut wire: serde_json::Value =
        serde_json::from_str(&kasane_project::encode_project(&d).unwrap()).unwrap();
    wire["document"]["bindings"][0]["axes"][0]["id"] = serde_json::json!("");
    assert_eq!(
        kasane_project::decode_project(&wire.to_string())
            .unwrap_err()
            .code,
        "INVALID_ID"
    );
}

#[test]
fn owned_conversion_edits_keep_limits_and_recheck_them_at_publication() {
    let mut s = base();
    let axis = s.binding(BINDING_ID).unwrap().axes[0].id.clone();
    let intent = E::Grid(G::Edit {
        owner: BINDING_ID.into(),
        operation: O::InsertSample {
            axis_id: axis.clone(),
            id: id(1980),
            value: 0.5,
        },
    });
    let plan = s.plan_binding_authoring(intent.clone()).unwrap();
    let mut edit = s.begin_owned_edit("owned conversion", None).unwrap();
    edit.binding_authoring(intent).unwrap();
    let before = s.version();
    let h = s.history_state();
    let mut limits = s.authoring_limits();
    limits.max_conversion_bytes = 1;
    s.set_authoring_limits(limits).unwrap();
    assert_eq!(
        edit.commit_to(&mut s).unwrap_err().code.as_ref(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), h);
    s.set_authoring_limits(Default::default()).unwrap();
    let mut edit = s.begin_owned_edit("owned conversion", None).unwrap();
    edit.apply_binding_plan(&plan).unwrap();
    edit.commit_to(&mut s).unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        id(1980),
        kasane_core::document::ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    s.edit("hide child", None, |e| e.replace_editor_state(flags))
        .unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::DeleteSample {
            sample_id: id(1980),
        },
    );
    assert!(!s.document().editor_state().objects.contains_key(&id(1980)));
    s.undo().unwrap();
    assert!(s.document().editor_state().get(&id(1980)).hidden_in_editor);
}
