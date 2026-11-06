use zipzi_contract::authoring::ObjectKind;
use zipzi_core::document::{
    ReferenceCollection as C, ReferenceEdit as E, ReferenceFields as F,
    ReferenceSelectionField as S,
};
use zipzi_core::{document::*, *};
use zipzi_sdk::AuthoringSession;
fn id(n: u32) -> String {
    format!("30000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}
fn run(s: &mut AuthoringSession, e: E) -> zipzi_sdk::Publication {
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_reference(e)
    })
    .unwrap();
    s.publish(p.1).unwrap()
}
fn group(s: &mut AuthoringSession, n: u32) {
    run(
        s,
        E::CreateModelParameterGroup {
            id: id(n),
            name: "EyeBlink".into(),
            before: None,
        },
    );
}
fn set(s: &mut AuthoringSession, n: u32) {
    run(
        s,
        E::CreateCdiCombinedSet {
            id: id(n),
            before: None,
        },
    );
}
fn member(s: &mut AuthoringSession, n: u32, c: C) {
    run(
        s,
        E::CreateMember {
            id: id(n),
            collection: c,
            before: None,
        },
    );
}
fn parameter(s: &mut AuthoringSession, n: u32) {
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter(Parameter {
                    id: id(n),
                    runtime_id: format!("Param{n}"),
                    ..Default::default()
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Parameter")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn patch(s: &mut AuthoringSession, n: u32, f: F) -> zipzi_sdk::Publication {
    let expected = s.snapshot().document().reference_fields(&id(n)).unwrap();
    let p = crate::support::prepared(s, |c| {
        c.edit_reference(E::Patch {
            id: id(n),
            expected,
            fields: f,
        })
    })
    .unwrap();
    s.publish(p.1).unwrap()
}
fn lock(s: &mut AuthoringSession, n: u32) {
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        id(n),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn mesh(s: &mut AuthoringSession, n: u32, nonempty: bool) {
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_mesh({
                    let mut value = Mesh {
                        id: id(n),
                        vertex_ids: if nonempty { vec![1] } else { vec![] },
                        base_positions: if nonempty {
                            vec![Vec2::new(2., 3.)]
                        } else {
                            vec![]
                        },
                        uvs: if nonempty {
                            vec![Vec2::new(0., 0.)]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Mesh")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
#[test]
fn empty_roots_pending_members_and_hit_areas_are_native_and_persistent() {
    let mut s = session();
    let eval = s.snapshot().content_id();
    group(&mut s, 100);
    set(&mut s, 200);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    member(&mut s, 201, C::CdiCombinedMembers { set_id: id(200) });
    run(
        &mut s,
        E::CreateModelHitArea {
            id: id(300),
            name: String::new(),
            before: None,
        },
    );
    assert_ne!(s.snapshot().content_id(), eval);
    assert_eq!(
        s.snapshot()
            .document()
            .reference_projection()
            .disabled
            .len(),
        5
    );
    {
        let kind = ObjectKind::ModelParameterGroup;
        let object_id = (&id(100)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    {
        let kind = ObjectKind::ModelParameterMember;
        let object_id = (&id(101)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    {
        let kind = ObjectKind::ModelHitArea;
        let object_id = (&id(300)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    {
        let kind = ObjectKind::CdiCombinedMember;
        let object_id = (&id(201)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let restored = crate::support::roundtrip(s.snapshot());
    let d = restored.document();
    assert_eq!(
        d.model3_settings(),
        s.snapshot().document().model3_settings()
    );
    assert_eq!(d.display_info(), s.snapshot().document().display_info());
    assert!(zipzi_project::export_cdi3(s.snapshot().document()).is_err());
}
#[test]
fn moving_members_preserves_identity_and_owner_roundtrips_do_not_revive_addresses() {
    let mut s = session();
    group(&mut s, 100);
    group(&mut s, 110);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    member(&mut s, 102, C::ModelParameterMembers { group_id: id(100) });
    parameter(&mut s, 10);
    let fields = F::ModelParameterMember {
        target: ModelTargetRef::Resolved { object_id: id(10) },
    };
    patch(&mut s, 101, fields.clone());
    let stale = s.begin_draft();
    run(
        &mut s,
        E::Move {
            ids: vec![id(101)],
            destination: C::ModelParameterMembers { group_id: id(100) },
            before: None,
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .reference_collection(&C::ModelParameterMembers { group_id: id(100) })
            .unwrap(),
        [id(102), id(101)]
    );
    run(
        &mut s,
        E::Move {
            ids: vec![id(101)],
            destination: C::ModelParameterMembers { group_id: id(110) },
            before: None,
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .reference_identity(&id(101))
            .unwrap()
            .owner,
        Some(id(110))
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().reference_fields(&id(101)).unwrap(),
        fields
    );
    assert_eq!(
        s.publish(stale.prepare("old reference").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    patch(
        &mut s,
        100,
        F::ModelParameterGroup {
            name: "LipSync".into(),
        },
    );
    assert_eq!(
        s.snapshot().document().reference_fields(&id(101)).unwrap(),
        fields
    );
}

#[test]
fn copies_allocate_owned_ids_and_keep_parameter_and_mesh_targets_shared() {
    let mut s = session();
    parameter(&mut s, 10);
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Resolved { object_id: id(10) },
        },
    );
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_reference(E::Duplicate {
            ids: vec![id(100), id(101)],
            destination: C::ModelParameterGroups,
            before: None,
        })
    })
    .unwrap();
    let g = p.0.copied_ids[&id(100)].clone();
    let m = p.0.copied_ids[&id(101)].clone();
    assert_ne!(g, id(100));
    s.publish(p.1).unwrap();
    assert_eq!(
        match s.snapshot().document().reference_fields(&m).unwrap() {
            F::ModelParameterMember { target } => target,
            _ => panic!(),
        },
        ModelTargetRef::Resolved { object_id: id(10) }
    );
    assert_eq!(
        s.snapshot()
            .document()
            .reference_identity(&m)
            .unwrap()
            .owner,
        Some(g)
    );
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_reference(
            E::Duplicate {
                ids: vec![id(101)],
                destination: C::ModelParameterMembers { group_id: id(100) },
                before: None
            }
        ))
        .is_err()
    );
    run(&mut s, E::Delete { ids: vec![id(100)] });
    assert!(!s
        .snapshot()
        .document()
        .contains_object(ObjectKind::ModelParameterMember, &id(101)));
    assert!(s.snapshot().document().parameter(&id(10)).is_some());
}
#[test]
fn dissolve_keeps_members_but_rejects_duplicate_business_targets_atomically() {
    let mut s = session();
    group(&mut s, 100);
    group(&mut s, 110);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    let h = {
        let kind = ObjectKind::ModelParameterMember;
        let object_id = (&id(101)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    run(
        &mut s,
        E::Dissolve {
            id: id(100),
            destination: id(110),
            before: None,
        },
    );
    s.snapshot().document().require_object(h.0, &h.1).unwrap();
    assert!(s
        .snapshot()
        .document()
        .model_parameter_group(&id(100))
        .is_none());
    assert_eq!(
        s.snapshot()
            .document()
            .reference_identity(&id(101))
            .unwrap()
            .owner,
        Some(id(110))
    );
    set(&mut s, 200);
    set(&mut s, 210);
    member(&mut s, 201, C::CdiCombinedMembers { set_id: id(200) });
    member(&mut s, 211, C::CdiCombinedMembers { set_id: id(210) });
    parameter(&mut s, 10);
    for n in [201, 211] {
        patch(
            &mut s,
            n,
            F::CdiCombinedMember {
                target: CdiParameterRef::Resolved {
                    parameter_id: id(10),
                },
            },
        );
    }
    let v = s.version();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_reference(
            E::Dissolve {
                id: id(200),
                destination: id(210),
                before: None
            }
        ))
        .is_err()
    );
    assert_eq!(s.version(), v);
    assert!(s
        .snapshot()
        .document()
        .contains_object(ObjectKind::CdiCombinedSet, &id(200)));
}
#[test]
fn resolved_target_deletion_reports_reference_row_ids() {
    let mut s = session();
    parameter(&mut s, 10);
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Resolved { object_id: id(10) },
        },
    );
    set(&mut s, 200);
    member(&mut s, 201, C::CdiCombinedMembers { set_id: id(200) });
    patch(
        &mut s,
        201,
        F::CdiCombinedMember {
            target: CdiParameterRef::Resolved {
                parameter_id: id(10),
            },
        },
    );
    let references = s.snapshot().document().references_to(&id(10));
    assert!(references.contains(&id(101)));
    assert!(references.contains(&id(201)));
    let mut draft = s.begin_draft();
    let outcome = draft
        .batch(draft.source(), |c| {
            c.review_deletion(
                &DeletionRequest {
                    roots: vec![c
                        .read_document()?
                        .deletion_target(ObjectKind::Parameter, &id(10))?],
                    ..Default::default()
                },
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap();
    assert!(
        outcome
            .report
            .rewritten
            .iter()
            .any(|r| r.owner.id == id(101))
            || outcome
                .report
                .removed
                .iter()
                .any(|r| r.object.id == id(101))
    );
    assert!(
        outcome
            .report
            .rewritten
            .iter()
            .any(|r| r.owner.id == id(201))
            || outcome
                .report
                .removed
                .iter()
                .any(|r| r.object.id == id(201))
    );
    draft.cancel();
    mesh(&mut s, 20, true);
    run(
        &mut s,
        E::CreateModelHitArea {
            id: id(300),
            name: "Head".into(),
            before: None,
        },
    );
    patch(
        &mut s,
        300,
        F::ModelHitArea {
            name: "Head".into(),
            mesh: ModelTargetRef::Resolved { object_id: id(20) },
        },
    );
    assert_eq!(
        s.snapshot().document().references_to(&id(20)),
        vec![id(300)]
    );
    assert_eq!(
        s.snapshot().document().reference_projection().hit_areas[0].id,
        id(300)
    );
    assert!(!s
        .snapshot()
        .document()
        .scene_projection()
        .drawing
        .contains(&id(20)));
}
#[test]
fn pending_slots_keep_native_ids_and_compact_runtime_maps_are_not_addresses() {
    let mut s = session();
    parameter(&mut s, 10);
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    member(&mut s, 102, C::ModelParameterMembers { group_id: id(100) });
    patch(
        &mut s,
        102,
        F::ModelParameterMember {
            target: ModelTargetRef::Resolved { object_id: id(10) },
        },
    );
    assert_eq!(
        s.snapshot().document().reference_projection().groups[0].member_slots[&id(102)],
        0
    );
    assert_eq!(
        s.snapshot()
            .document()
            .model_parameter_group(&id(100))
            .unwrap()
            .parameters[1]
            .id,
        id(102)
    );
    let a = s.snapshot().document().reference_fields(&id(102)).unwrap();
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Unresolved {
                runtime_id: "ImportedMissing".into(),
            },
        },
    );
    assert!(matches!(
        a,
        F::ModelParameterMember { .. } | F::CdiCombinedMember { .. }
    ));
    run(
        &mut s,
        E::CreateModelHitArea {
            id: id(300),
            name: "Body".into(),
            before: None,
        },
    );
    mesh(&mut s, 20, false);
    patch(
        &mut s,
        300,
        F::ModelHitArea {
            name: "Body".into(),
            mesh: ModelTargetRef::Resolved { object_id: id(20) },
        },
    );
    assert!(s
        .snapshot()
        .document()
        .reference_projection()
        .hit_areas
        .is_empty());
}
#[test]
fn selection_fields_use_native_validation_and_monotonic_addresses() {
    let mut s = session();
    mesh(&mut s, 20, true);
    mesh(&mut s, 21, true);
    let stale = s.begin_draft();
    let (_, p) = selection(&s, 20, S::MeshMasks, vec![id(21)]).unwrap();
    s.publish(p).unwrap();
    assert_eq!(
        s.snapshot().document().mesh(&id(20)).unwrap().masks,
        [id(21)]
    );
    s.undo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .mesh(&id(20))
        .unwrap()
        .masks
        .is_empty());
    assert_eq!(
        s.publish(stale.prepare("old masks").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert!(selection(&s, 20, S::MeshMasks, vec![id(999)]).is_err());
    assert!(selection(&s, 20, S::MeshMasks, vec![id(21), id(21)]).is_err());
}

#[test]
fn lock_guards_cover_raw_replacements_and_deleted_ids_cannot_revive() {
    let mut s = session();
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    lock(&mut s, 101);
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_reference(E::Delete { ids: vec![id(100)] }))
        .is_err()
    );
    let mut settings = s.snapshot().document().model3_settings().clone();
    settings.groups = Some(vec![]);
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_model3_settings(settings))
                .and_then(|value| {
                    let prepared = draft.prepare("Bypass")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    set(&mut s, 200);
    member(&mut s, 201, C::CdiCombinedMembers { set_id: id(200) });
    let h = {
        let kind = ObjectKind::CdiCombinedMember;
        let object_id = (&id(201)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let a = s.snapshot().document().reference_fields(&id(201)).unwrap();
    run(&mut s, E::Delete { ids: vec![id(200)] });
    s.undo(s.version()).unwrap();
    assert!((s.read_snapshot().source != h.2));
    assert!(matches!(a, F::CdiCombinedMember { .. }));
}
#[test]
fn failed_batches_noops_and_wrong_collections_leave_history_unchanged() {
    let mut s = session();
    group(&mut s, 100);
    let v = s.version();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![id(100)],
            destination: C::ModelParameterGroups,
            before: None,
        },
    );
    assert!(r.writes.is_empty());
    assert_eq!(s.version(), v);
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_reference(
            E::Move {
                ids: vec![id(100)],
                destination: C::CdiCombinedSets,
                before: Some(id(100)),
            }
        ))
        .is_err()
    );
    let err = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_reference(E::CreateCdiCombinedSet {
                    id: id(200),
                    before: None,
                })?;
                e.edit_reference(E::Move {
                    ids: vec![id(100)],
                    destination: C::CdiCombinedSets,
                    before: None,
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Batch")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap_err();
    assert_eq!(err.code.as_str(), "WRONG_REFERENCE_COLLECTION");
    assert_eq!(s.version(), v);
    assert!(!s
        .snapshot()
        .document()
        .contains_object(ObjectKind::CdiCombinedSet, &id(200)));
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_reference(E::CreateModelHitArea {
            id: id(300),
            name: "".into(),
            before: None,
        })
    })
    .unwrap();
    parameter(&mut s, 10);
    assert!(s.publish(p.1).is_err());
}
#[test]
fn cdi_import_matches_set_contents_and_retains_member_identities_when_reordered() {
    let mut s = session();
    parameter(&mut s, 10);
    parameter(&mut s, 11);
    set(&mut s, 200);
    set(&mut s, 210);
    member(&mut s, 201, C::CdiCombinedMembers { set_id: id(200) });
    member(&mut s, 211, C::CdiCombinedMembers { set_id: id(210) });
    patch(
        &mut s,
        201,
        F::CdiCombinedMember {
            target: CdiParameterRef::Resolved {
                parameter_id: id(10),
            },
        },
    );
    patch(
        &mut s,
        211,
        F::CdiCombinedMember {
            target: CdiParameterRef::Resolved {
                parameter_id: id(11),
            },
        },
    );
    let h = {
        let kind = ObjectKind::CdiCombinedMember;
        let object_id = (&id(201)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let mut wire: serde_json::Value =
        serde_json::from_str(&zipzi_project::export_cdi3(s.snapshot().document()).unwrap())
            .unwrap();
    wire["CombinedParameters"].as_array_mut().unwrap().reverse();
    let text = wire.to_string();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let imported = zipzi_project::import_cdi3(&e.read_document()?, &text)
                    .map_err(|e| zipzi_sdk::Status::error(e.code, e.path))?;
                e.replace_display_info(imported.candidate.display_info().clone())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Import reordered")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .reference_collection(&C::CdiCombinedSets)
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        [id(210), id(200)]
    );
    s.snapshot().document().require_object(h.0, &h.1).unwrap();
    assert!(s
        .snapshot()
        .document()
        .contains_object(ObjectKind::CdiCombinedMember, &id(211)));
}
#[test]
fn model3_reports_pending_identity_and_foreign_metadata_uses_runtime_ids() {
    let mut s = session();
    parameter(&mut s, 10);
    mesh(&mut s, 20, true);
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    run(
        &mut s,
        E::CreateModelHitArea {
            id: id(300),
            name: "Head".into(),
            before: None,
        },
    );
    let e = zipzi_project::model3::export_settings(s.snapshot().document()).unwrap_err();
    assert!(e.message.contains(&id(101)));
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Resolved { object_id: id(10) },
        },
    );
    let e = zipzi_project::model3::export_settings(s.snapshot().document()).unwrap_err();
    assert!(e.message.contains(&id(300)));
    patch(
        &mut s,
        300,
        F::ModelHitArea {
            name: "Head".into(),
            mesh: ModelTargetRef::Resolved { object_id: id(20) },
        },
    );
    let fields = zipzi_project::model3::export_settings(s.snapshot().document()).unwrap();
    assert_eq!(fields["Groups"][0]["Ids"], serde_json::json!(["Param10"]));
    let text = serde_json::to_string(&fields).unwrap();
    for n in [100, 101, 300] {
        assert!(!text.contains(&id(n)));
    }
    let imported = zipzi_project::model3::import_settings(
        s.snapshot().document(),
        &serde_json::Value::Object(fields),
    )
    .unwrap();
    assert_ne!(imported.groups.as_ref().unwrap()[0].id, id(100));
    assert_eq!(
        imported.groups.as_ref().unwrap()[0].parameters[0].target,
        ModelTargetRef::Resolved { object_id: id(10) }
    );
}
#[test]
fn raw_remove_reinsert_does_not_revive_handles_and_unresolved_duplicates_are_rejected() {
    let mut s = session();
    parameter(&mut s, 10);
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    member(&mut s, 102, C::ModelParameterMembers { group_id: id(100) });
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Resolved { object_id: id(10) },
        },
    );
    assert!(crate::support::prepared(&s, |c| c.edit_reference(E::Patch {
        id: id(102),
        expected: s.snapshot().document().reference_fields(&id(102)).unwrap(),
        fields: F::ModelParameterMember {
            target: ModelTargetRef::Unresolved {
                runtime_id: "Param10".into()
            }
        }
    }))
    .is_err());
    let h = {
        let kind = ObjectKind::ModelParameterMember;
        let object_id = (&id(101)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let settings = s.snapshot().document().model3_settings().clone();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut empty = settings.clone();
                empty.groups = Some(vec![]);
                e.replace_model3_settings(empty)?;
                e.replace_model3_settings(settings)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Replace identities")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(s.read_snapshot().source, h.2);
    assert!(s
        .snapshot()
        .document()
        .contains_object(ObjectKind::ModelParameterMember, &id(101)));
}

#[test]
fn replacing_a_target_incarnation_does_not_revive_an_unchanged_field_address() {
    let mut s = session();
    parameter(&mut s, 10);
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    let configured = F::ModelParameterMember {
        target: ModelTargetRef::Resolved { object_id: id(10) },
    };
    patch(&mut s, 101, configured.clone());
    let stale = s.begin_draft();
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Unspecified,
        },
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().reference_fields(&id(101)).unwrap(),
        configured
    );
    assert_eq!(
        s.publish(stale.prepare("old target").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
}

#[test]
fn offscreen_masks_and_blend_constraints_share_reference_validation_and_history() {
    let mut s = session();
    mesh(&mut s, 20, true);
    mesh(&mut s, 21, true);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(30),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
                        id: id(31),
                        part_id: id(30),
                        ..Default::default()
                    },
                )?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(10),
                        kind: ParameterKind::BlendShape,
                        minimum: 0.,
                        maximum: 1.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(11),
                        minimum: 0.,
                        maximum: 1.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateTable {
                        table: BlendShapeKeyTable {
                            id: id(40),
                            parameter_id: id(10),
                            keys: vec![0., 1.],
                            base_key_idx: 0,
                            samples: Default::default(),
                            base_sample_id: Default::default(),
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateConstraint {
                        constraint: BlendShapeConstraint {
                            id: id(41),
                            parameter_id: id(11),
                            keys: vec![0., 1.],
                            weights: vec![1., 1.],
                            samples: Default::default(),
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                {
                    let binding = BlendShapeBinding {
                        id: id(42),
                        target_id: id(20),
                        target_kind: BlendShapeTargetKind::Mesh,
                        key_table_id: id(40),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Mesh(vec![
                            DeltaMeshKeyform {
                                positions: vec![Vec2::default()],
                                ..Default::default()
                            };
                            2
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
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("Reference owners")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    for (owner, field, targets) in [
        (31, S::OffscreenMasks, vec![id(21)]),
        (42, S::BlendConstraints, vec![id(41)]),
    ] {
        assert!(selection(&s, owner, field, vec![id(999)]).is_err());
        assert!(selection(&s, owner, field, vec![targets[0].clone(); 2]).is_err());
        let stale = s.begin_draft();
        let (outcome, p) = selection(&s, owner, field, targets.clone()).unwrap();
        assert!(!outcome.effects.metadata_only);
        assert_eq!(outcome.effects.drawing_changed, field == S::OffscreenMasks);
        assert_eq!(
            outcome.effects.deformation_changed,
            field == S::BlendConstraints
        );
        s.publish(p).unwrap();
        assert_eq!(
            s.snapshot()
                .document()
                .reference_selection(&id(owner), field)
                .unwrap(),
            targets
        );
        s.undo(s.version()).unwrap();
        assert_eq!(
            s.publish(stale.prepare("old selection").unwrap())
                .unwrap_err()
                .code,
            "STALE_SOURCE"
        );
        s.redo(s.version()).unwrap();
    }
    assert_eq!(
        s.snapshot().document().offscreen(&id(31)).unwrap().masks,
        [id(21)]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .blend_binding(&id(42))
            .unwrap()
            .constraint_ids,
        [id(41)]
    );
    lock(&mut s, 30);
    assert!(selection(&s, 31, S::OffscreenMasks, vec![]).is_err());
    lock(&mut s, 20);
    assert!(selection(&s, 42, S::BlendConstraints, vec![]).is_err());
}
fn selection(
    s: &AuthoringSession,
    n: u32,
    field: S,
    targets: Vec<String>,
) -> Result<
    (
        zipzi_core::state::ReferenceOutcome,
        zipzi_sdk::PreparedChange,
    ),
    zipzi_sdk::Status,
> {
    let expected = s
        .snapshot()
        .document()
        .reference_selection(&id(n), field)?
        .to_vec();
    crate::support::prepared(s, |c| {
        c.edit_reference(E::ReplaceSelection {
            owner: id(n),
            field,
            expected,
            targets,
        })
    })
}
