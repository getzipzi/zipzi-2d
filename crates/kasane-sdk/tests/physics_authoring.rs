use kasane_core::{
    document::*,
    physics::{PhysicsAxis, PhysicsVector},
    Canvas, Parameter, Vec2,
};
use kasane_sdk::{AuthoringSession, CapabilityState, ChangeCategory, ObjectKind};
use std::collections::BTreeMap;
fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "00000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn setup() -> (AuthoringSession, String, String, String, String) {
    let mut s = AuthoringSession::new(&id(), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
    let a = id();
    let g = id();
    let x = id();
    let y = id();
    s.edit("parameters", None, |e| {
        for (id, runtime) in [(&x, "X"), (&y, "Y")] {
            e.create_parameter(Parameter {
                id: id.clone(),
                runtime_id: runtime.into(),
                minimum: -1.,
                maximum: 1.,
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();
    run(
        &mut s,
        PhysicsEdit::CreateGroup {
            asset_id: a.clone(),
            id: g.clone(),
            name: "Rig".into(),
            before: None,
        },
    );
    (s, a, g, x, y)
}
fn run(s: &mut AuthoringSession, intent: PhysicsEdit) -> kasane_sdk::EditReceipt {
    let p = s.plan_physics_edit(intent).unwrap();
    s.execute_physics_plan(&p).unwrap()
}
fn input(s: &mut AuthoringSession, g: &str) -> String {
    let v = id();
    run(
        s,
        PhysicsEdit::CreateInput {
            group_id: g.into(),
            id: v.clone(),
            before: None,
        },
    );
    v
}
fn output(s: &mut AuthoringSession, g: &str, node: &str) -> String {
    let v = id();
    run(
        s,
        PhysicsEdit::CreateOutput {
            group_id: g.into(),
            id: v.clone(),
            node_id: node.into(),
            before: None,
        },
    );
    v
}
fn patch(
    s: &mut AuthoringSession,
    a: &str,
    g: &str,
    changes: Vec<PhysicsProperty>,
) -> kasane_sdk::EditReceipt {
    run(
        s,
        PhysicsEdit::Patch {
            patch: PhysicsPatch {
                asset_id: a.into(),
                asset_changes: vec![],
                rig: Some(PhysicsRigPatch {
                    group_id: g.into(),
                    changes,
                }),
            },
        },
    )
}
#[test]
fn root_and_group_are_one_transaction_and_ids_survive_native_save() {
    let mut s = AuthoringSession::new(&id(), Canvas::new(1., 1., Vec2::default(), 1.)).unwrap();
    let a = id();
    let g = id();
    let r = run(
        &mut s,
        PhysicsEdit::CreateGroup {
            asset_id: a.clone(),
            id: g.clone(),
            name: "Empty".into(),
            before: None,
        },
    );
    assert_eq!(s.history_lengths(), (1, 0));
    assert_eq!(r.changes.created.len(), 4);
    let group = s.document().get_physics_group(&g).unwrap();
    assert_eq!(group.vertices.len(), 2);
    assert!(group.inputs.is_empty() && group.outputs.is_empty());
    let restored =
        kasane_project::decode_project(&kasane_project::encode_project(s.document()).unwrap())
            .unwrap();
    assert_eq!(restored.physics(), s.document().physics());
    let h = s.handle(ObjectKind::PhysicsGroup, &g).unwrap();
    s.undo().unwrap();
    assert!(s.document().physics().is_none());
    s.redo().unwrap();
    assert!(s.resolve_handle(&h).is_err());
}
#[test]
fn pending_channels_filter_individually_and_map_back_to_native_ids() {
    let (mut s, a, g, x, y) = setup();
    let missing = input(&mut s, &g);
    let active = input(&mut s, &g);
    let node = s.document().get_physics_group(&g).unwrap().vertices[1]
        .id
        .clone();
    let off = output(&mut s, &g, &node);
    let on = output(&mut s, &g, &node);
    patch(
        &mut s,
        &a,
        &g,
        vec![
            PhysicsProperty::InputParameter {
                id: active.clone(),
                target: PhysicsTarget::Resolved {
                    parameter_id: x.clone(),
                },
            },
            PhysicsProperty::OutputParameter {
                id: on.clone(),
                target: PhysicsTarget::Resolved {
                    parameter_id: y.clone(),
                },
            },
            PhysicsProperty::InputWeight {
                id: active.clone(),
                value: 25.,
            },
        ],
    );
    let p = s.document().physics_projection().unwrap().unwrap();
    assert_eq!(p.groups[0].inputs.native_to_runtime[&missing], None);
    assert_eq!(p.groups[0].inputs.runtime_to_native, vec![active.clone()]);
    assert_eq!(p.groups[0].outputs.native_to_runtime[&off], None);
    assert_eq!(p.groups[0].outputs.runtime_to_native, vec![on.clone()]);
    assert_eq!(p.executable.data.settings[0].inputs[0].weight, 25.);
    assert_eq!(
        s.export_physics3().unwrap_err().code.as_ref(),
        "NEEDS_CONFIGURATION"
    );
    let plan = s.plan_available_physics_export().unwrap();
    assert_eq!(plan.omitted.len(), 2);
    let exported = s.export_physics_plan(&plan).unwrap().unwrap();
    assert!(!exported.contains(&on));
    assert!(!exported.contains(&node));
    assert!(!exported.contains(&g));
    let mut preview = s.motion_preview();
    preview.stabilize_physics();
    let trace = preview.physics_snapshot();
    assert_eq!(trace[0].id, g);
    assert_eq!(trace[0].input_ids, vec![active]);
    assert_eq!(trace[0].output_ids, vec![on]);
    assert_eq!(trace[0].inputs.len(), 1);
    assert_eq!(trace[0].outputs.len(), 1);
    patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::InputParameter {
            id: missing.clone(),
            target: PhysicsTarget::Resolved { parameter_id: x },
        }],
    );
    assert!(s.export_physics_plan(&plan).is_err());
    let p = s.document().physics_projection().unwrap().unwrap();
    assert_eq!(p.groups[0].inputs.native_to_runtime[&missing], Some(0));
    let restored =
        kasane_project::decode_project(&kasane_project::encode_project(s.document()).unwrap())
            .unwrap();
    assert_eq!(restored.physics(), s.document().physics());
}
#[test]
fn node_insertion_and_joint_replacement_do_not_retarget_output_by_index() {
    let (mut s, a, g, _, y) = setup();
    let node = s.document().get_physics_group(&g).unwrap().vertices[1]
        .id
        .clone();
    let o = output(&mut s, &g, &node);
    patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::OutputParameter {
            id: o.clone(),
            target: PhysicsTarget::Resolved { parameter_id: y },
        }],
    );
    let new = id();
    run(
        &mut s,
        PhysicsEdit::InsertNode {
            group_id: g.clone(),
            node: PhysicsNode {
                id: new.clone(),
                position: PhysicsVector { x: 0., y: 5. },
                mobility: 0.5,
                delay: 0.8,
                acceleration: 1.,
                radius: 5.,
                extensions: BTreeMap::new(),
            },
            before: Some(node.clone()),
        },
    );
    let p = s.document().physics_projection().unwrap().unwrap();
    assert_eq!(p.executable.data.settings[0].outputs[0].vertex_index, 2);
    assert_eq!(s.document().get_physics_output(&o).unwrap().1.node_id, node);
    let before = s.version();
    let error = s
        .plan_physics_edit(PhysicsEdit::Delete {
            ids: vec![node.clone()],
            node_replacements: BTreeMap::new(),
        })
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "PHYSICS_NODE_IN_USE");
    assert_eq!(error.referrers.as_ref(), [o.clone()]);
    assert_eq!(s.version(), before);
    let r = run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![node.clone()],
            node_replacements: BTreeMap::from([(node.clone(), new.clone())]),
        },
    );
    assert_eq!(s.document().get_physics_output(&o).unwrap().1.node_id, new);
    assert!(!r.changes.physics_references.is_empty());
    s.undo().unwrap();
    assert_eq!(s.document().get_physics_output(&o).unwrap().1.node_id, node);
    s.redo().unwrap();
    assert_eq!(s.document().get_physics_output(&o).unwrap().1.node_id, new);
}
#[test]
fn field_address_survives_reorder_but_not_owner_move_and_undo() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    let j = input(&mut s, &g);
    let address = s.physics_field_address(&i).unwrap();
    let h = s.handle(ObjectKind::PhysicsInput, &i).unwrap();
    run(
        &mut s,
        PhysicsEdit::MoveInputs {
            ids: vec![j],
            group_id: g.clone(),
            before: Some(i.clone()),
        },
    );
    let p = s
        .plan_physics_field_patch(
            &address,
            vec![PhysicsProperty::InputWeight {
                id: i.clone(),
                value: 35.,
            }],
        )
        .unwrap();
    s.execute_physics_plan(&p).unwrap();
    assert_eq!(s.document().get_physics_input(&i).unwrap().1.weight, 35.);
    let address = s.physics_field_address(&i).unwrap();
    let g2 = id();
    run(
        &mut s,
        PhysicsEdit::CreateGroup {
            asset_id: a,
            id: g2.clone(),
            name: "Other".into(),
            before: None,
        },
    );
    run(
        &mut s,
        PhysicsEdit::MoveInputs {
            ids: vec![i.clone()],
            group_id: g2,
            before: None,
        },
    );
    assert!(s.resolve_handle(&h).is_ok());
    s.undo().unwrap();
    assert!(s
        .plan_physics_field_patch(
            &address,
            vec![PhysicsProperty::InputWeight { id: i, value: 80. }]
        )
        .is_err());
}
#[test]
fn cross_group_output_moves_require_explicit_node_and_are_atomic() {
    let (mut s, a, g, _, _) = setup();
    let g2 = id();
    run(
        &mut s,
        PhysicsEdit::CreateGroup {
            asset_id: a,
            id: g2.clone(),
            name: "Other".into(),
            before: None,
        },
    );
    let n = s.document().get_physics_group(&g).unwrap().vertices[1]
        .id
        .clone();
    let n2 = s.document().get_physics_group(&g2).unwrap().vertices[1]
        .id
        .clone();
    let o = output(&mut s, &g, &n);
    let address = s.physics_field_address(&o).unwrap();
    let version = s.version();
    let history = s.history_lengths();
    assert!(s
        .plan_physics_edit(PhysicsEdit::MoveOutputs {
            ids: vec![o.clone()],
            group_id: g2.clone(),
            before: None,
            node_replacements: BTreeMap::new()
        })
        .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    run(
        &mut s,
        PhysicsEdit::MoveOutputs {
            ids: vec![o.clone()],
            group_id: g2,
            before: None,
            node_replacements: BTreeMap::from([(o.clone(), n2)]),
        },
    );
    assert!(s.plan_physics_field_patch(&address, vec![]).is_err());
    assert_eq!(s.document().get_physics_output(&o).unwrap().1.id, o);
}
#[test]
fn duplicate_group_remaps_owned_identity_and_preserves_external_targets() {
    let (mut s, a, g, x, _) = setup();
    let i = input(&mut s, &g);
    patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::InputParameter {
            id: i,
            target: PhysicsTarget::Resolved {
                parameter_id: x.clone(),
            },
        }],
    );
    let n = s.document().get_physics_group(&g).unwrap().vertices[1]
        .id
        .clone();
    output(&mut s, &g, &n);
    run(
        &mut s,
        PhysicsEdit::Duplicate {
            ids: vec![g.clone()],
            group_id: None,
            before: None,
        },
    );
    let groups = &s.document().physics().unwrap().data.settings;
    let original = &groups[0];
    let copy = &groups[1];
    let old: std::collections::HashSet<_> = original.entity_ids().collect();
    assert!(copy.entity_ids().all(|id| !old.contains(id)));
    assert_eq!(copy.inputs[0].source.parameter_id(), Some(x.as_str()));
    assert_eq!(copy.outputs[0].node_id, copy.vertices[1].id);
    assert_ne!(copy.external_id, original.external_id);
    let h = s
        .handle(ObjectKind::PhysicsInput, &copy.inputs[0].id)
        .unwrap();
    let id = copy.id.clone();
    run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![id],
            node_replacements: Default::default(),
        },
    );
    s.undo().unwrap();
    assert!(s.resolve_handle(&h).is_err());
    assert_eq!(s.document().physics().unwrap().data.settings.len(), 2);
}
#[test]
fn root_node_and_invalid_references_are_rejected_without_history() {
    let (mut s, _, g, _, _) = setup();
    let n = s.document().get_physics_group(&g).unwrap().vertices[0]
        .id
        .clone();
    let h = s.handle(ObjectKind::PhysicsNode, &n).unwrap();
    assert!(matches!(
        s.physics_capabilities(&h).unwrap().delete,
        CapabilityState::Unsupported {
            code: "PHYSICS_ROOT_NODE"
        }
    ));
    let before = s.version();
    assert!(s
        .plan_physics_edit(PhysicsEdit::Delete {
            ids: vec![n],
            node_replacements: Default::default()
        })
        .is_err());
    assert_eq!(s.version(), before);
    let i = input(&mut s, &g);
    let address = s.physics_field_address(&i).unwrap();
    assert!(s
        .plan_physics_field_patch(
            &address,
            vec![PhysicsProperty::InputParameter {
                id: i,
                target: PhysicsTarget::Resolved { parameter_id: id() }
            }]
        )
        .is_err());
}
#[test]
fn stale_and_forked_plans_cannot_publish_and_noop_preserves_redo() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    let plan = s
        .plan_physics_edit(PhysicsEdit::CreateInput {
            group_id: g.clone(),
            id: id(),
            before: None,
        })
        .unwrap();
    patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::InputWeight {
            id: i.clone(),
            value: 20.,
        }],
    );
    assert!(s.execute_physics_plan(&plan).is_err());
    s.undo().unwrap();
    let history = s.history_lengths();
    let r = patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::InputWeight { id: i, value: 100. }],
    );
    assert!(!r.changed);
    assert_eq!(s.history_lengths(), history);
    let doc = s.document();
    let p = doc
        .plan_physics_edit(PhysicsEdit::CreateInput {
            group_id: g.clone(),
            id: id(),
            before: None,
        })
        .unwrap();
    let mut fork = doc.fork_candidate();
    let mut asset = fork.physics().unwrap().clone();
    asset.data.meta.fps = Some(60.);
    assert!(fork.set_physics(asset).status.is_ok());
    assert_eq!(fork.apply_physics_plan(&p).status.code, "STALE_PLAN");
}
#[test]
fn changesets_label_order_and_history_direction_are_precise() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    let r = patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::Name("Renamed".into())],
    );
    assert!(r.changes.categories.contains(&ChangeCategory::Labels));
    assert!(!r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert!(!r.changes.full_invalidation);
    let r = run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![i.clone()],
            node_replacements: Default::default(),
        },
    );
    assert!(r.changes.deleted.iter().any(|e| e.id == i));
    s.drain_events();
    s.undo().unwrap();
    let r = s.drain_events().pop().unwrap();
    assert!(r.changes.created.iter().any(|e| e.id == i));
    assert!(r.changes.deleted.iter().all(|e| e.id != i));
}
#[test]
fn unresolved_import_keeps_usable_channels_and_external_group_ids() {
    let (mut s, a, _, x, y) = setup();
    let source = include_str!("../../../fixtures/animation_cpu/thirty.physics3.json");
    let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
    value["PhysicsSettings"][0]["Input"][0]["Source"]["Id"] = "Missing".into();
    value["PhysicsSettings"][0]["Output"][0]["Destination"]["Id"] = "Y".into();
    s.edit("import", None, |e| {
        e.import_physics3(&a, &value.to_string())?;
        Ok(())
    })
    .unwrap();
    let p = s.document().physics_projection().unwrap().unwrap();
    assert_eq!(p.disabled.len(), 1);
    assert_eq!(p.executable.data.settings[0].outputs.len(), 1);
    assert_eq!(p.executable.data.settings[0].inputs.len(), 0);
    assert_eq!(
        s.export_physics3().unwrap_err().code.as_ref(),
        "UNRESOLVED_PARAMETER"
    );
    let g = s.document().physics().unwrap().data.settings[0].clone();
    assert_eq!(
        g.external_id,
        value["PhysicsSettings"][0]["Id"].as_str().unwrap()
    );
    let input = g.inputs[0].id.clone();
    patch(
        &mut s,
        &a,
        &g.id,
        vec![
            PhysicsProperty::InputParameter {
                id: input,
                target: PhysicsTarget::Resolved { parameter_id: x },
            },
            PhysicsProperty::OutputAxis {
                id: g.outputs[0].id.clone(),
                value: PhysicsAxis::Angle,
            },
        ],
    );
    let encoded = s.export_physics3().unwrap().unwrap();
    assert!(!encoded.contains(&g.id));
    assert!(encoded.contains(&g.external_id));
    assert!(s.document().get_parameter(&y).is_some());
}
#[test]
fn locks_guard_planning_field_patches_replacement_and_generic_deletion() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    s.edit("Lock", None, |e| {
        let mut state = e.candidate_document().editor_state().clone();
        state.objects.entry(i.clone()).or_default().locked = true;
        e.replace_editor_state(state)
    })
    .unwrap();
    let other = id();
    run(
        &mut s,
        PhysicsEdit::CreateGroup {
            asset_id: a.clone(),
            id: other.clone(),
            name: "Other".into(),
            before: None,
        },
    );
    let group_handle = s.handle(ObjectKind::PhysicsGroup, &g).unwrap();
    assert!(matches!(
        s.physics_capabilities(&group_handle).unwrap().reorder,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert!(s
        .plan_physics_edit(PhysicsEdit::ReorderGroups {
            ids: vec![other],
            before: Some(g.clone()),
        })
        .is_err());
    let h = s.handle(ObjectKind::PhysicsInput, &i).unwrap();
    assert!(matches!(
        s.physics_capabilities(&h).unwrap().patch,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert_eq!(
        s.plan_physics_edit(PhysicsEdit::Delete {
            ids: vec![g.clone()],
            node_replacements: Default::default()
        })
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    let patch = PhysicsPatch {
        asset_id: a.clone(),
        asset_changes: vec![],
        rig: Some(PhysicsRigPatch {
            group_id: g.clone(),
            changes: vec![PhysicsProperty::InputWeight {
                id: i.clone(),
                value: 20.,
            }],
        }),
    };
    let history = s.history_lengths();
    assert!(s.edit("Patch", None, |e| e.patch_physics(&patch)).is_err());
    let mut asset = s.document().physics().unwrap().clone();
    std::sync::Arc::make_mut(&mut asset.data.settings[0])
        .inputs
        .clear();
    asset.reconcile_counts();
    assert!(s.edit("Replace", None, |e| e.set_physics(asset)).is_err());
    assert!(s.edit("Erase", None, |e| e.erase_object(&a)).is_err());
    assert_eq!(s.history_lengths(), history);
}
#[test]
fn node_reorder_and_copy_preserve_logical_output_and_renew_copy_identity() {
    let (mut s, _, g, _, _) = setup();
    let n = s.document().get_physics_group(&g).unwrap().vertices[1]
        .id
        .clone();
    let o = output(&mut s, &g, &n);
    run(
        &mut s,
        PhysicsEdit::DuplicateNode {
            id: n.clone(),
            before: Some(n.clone()),
        },
    );
    let group = s.document().get_physics_group(&g).unwrap();
    assert_eq!(group.vertices.len(), 3);
    let copy = group.vertices[1].id.clone();
    assert_ne!(copy, n);
    run(
        &mut s,
        PhysicsEdit::MoveNode {
            id: n.clone(),
            before: Some(copy),
        },
    );
    assert_eq!(
        s.document().get_physics_group(&g).unwrap().node_index(&n),
        Some(1)
    );
    assert_eq!(s.document().get_physics_output(&o).unwrap().1.node_id, n);
}
#[test]
fn deleted_and_recreated_child_in_one_transaction_retires_old_handle() {
    let (mut s, _, g, _, _) = setup();
    let i = input(&mut s, &g);
    let old = s.handle(ObjectKind::PhysicsInput, &i).unwrap();
    s.edit("Recreate", None, |e| {
        e.edit_physics(PhysicsEdit::Delete {
            ids: vec![i.clone()],
            node_replacements: Default::default(),
        })?;
        e.edit_physics(PhysicsEdit::CreateInput {
            group_id: g.clone(),
            id: i.clone(),
            before: None,
        })
    })
    .unwrap();
    assert!(s.resolve_handle(&old).is_err());
    assert!(s.handle(ObjectKind::PhysicsInput, &i).is_ok());
}

#[test]
fn reopened_field_address_survives_first_reorder_and_is_retired_by_reload() {
    let (mut source, _, group, _, _) = setup();
    let first = input(&mut source, &group);
    let second = input(&mut source, &group);
    let path = std::env::temp_dir().join(format!(
        "kasane-physics-address-{}-{}",
        std::process::id(),
        id()
    ));
    source.save_project(&path, None).unwrap();
    drop(source);
    let mut session =
        AuthoringSession::new(&id(), Canvas::new(1., 1., Vec2::default(), 1.)).unwrap();
    session.open_project(&path, None).unwrap();
    let address = session.physics_field_address(&first).unwrap();
    run(
        &mut session,
        PhysicsEdit::MoveInputs {
            ids: vec![second],
            group_id: group.clone(),
            before: Some(first.clone()),
        },
    );
    let plan = session
        .plan_physics_field_patch(
            &address,
            vec![PhysicsProperty::InputWeight {
                id: first.clone(),
                value: 40.,
            }],
        )
        .unwrap();
    session.execute_physics_plan(&plan).unwrap();
    assert_eq!(
        session
            .document()
            .get_physics_input(&first)
            .unwrap()
            .1
            .weight,
        40.
    );
    let address = session.physics_field_address(&first).unwrap();
    session.open_project(&path, None).unwrap();
    assert!(session.plan_physics_field_patch(&address, vec![]).is_err());
    drop(session);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn deleting_channels_from_a_root_only_import_preserves_the_chain() {
    let (mut s, a, g, _, _) = setup();
    let mut asset = s.document().physics().unwrap().clone();
    std::sync::Arc::make_mut(&mut asset.data.settings[0])
        .vertices
        .truncate(1);
    let source = asset.data.to_wire(|_| unreachable!());
    let text = serde_json::to_string(&source).unwrap();
    s.edit("Import root-only rig", None, |e| {
        e.import_physics3(&a, &text).map(|_| ())
    })
    .unwrap();
    let rig = s.document().physics().unwrap().data.settings[0].clone();
    assert_ne!(rig.id, g);
    let input = input(&mut s, &rig.id);
    let output = output(&mut s, &rig.id, &rig.vertices[0].id);
    let before = s.document().physics().unwrap().clone();
    run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![input.clone(), output.clone()],
            node_replacements: Default::default(),
        },
    );
    let current = s.document().get_physics_group(&rig.id).unwrap();
    assert_eq!(current.vertices, rig.vertices);
    assert!(current.inputs.is_empty() && current.outputs.is_empty());
    s.undo().unwrap();
    assert_eq!(s.document().physics(), Some(&before));
}

#[test]
fn root_creation_plan_rejects_ids_owned_by_other_object_kinds() {
    let (mut s, asset, _, parameter, _) = setup();
    run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![asset],
            node_replacements: Default::default(),
        },
    );
    let version = s.version();
    let history = s.history_lengths();
    let error = s
        .plan_physics_edit(PhysicsEdit::CreateGroup {
            asset_id: parameter,
            id: id(),
            name: "Rig".into(),
            before: None,
        })
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "DUPLICATE_ID");
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    assert!(s.document().physics().is_none());
}
