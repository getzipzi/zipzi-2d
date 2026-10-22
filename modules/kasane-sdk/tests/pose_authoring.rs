use kasane_core::{document::*, Canvas, Part, Vec2};
use kasane_sdk::{AuthoringSession, CapabilityState, ChangeCategory, ObjectKind};
fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "00000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn run(s: &mut AuthoringSession, edit: PoseEdit) -> kasane_sdk::EditReceipt {
    let p = s.plan_pose_edit(edit).unwrap();
    s.execute_pose_plan(&p).unwrap()
}
fn setup() -> (AuthoringSession, String, String, String, String, String) {
    let mut s = AuthoringSession::new(&id(), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
    let (p, g, h, a, b) = (id(), id(), id(), id(), id());
    s.edit("Parts", None, |e| {
        for (id, runtime) in [(&a, "PartA"), (&b, "PartB")] {
            e.create_part(Part {
                id: id.clone(),
                runtime_id: runtime.into(),
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();
    for group in [&g, &h] {
        run(
            &mut s,
            PoseEdit::CreateGroup {
                pose_id: p.clone(),
                id: group.clone(),
                name: "Group".into(),
                before: None,
            },
        );
    }
    (s, p, g, h, a, b)
}
fn entry(s: &mut AuthoringSession, g: &str) -> String {
    let id = id();
    run(
        s,
        PoseEdit::CreateEntry {
            group_id: g.into(),
            id: id.clone(),
            before: None,
        },
    );
    id
}
fn link(s: &mut AuthoringSession, e: &str) -> String {
    let id = id();
    run(
        s,
        PoseEdit::CreateLink {
            entry_id: e.into(),
            id: id.clone(),
            before: None,
        },
    );
    id
}
fn patch(s: &mut AuthoringSession, id: &str, fields: PoseFields) -> kasane_sdk::EditReceipt {
    let a = s.pose_field_address(id).unwrap();
    let p = s.plan_pose_field_patch(&a, fields).unwrap();
    s.execute_pose_plan(&p).unwrap()
}
fn part(id: &str) -> PosePartRef {
    PosePartRef::Resolved { part_id: id.into() }
}
#[test]
fn empty_group_creates_singleton_in_one_history_and_persists_all_identities() {
    let mut s = AuthoringSession::new(&id(), Canvas::new(10., 10., Vec2::default(), 1.)).unwrap();
    let (p, g) = (id(), id());
    assert!(matches!(
        s.pose_collection_capabilities(&PoseCollection::Groups)
            .unwrap()
            .create,
        CapabilityState::Available
    ));
    let r = run(
        &mut s,
        PoseEdit::CreateGroup {
            pose_id: p.clone(),
            id: g.clone(),
            name: "Empty".into(),
            before: None,
        },
    );
    assert_eq!(r.changes.created.len(), 2);
    let old = s.handle(ObjectKind::PoseGroup, &g).unwrap();
    assert!(matches!(
        s.pose_capabilities(&old).unwrap().dissolve,
        CapabilityState::Unavailable {
            code: "POSE_RECEIVING_GROUP_REQUIRED"
        }
    ));
    s.undo().unwrap();
    assert!(s.document().pose().is_none());
    s.redo().unwrap();
    assert!(s.resolve_handle(&old).is_err());
    let e = entry(&mut s, &g);
    let l = link(&mut s, &e);
    let d = kasane_project::decode_project(&kasane_project::encode_project(s.document()).unwrap())
        .unwrap();
    assert_eq!(d.pose(), s.document().pose());
    assert!(d.validate_structure().is_empty());
    assert!(d.get_pose_link(&l).is_some());
    let proj = d.pose_projection().unwrap().unwrap();
    assert!(proj.groups[0].entries.is_empty());
    assert_eq!(proj.disabled.len(), 2);
}
#[test]
fn fields_follow_ids_after_reorder_but_expire_after_cross_owner_roundtrip() {
    let (mut s, _, g, h, a, _) = setup();
    let e = entry(&mut s, &g);
    let x = entry(&mut s, &g);
    let handle = s.handle(ObjectKind::PoseEntry, &e).unwrap();
    let address = s.pose_field_address(&e).unwrap();
    run(
        &mut s,
        PoseEdit::Move {
            ids: vec![x.clone()],
            destination: PoseCollection::Entries {
                group_id: g.clone(),
            },
            before: Some(e.clone()),
        },
    );
    let p = s
        .plan_pose_field_patch(&address, PoseFields::Entry { part: part(&a) })
        .unwrap();
    s.execute_pose_plan(&p).unwrap();
    assert_eq!(s.document().get_pose_entry(&e).unwrap().1.part, part(&a));
    let address = s.pose_field_address(&e).unwrap();
    for group in [&h, &g] {
        run(
            &mut s,
            PoseEdit::Move {
                ids: vec![e.clone()],
                destination: PoseCollection::Entries {
                    group_id: group.clone(),
                },
                before: None,
            },
        );
    }
    assert!(s.resolve_handle(&handle).is_ok());
    assert_eq!(
        s.plan_pose_field_patch(&address, address.expected().clone())
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );
    let address = s.pose_field_address(&e).unwrap();
    run(
        &mut s,
        PoseEdit::Delete {
            ids: vec![e.clone()],
        },
    );
    s.undo().unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    assert!(s
        .plan_pose_field_patch(&address, address.expected().clone())
        .is_err());
}
#[test]
fn copy_remaps_owned_ids_and_dissolve_preserves_shared_targets_and_handles() {
    let (mut s, p, g, h, a, b) = setup();
    let e = entry(&mut s, &g);
    patch(&mut s, &e, PoseFields::Entry { part: part(&a) });
    let l = link(&mut s, &e);
    patch(&mut s, &l, PoseFields::Link { target: part(&b) });
    let r = run(
        &mut s,
        PoseEdit::Duplicate {
            ids: vec![g.clone(), e.clone()],
            destination: PoseCollection::Groups,
            before: None,
        },
    );
    assert_eq!(r.changes.created.len(), 3);
    let d = s.document();
    let copied = &d.pose().unwrap().groups[2];
    assert_ne!(copied.id, g);
    assert_ne!(copied.entries[0].id, e);
    assert_ne!(copied.entries[0].links.as_ref().unwrap()[0].id, l);
    assert_eq!(copied.entries[0].part, part(&a));
    let handle = s.handle(ObjectKind::PoseEntry, &e).unwrap();
    let r = run(
        &mut s,
        PoseEdit::DissolveGroup {
            id: g.clone(),
            destination: h.clone(),
            before: None,
        },
    );
    assert_eq!(r.changes.deleted.len(), 1);
    assert_eq!(s.document().pose_owner(&e), Some(h.as_str()));
    assert!(s.resolve_handle(&handle).is_ok());
    assert_eq!(s.document().get_pose_link(&l).unwrap().1.target, part(&b));
    assert!(s
        .edit("Referenced Part", None, |e| e.erase_object(&a))
        .is_err());
    run(&mut s, PoseEdit::Delete { ids: vec![p] });
    assert!(s.document().get_part(&a).is_some());
    s.undo().unwrap();
    assert!(s.document().pose().is_some());
}
#[test]
fn invalid_joint_moves_duplicate_targets_and_stale_plans_are_atomic() {
    let (mut s, _, g, h, a, _) = setup();
    let e = entry(&mut s, &g);
    patch(&mut s, &e, PoseFields::Entry { part: part(&a) });
    let x = entry(&mut s, &h);
    patch(&mut s, &x, PoseFields::Entry { part: part(&a) });
    let v = s.version();
    let history = s.history_state();
    assert_eq!(
        s.plan_pose_edit(PoseEdit::Move {
            ids: vec![e.clone()],
            destination: PoseCollection::Entries {
                group_id: h.clone()
            },
            before: None
        })
        .unwrap_err()
        .code
        .as_ref(),
        "DUPLICATE_POSE_PART"
    );
    assert_eq!(s.version(), v);
    assert_eq!(s.history_state(), history);
    let p = s
        .plan_pose_edit(PoseEdit::Delete {
            ids: vec![e.clone()],
        })
        .unwrap();
    entry(&mut s, &g);
    let v = s.version();
    assert!(s.execute_pose_plan(&p).is_err());
    assert_eq!(s.version(), v);
    let v = s.version();
    assert!(s
        .edit("Joint invalid", None, |edit| {
            edit.edit_pose(PoseEdit::Delete {
                ids: vec![e.clone()],
            })?;
            edit.edit_pose(PoseEdit::Move {
                ids: vec![x.clone()],
                destination: PoseCollection::Links {
                    entry_id: x.clone(),
                },
                before: None,
            })
        })
        .is_err());
    assert_eq!(s.version(), v);
    assert!(s.document().get_pose_entry(&e).is_some());
}
#[test]
fn direct_replacement_and_generic_delete_obey_descendant_locks() {
    let (mut s, p, g, _, a, _) = setup();
    let e = entry(&mut s, &g);
    let l = link(&mut s, &e);
    s.edit("Lock", None, |edit| {
        let mut state = edit.candidate_document().editor_state().clone();
        state.objects.insert(
            l.clone(),
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        edit.replace_editor_state(state)
    })
    .unwrap();
    let h = s.handle(ObjectKind::PoseGroup, &g).unwrap();
    assert!(matches!(
        s.pose_capabilities(&h).unwrap().delete,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert_eq!(
        s.plan_pose_edit(PoseEdit::Delete { ids: vec![g] })
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    let mut replacement = s.pose().unwrap();
    replacement.groups.clear();
    assert!(s
        .edit("Replace", None, |edit| edit.set_pose(replacement))
        .is_err());
    assert!(s
        .edit("Delete", None, |edit| edit.erase_object(&p))
        .is_err());
    s.edit("Unlock", None, |edit| {
        let mut state = edit.candidate_document().editor_state().clone();
        state.objects.remove(&l);
        edit.replace_editor_state(state)
    })
    .unwrap();
    patch(&mut s, &l, PoseFields::Link { target: part(&a) });
    let handle = s.handle(ObjectKind::PoseLink, &l).unwrap();
    let original = s.pose().unwrap();
    let mut without = original.clone();
    without.groups[0].entries[0].links = None;
    s.edit("Replace and restore", None, |edit| {
        edit.set_pose(without)?;
        edit.set_pose(original)
    })
    .unwrap();
    assert!(s.resolve_handle(&handle).is_err());
}
#[test]
fn no_op_keeps_redo_and_label_changes_do_not_invalidate_evaluation() {
    let (mut s, _, g, _, _, _) = setup();
    let e = entry(&mut s, &g);
    s.undo().unwrap();
    let history = s.history_state();
    let v = s.version();
    let address = s.pose_field_address(&g).unwrap();
    let p = s
        .plan_pose_field_patch(&address, address.expected().clone())
        .unwrap();
    let r = s.execute_pose_plan(&p).unwrap();
    assert!(!r.changed);
    assert_eq!(s.version(), v);
    assert_eq!(s.history_state(), history);
    s.redo().unwrap();
    assert!(s.document().get_pose_entry(&e).is_some());
    let evaluation = s.evaluation_revision();
    let frame = s.preview_frame().unwrap();
    let r = patch(
        &mut s,
        &g,
        PoseFields::Group {
            name: "Renamed".into(),
        },
    );
    assert!(r.changes.categories.contains(&ChangeCategory::Labels));
    assert!(!r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert_eq!(s.evaluation_revision(), evaluation);
    assert!(std::sync::Arc::ptr_eq(&frame, &s.preview_frame().unwrap()));
    s.undo().unwrap();
    assert_eq!(s.evaluation_revision(), evaluation);
    s.redo().unwrap();
    assert_eq!(s.evaluation_revision(), evaluation);
}
#[test]
fn projection_compacts_entries_and_links_and_export_uses_identical_omissions() {
    let (mut s, _, g, _, a, b) = setup();
    let unready = entry(&mut s, &g);
    let dependent = link(&mut s, &unready);
    patch(&mut s, &dependent, PoseFields::Link { target: part(&b) });
    let e = entry(&mut s, &g);
    patch(&mut s, &e, PoseFields::Entry { part: part(&a) });
    let unlinked = link(&mut s, &e);
    let l = link(&mut s, &e);
    patch(&mut s, &l, PoseFields::Link { target: part(&b) });
    let p = s.document().pose_projection().unwrap().unwrap();
    assert_eq!(p.slots[&e], vec![0, 0]);
    assert_eq!(p.slots[&l], vec![0, 0, 0]);
    assert!(!p.slots.contains_key(&unready));
    assert!(!p.slots.contains_key(&unlinked));
    assert!(p
        .disabled
        .iter()
        .any(|v| v.id == dependent
            && matches!(v.reason, PoseDisabledReason::OwnerUnavailable { .. })));
    assert_eq!(
        s.export_pose3().unwrap_err().code.as_ref(),
        "UNCONFIGURED_PART"
    );
    let export = s.plan_available_pose_export().unwrap();
    assert_eq!(export.omitted.len(), 3);
    let text = s.export_pose_plan(&export).unwrap().unwrap();
    assert!(!text.contains(&e));
    assert!(!text.contains(&l));
    assert!(text.contains("PartA"));
    let imported =
        kasane_project::import_pose3(s.document(), &s.pose().unwrap().id, &text).unwrap();
    assert_ne!(
        imported.candidate.pose().unwrap().groups[0].entries[0].id,
        e
    );
    let mut preview = s.motion_preview();
    let snapshot = preview.advance(0.1).unwrap();
    assert_eq!(snapshot.part_opacities[&a], 1.);
    assert_eq!(snapshot.part_opacities[&b], 1.);
    patch(
        &mut s,
        &unready,
        PoseFields::Entry {
            part: PosePartRef::Unresolved {
                runtime_id: "Missing".into(),
            },
        },
    );
    assert!(s.export_pose_plan(&export).is_err());
    let p = s.document().pose_projection().unwrap().unwrap();
    assert!(p.disabled.iter().any(|v|matches!(&v.reason,PoseDisabledReason::Unresolved { runtime_id } if runtime_id=="Missing")));
}
#[test]
fn link_repair_changesets_are_directional_for_direct_edits_and_history() {
    let (mut s, _, g, _, a, _) = setup();
    let e = entry(&mut s, &g);
    let l = link(&mut s, &e);
    let r = patch(&mut s, &l, PoseFields::Link { target: part(&a) });
    let edge = &r.changes.pose_references[0];
    assert_eq!(edge.before, Some(PosePartRef::Unspecified));
    assert_eq!(edge.after, Some(part(&a)));
    let r = s.undo().unwrap();
    assert_eq!(r.changes.pose_references[0].before, Some(part(&a)));
    assert_eq!(
        r.changes.pose_references[0].after,
        Some(PosePartRef::Unspecified)
    );
    s.redo().unwrap();
    let address = s.pose_field_address(&l).unwrap();
    run(
        &mut s,
        PoseEdit::Delete {
            ids: vec![l.clone()],
        },
    );
    s.undo().unwrap();
    assert!(s
        .plan_pose_field_patch(&address, PoseFields::Link { target: part(&a) })
        .is_err());
}
#[test]
fn field_patches_share_unrelated_pose_storage_and_publish_extension_changes() {
    let (mut s, _, g, h, a, _) = setup();
    let first = entry(&mut s, &g);
    let second = entry(&mut s, &h);
    let l = link(&mut s, &second);
    let before = s.document().clone();
    patch(&mut s, &second, PoseFields::Entry { part: part(&a) });
    assert_eq!(
        before.get_pose_group(&g).unwrap().entries.as_ptr(),
        s.document().get_pose_group(&g).unwrap().entries.as_ptr()
    );
    assert_eq!(
        before
            .get_pose_entry(&second)
            .unwrap()
            .1
            .links
            .as_ref()
            .unwrap()
            .as_ptr(),
        s.document()
            .get_pose_entry(&second)
            .unwrap()
            .1
            .links
            .as_ref()
            .unwrap()
            .as_ptr()
    );
    assert_eq!(
        before.get_pose_entry(&second).unwrap().1.part,
        PosePartRef::Unspecified
    );
    let mut replacement = s.pose().unwrap();
    replacement.groups[0].entries[0]
        .extensions
        .insert("Future".into(), serde_json::json!({"keep":true}));
    let (_, receipt) = s
        .edit("Explicit extensions", None, |e| e.set_pose(replacement))
        .unwrap();
    assert!(receipt.changes.modified.iter().any(|e| e.id == first));
    assert!(receipt
        .changes
        .categories
        .contains(&ChangeCategory::Content));
    assert!(receipt
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert!(s.document().get_pose_link(&l).is_some());
}
#[test]
fn identity_collisions_and_missing_resolved_targets_cannot_be_saved() {
    let (mut s, _, g, _, a, _) = setup();
    let e = entry(&mut s, &g);
    let v = s.version();
    assert!(s
        .plan_pose_edit(PoseEdit::CreateEntry {
            group_id: g.clone(),
            id: a,
            before: None
        })
        .is_err());
    let mut replacement = s.pose().unwrap();
    replacement.groups[0].entries[0].id = replacement.groups[0].id.clone();
    assert!(s
        .edit("Duplicate ID", None, |e| e.set_pose(replacement))
        .is_err());
    let address = s.pose_field_address(&e).unwrap();
    assert!(s
        .plan_pose_field_patch(&address, PoseFields::Entry { part: part(&id()) })
        .is_err());
    assert_eq!(s.version(), v);
    let mut wire: serde_json::Value =
        serde_json::from_str(&kasane_project::encode_project(s.document()).unwrap()).unwrap();
    assert_eq!(wire["format_version"], 16);
    wire["format_version"] = 15.into();
    assert_eq!(
        kasane_project::decode_project(&wire.to_string())
            .unwrap_err()
            .code,
        "UNSUPPORTED_VERSION"
    );
}
#[test]
fn multi_source_moves_use_native_order_and_no_op_reordering_preserves_redo() {
    let (mut s, _, g, h, _, _) = setup();
    let a = entry(&mut s, &g);
    let b = entry(&mut s, &g);
    let c = entry(&mut s, &h);
    run(
        &mut s,
        PoseEdit::Move {
            ids: vec![c.clone(), b.clone(), a.clone()],
            destination: PoseCollection::Entries {
                group_id: h.clone(),
            },
            before: None,
        },
    );
    assert_eq!(
        s.document()
            .get_pose_group(&h)
            .unwrap()
            .entries
            .iter()
            .map(|e| &e.id)
            .collect::<Vec<_>>(),
        vec![&a, &b, &c]
    );
    let l = link(&mut s, &c);
    s.undo().unwrap();
    let history = s.history_state();
    let v = s.version();
    let r = run(
        &mut s,
        PoseEdit::Move {
            ids: vec![c],
            destination: PoseCollection::Entries { group_id: h },
            before: None,
        },
    );
    assert!(!r.changed);
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), v);
    s.redo().unwrap();
    assert!(s.document().get_pose_link(&l).is_some());
}
