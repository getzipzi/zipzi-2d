use std::collections::BTreeMap;
use zipzi_contract::authoring::CapabilityState;
use zipzi_core::document::PhysicsDefinitionWire;
use zipzi_core::state::RootDomain;
use zipzi_core::{
    document::*,
    physics::{PhysicsAxis, PhysicsVector},
    Canvas, Parameter, Vec2,
};
use zipzi_sdk::{AuthoringSession, PreparedChange, Publication, Status};
fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "00000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn setup() -> (AuthoringSession, String, String, String, String) {
    let mut s = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let a = id();
    let g = id();
    let x = id();
    let y = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
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
            .and_then(|value| {
                let prepared = draft.prepare("parameters")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
pub(super) fn prepare(s: &AuthoringSession, intent: PhysicsEdit) -> Result<PreparedChange, Status> {
    let mut draft = s.begin_draft();
    draft.batch(draft.source(), |candidate_commands| {
        candidate_commands.edit_physics(intent)
    })?;
    draft.prepare("Physics")
}
fn run(s: &mut AuthoringSession, intent: PhysicsEdit) -> Publication {
    let prepared = prepare(s, intent).unwrap();
    s.publish(prepared).unwrap()
}
pub(super) fn roundtrip(s: &AuthoringSession) -> zipzi_core::state::DocumentState {
    let dir = tempfile::tempdir().unwrap();
    let io = zipzi_project::ProjectIo::new(dir.path(), Default::default());
    let path = dir.path().join("physics.zipzi");
    io.save(
        s.snapshot().document(),
        s.snapshot().resources(),
        &path,
        None,
        || true,
    )
    .unwrap();
    io.load(&path, || true).unwrap().document
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
fn patch(s: &mut AuthoringSession, a: &str, g: &str, changes: Vec<PhysicsProperty>) -> Publication {
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
    let mut s = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(),
                (Canvas::new(1., 1., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
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
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (1, 0)
    );
    assert!(r.writes.root_changed(RootDomain::Physics));
    let group = s.snapshot().document().physics_group(&g).unwrap();
    assert_eq!(group.vertices.len(), 2);
    assert!(group.inputs.is_empty() && group.outputs.is_empty());
    let restored = roundtrip(&s);
    assert_eq!(restored.physics(), s.snapshot().document().physics());
    let stale = prepare(
        &s,
        PhysicsEdit::Patch {
            patch: PhysicsPatch {
                asset_id: a.clone(),
                asset_changes: vec![],
                rig: None,
            },
        },
    )
    .unwrap();
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().physics().is_none());
    s.redo(s.version()).unwrap();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
}
#[test]
fn pending_channels_filter_individually_and_map_back_to_native_ids() {
    let (mut s, a, g, x, y) = setup();
    let missing = input(&mut s, &g);
    let active = input(&mut s, &g);
    let node = s.snapshot().document().physics_group(&g).unwrap().vertices[1]
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
    let p = s
        .snapshot()
        .document()
        .physics_projection()
        .unwrap()
        .unwrap();
    assert_eq!(p.groups[0].inputs.native_to_runtime[&missing], None);
    assert_eq!(p.groups[0].inputs.runtime_to_native, vec![active.clone()]);
    assert_eq!(p.groups[0].outputs.native_to_runtime[&off], None);
    assert_eq!(p.groups[0].outputs.runtime_to_native, vec![on.clone()]);
    assert_eq!(p.executable.data.settings[0].inputs[0].weight, 25.);
    assert_eq!(
        zipzi_project::export_physics3(s.snapshot().document())
            .unwrap_err()
            .code
            .as_str(),
        "NEEDS_CONFIGURATION"
    );
    let captured = s.read_snapshot();
    let stale = s.begin_draft();
    assert_eq!(p.disabled.len(), 2);
    let exported = zipzi_project::export_available_physics3(captured.snapshot.document())
        .unwrap()
        .unwrap();
    assert!(!exported.contains(&on));
    assert!(!exported.contains(&node));
    assert!(!exported.contains(&g));
    let mut preview = zipzi_animation::MotionPreview::new(s.snapshot().document());
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
    assert_eq!(
        s.publish(stale.prepare("stale export acceptance").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        zipzi_project::export_available_physics3(captured.snapshot.document())
            .unwrap()
            .unwrap(),
        exported
    );
    let p = s
        .snapshot()
        .document()
        .physics_projection()
        .unwrap()
        .unwrap();
    assert_eq!(p.groups[0].inputs.native_to_runtime[&missing], Some(0));
    let restored = roundtrip(&s);
    assert_eq!(restored.physics(), s.snapshot().document().physics());
}
#[test]
fn node_insertion_and_joint_replacement_do_not_retarget_output_by_index() {
    let (mut s, a, g, _, y) = setup();
    let node = s.snapshot().document().physics_group(&g).unwrap().vertices[1]
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
    let p = s
        .snapshot()
        .document()
        .physics_projection()
        .unwrap()
        .unwrap();
    assert_eq!(p.executable.data.settings[0].outputs[0].vertex_index, 2);
    assert_eq!(
        s.snapshot()
            .document()
            .physics_output(&o)
            .unwrap()
            .1
            .node_id,
        node
    );
    let before = s.version();
    let error = prepare(
        &s,
        PhysicsEdit::Delete {
            ids: vec![node.clone()],
            node_replacements: BTreeMap::new(),
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "PHYSICS_NODE_IN_USE");
    assert_eq!(
        s.snapshot()
            .document()
            .physics_output(&o)
            .unwrap()
            .1
            .node_id,
        node
    );
    assert_eq!(s.version(), before);
    let r = run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![node.clone()],
            node_replacements: BTreeMap::from([(node.clone(), new.clone())]),
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .physics_output(&o)
            .unwrap()
            .1
            .node_id,
        new
    );
    assert!(r.writes.root_changed(RootDomain::Physics));
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .physics_output(&o)
            .unwrap()
            .1
            .node_id,
        node
    );
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .physics_output(&o)
            .unwrap()
            .1
            .node_id,
        new
    );
}
#[test]
fn field_address_survives_reorder_but_not_owner_move_and_undo() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    let j = input(&mut s, &g);
    let mut stale = s.begin_draft();
    stale
        .batch(stale.source(), |c| {
            c.patch_physics(&PhysicsPatch {
                asset_id: a.clone(),
                asset_changes: vec![],
                rig: Some(PhysicsRigPatch {
                    group_id: g.clone(),
                    changes: vec![PhysicsProperty::InputWeight {
                        id: i.clone(),
                        value: 80.,
                    }],
                }),
            })
        })
        .unwrap();
    run(
        &mut s,
        PhysicsEdit::MoveInputs {
            ids: vec![j],
            group_id: g.clone(),
            before: Some(i.clone()),
        },
    );
    patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::InputWeight {
            id: i.clone(),
            value: 35.,
        }],
    );
    assert_eq!(
        s.snapshot().document().physics_input(&i).unwrap().1.weight,
        35.
    );
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
            group_id: g2.clone(),
            before: None,
        },
    );
    assert_eq!(s.snapshot().document().physics_input(&i).unwrap().0.id, g2);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().physics_input(&i).unwrap().0.id, g);
    assert_eq!(
        s.publish(stale.prepare("stale field").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
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
    let n = s.snapshot().document().physics_group(&g).unwrap().vertices[1]
        .id
        .clone();
    let n2 = s.snapshot().document().physics_group(&g2).unwrap().vertices[1]
        .id
        .clone();
    let o = output(&mut s, &g, &n);
    let stale = s.begin_draft();
    let version = s.version();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    assert!(prepare(
        &s,
        PhysicsEdit::MoveOutputs {
            ids: vec![o.clone()],
            group_id: g2.clone(),
            before: None,
            node_replacements: BTreeMap::new()
        }
    )
    .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    run(
        &mut s,
        PhysicsEdit::MoveOutputs {
            ids: vec![o.clone()],
            group_id: g2,
            before: None,
            node_replacements: BTreeMap::from([(o.clone(), n2)]),
        },
    );
    assert_eq!(
        s.publish(stale.prepare("stale move").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(s.snapshot().document().physics_output(&o).unwrap().1.id, o);
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
    let n = s.snapshot().document().physics_group(&g).unwrap().vertices[1]
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
    let groups = &s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings;
    let original = &groups[0];
    let copy = &groups[1];
    let old: std::collections::HashSet<_> = original.entity_ids().collect();
    assert!(copy.entity_ids().all(|id| !old.contains(id)));
    assert_eq!(copy.inputs[0].source.parameter_id(), Some(x.as_str()));
    assert_eq!(copy.outputs[0].node_id, copy.vertices[1].id);
    assert_ne!(copy.external_id, original.external_id);
    let stale = s.begin_draft();
    let id = copy.id.clone();
    run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![id],
            node_replacements: Default::default(),
        },
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("old copy").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        s.snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .data
            .settings
            .len(),
        2
    );
}
#[test]
fn root_node_and_invalid_references_are_rejected_without_history() {
    let (mut s, _, g, _, _) = setup();
    let n = s.snapshot().document().physics_group(&g).unwrap().vertices[0]
        .id
        .clone();

    assert!(matches!(
        s.snapshot()
            .document()
            .physics_capabilities(&n)
            .unwrap()
            .delete,
        CapabilityState::Available
    ));
    let before = s.version();
    assert!(prepare(
        &s,
        PhysicsEdit::Delete {
            ids: vec![n],
            node_replacements: Default::default()
        }
    )
    .is_err());
    assert_eq!(s.version(), before);
    let i = input(&mut s, &g);

    assert!(prepare(
        &s,
        PhysicsEdit::Patch {
            patch: PhysicsPatch {
                asset_id: s
                    .snapshot()
                    .document()
                    .physics()
                    .as_ref()
                    .unwrap()
                    .id
                    .clone(),
                asset_changes: vec![],
                rig: Some(PhysicsRigPatch {
                    group_id: g.clone(),
                    changes: vec![PhysicsProperty::InputParameter {
                        id: i,
                        target: PhysicsTarget::Resolved { parameter_id: id() }
                    }]
                })
            }
        }
    )
    .is_err());
}
#[test]
fn stale_and_forked_plans_cannot_publish_and_noop_preserves_redo() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    let plan = prepare(
        &s,
        PhysicsEdit::CreateInput {
            group_id: g.clone(),
            id: id(),
            before: None,
        },
    )
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
    assert!(s.publish(plan).is_err());
    s.undo(s.version()).unwrap();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let r = patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::InputWeight { id: i, value: 100. }],
    );
    assert!(r.writes.is_empty());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    let foreign = AuthoringSession::new(s.snapshot().clone());
    let prepared = prepare(
        &foreign,
        PhysicsEdit::CreateInput {
            group_id: g,
            id: id(),
            before: None,
        },
    )
    .unwrap();
    assert_eq!(s.publish(prepared).unwrap_err().code, "STALE_SOURCE");
}
#[test]
fn changesets_label_order_and_history_direction_are_precise() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    let before = s.snapshot().content_id();
    let r = patch(
        &mut s,
        &a,
        &g,
        vec![PhysicsProperty::Name("Renamed".into())],
    );
    assert!(r.writes.root_changed(RootDomain::Physics));
    assert_ne!(s.snapshot().content_id(), before);
    assert_eq!(
        s.snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .data
            .meta
            .dictionary
            .iter()
            .find(|entry| entry.id == g)
            .unwrap()
            .name,
        "Renamed"
    );
    let r = run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![i.clone()],
            node_replacements: Default::default(),
        },
    );
    assert!(r.writes.root_changed(RootDomain::Physics));
    assert!(s.snapshot().document().physics_input(&i).is_none());
    s.take_events();
    s.undo(s.version()).unwrap();
    let r = s.take_events().pop_back().unwrap();
    assert!(r.writes.root_changed(RootDomain::Physics));
    assert!(s.snapshot().document().physics_input(&i).is_some());
}

#[test]
fn unresolved_import_keeps_usable_channels_and_external_group_ids() {
    let (mut s, a, _, x, y) = setup();
    let source = include_str!("../../../fixtures/animation_cpu/thirty.physics3.json");
    let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
    value["PhysicsSettings"][0]["Input"][0]["Source"]["Id"] = "Missing".into();
    value["PhysicsSettings"][0]["Output"][0]["Destination"]["Id"] = "Y".into();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                {
                    let imported =
                        zipzi_project::import_physics3(&e.read_document()?, &a, &value.to_string())
                            .map_err(|error| Status::error(error.code, error.message))?;
                    e.replace_physics(Some(
                        imported
                            .candidate
                            .physics()
                            .as_ref()
                            .unwrap()
                            .as_ref()
                            .clone(),
                    ))
                }?;
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("import")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let p = s
        .snapshot()
        .document()
        .physics_projection()
        .unwrap()
        .unwrap();
    assert_eq!(p.disabled.len(), 1);
    assert_eq!(p.executable.data.settings[0].outputs.len(), 1);
    assert_eq!(p.executable.data.settings[0].inputs.len(), 0);
    assert_eq!(
        zipzi_project::export_physics3(s.snapshot().document())
            .unwrap_err()
            .code
            .as_str(),
        "UNRESOLVED_PARAMETER"
    );
    let g = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0]
        .clone();
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
    let encoded = zipzi_project::export_physics3(s.snapshot().document())
        .unwrap()
        .unwrap();
    assert!(!encoded.contains(&g.id));
    assert!(encoded.contains(&g.external_id));
    assert!(s.snapshot().document().parameter(&y).is_some());
}
#[test]
fn locks_guard_planning_field_patches_replacement_and_generic_deletion() {
    let (mut s, a, g, _, _) = setup();
    let i = input(&mut s, &g);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut state = e.read_document().unwrap().editor_state().clone();
                state.objects.entry(i.clone()).or_default().locked = true;
                e.replace_editor_state(state)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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

    assert!(matches!(
        s.snapshot()
            .document()
            .physics_capabilities(&g)
            .unwrap()
            .reorder,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert!(prepare(
        &s,
        PhysicsEdit::ReorderGroups {
            ids: vec![other],
            before: Some(g.clone()),
        }
    )
    .is_err());

    assert!(matches!(
        s.snapshot()
            .document()
            .physics_capabilities(&i)
            .unwrap()
            .patch,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert_eq!(
        prepare(
            &s,
            PhysicsEdit::Delete {
                ids: vec![g.clone()],
                node_replacements: Default::default()
            }
        )
        .unwrap_err()
        .code
        .as_str(),
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
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.patch_physics(&patch))
            .and_then(|value| {
                let prepared = draft.prepare("Patch")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    let mut asset = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .as_ref()
        .clone();
    std::sync::Arc::make_mut(&mut asset.data.settings[0])
        .inputs
        .clear();
    asset.reconcile_counts();
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_physics(Some(asset)))
            .and_then(|value| {
                let prepared = draft.prepare("Replace")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_physics(None))
            .and_then(|value| {
                let prepared = draft.prepare("Erase")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
}
#[test]
fn node_reorder_and_copy_preserve_logical_output_and_renew_copy_identity() {
    let (mut s, _, g, _, _) = setup();
    let n = s.snapshot().document().physics_group(&g).unwrap().vertices[1]
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
    let group = s.snapshot().document().physics_group(&g).unwrap();
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
        s.snapshot()
            .document()
            .physics_group(&g)
            .unwrap()
            .node_index(&n),
        Some(1)
    );
    assert_eq!(
        s.snapshot()
            .document()
            .physics_output(&o)
            .unwrap()
            .1
            .node_id,
        n
    );
}
#[test]
fn deleted_and_recreated_child_in_one_transaction_retires_old_handle() {
    let (mut s, _, g, _, _) = setup();
    let i = input(&mut s, &g);
    let before = s.read_snapshot();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |e| {
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
    let receipt = s.publish(draft.prepare("Recreate").unwrap()).unwrap();
    assert!(receipt.writes.is_empty());
    assert_eq!(s.read_snapshot().source, before.source);
    assert_eq!(s.snapshot().document().physics_input(&i).unwrap().1.id, i);
}

#[test]
fn reopened_field_address_survives_first_reorder_and_is_retired_by_reload() {
    let (mut source, a, group, _, _) = setup();
    let first = input(&mut source, &group);
    let second = input(&mut source, &group);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("physics.zipzi");
    let io = zipzi_project::ProjectIo::new(dir.path(), Default::default());
    source.save_as(&io, &path, None, || true).unwrap();
    let (mut session, _) = AuthoringSession::open(&io, &path, || true).unwrap();
    let stale = session.begin_draft();
    run(
        &mut session,
        PhysicsEdit::MoveInputs {
            ids: vec![second],
            group_id: group.clone(),
            before: Some(first.clone()),
        },
    );
    patch(
        &mut session,
        &a,
        &group,
        vec![PhysicsProperty::InputWeight {
            id: first.clone(),
            value: 40.,
        }],
    );
    assert_eq!(
        session
            .snapshot()
            .document()
            .physics_input(&first)
            .unwrap()
            .1
            .weight,
        40.
    );
    let loaded = io.load(&path, || true).unwrap();
    session
        .replace(
            session.version(),
            zipzi_sdk::Snapshot::new(loaded.document, loaded.resources).unwrap(),
        )
        .unwrap();
    assert_eq!(
        session
            .publish(stale.prepare("old source").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        session
            .snapshot()
            .document()
            .physics_input(&first)
            .unwrap()
            .1
            .weight,
        100.
    );
}

#[test]
fn deleting_channels_from_a_root_only_import_preserves_the_chain() {
    let (mut s, a, g, _, _) = setup();
    let mut asset = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .as_ref()
        .clone();
    std::sync::Arc::make_mut(&mut asset.data.settings[0])
        .vertices
        .truncate(1);
    let source = asset.data.to_wire(|_| unreachable!());
    let text = serde_json::to_string(&source).unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                {
                    let imported = zipzi_project::import_physics3(&e.read_document()?, &a, &text)
                        .map_err(|error| Status::error(error.code, error.message))?;
                    e.replace_physics(Some(
                        imported
                            .candidate
                            .physics()
                            .as_ref()
                            .unwrap()
                            .as_ref()
                            .clone(),
                    ))
                }
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Import root-only rig")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let rig = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0]
        .clone();
    assert_ne!(rig.id, g);
    let input = input(&mut s, &rig.id);
    let output = output(&mut s, &rig.id, &rig.vertices[0].id);
    let before = s.snapshot().document().physics().as_ref().unwrap().clone();
    run(
        &mut s,
        PhysicsEdit::Delete {
            ids: vec![input.clone(), output.clone()],
            node_replacements: Default::default(),
        },
    );
    let current = s.snapshot().document().physics_group(&rig.id).unwrap();
    assert_eq!(current.vertices, rig.vertices);
    assert!(current.inputs.is_empty() && current.outputs.is_empty());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().physics(), &Some(before));
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
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let error = prepare(
        &s,
        PhysicsEdit::CreateGroup {
            asset_id: parameter,
            id: id(),
            name: "Rig".into(),
            before: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code.as_str(), "DUPLICATE_ID");
    assert_eq!(s.version(), version);
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    assert!(s.snapshot().document().physics().is_none());
}
