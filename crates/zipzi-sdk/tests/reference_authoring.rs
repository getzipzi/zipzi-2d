use zipzi_core::{document::*, *};
use zipzi_sdk::{
    AuthoringSession, ObjectKind, ReferenceCollection as C, ReferenceEdit as E,
    ReferenceFields as F, ReferenceSelectionField as S,
};
fn id(n: u32) -> String {
    format!("30000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}
fn run(s: &mut AuthoringSession, e: E) -> zipzi_sdk::EditReceipt {
    let p = s.plan_reference_edit(e).unwrap();
    s.execute_reference_plan(&p).unwrap()
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
    s.edit("Parameter", None, |e| {
        e.create_parameter(Parameter {
            id: id(n),
            runtime_id: format!("Param{n}"),
            ..Default::default()
        })
    })
    .unwrap();
}
fn patch(s: &mut AuthoringSession, n: u32, f: F) -> zipzi_sdk::EditReceipt {
    let a = s.reference_field_address(&id(n)).unwrap();
    let p = s.plan_reference_field_patch(&a, f).unwrap();
    s.execute_reference_plan(&p).unwrap()
}
fn lock(s: &mut AuthoringSession, n: u32) {
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        id(n),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("Lock", None, |e| e.replace_editor_state(state))
        .unwrap();
}
fn mesh(s: &mut AuthoringSession, n: u32, nonempty: bool) {
    s.edit("Mesh", None, |e| {
        e.create_mesh(Mesh {
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
        })
    })
    .unwrap();
}
#[test]
fn empty_roots_pending_members_and_hit_areas_are_native_and_persistent() {
    let mut s = session();
    let eval = s.evaluation_revision();
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
    assert_eq!(s.evaluation_revision(), eval);
    assert_eq!(s.reference_projection().disabled.len(), 5);
    s.handle(ObjectKind::ModelParameterGroup, &id(100)).unwrap();
    s.handle(ObjectKind::ModelParameterMember, &id(101))
        .unwrap();
    s.handle(ObjectKind::ModelHitArea, &id(300)).unwrap();
    s.handle(ObjectKind::CdiCombinedMember, &id(201)).unwrap();
    let encoded = zipzi_project::encode_project(s.document()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["format_version"], 20);
    let d = zipzi_project::decode_project(&encoded).unwrap();
    assert_eq!(d.model3_settings(), s.document().model3_settings());
    assert_eq!(d.display_info(), s.document().display_info());
    assert!(d.validate_structure().is_empty());
    assert!(s.export_cdi3().is_err());
}
#[test]
fn moving_members_preserves_identity_and_owner_roundtrips_do_not_revive_addresses() {
    let mut s = session();
    group(&mut s, 100);
    group(&mut s, 110);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    member(&mut s, 102, C::ModelParameterMembers { group_id: id(100) });
    parameter(&mut s, 10);
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Resolved { object_id: id(10) },
        },
    );
    let handle = s
        .handle(ObjectKind::ModelParameterMember, &id(101))
        .unwrap();
    let a = s.reference_field_address(&id(101)).unwrap();
    run(
        &mut s,
        E::Move {
            ids: vec![id(101)],
            destination: C::ModelParameterMembers { group_id: id(100) },
            before: None,
        },
    );
    assert!(s
        .plan_reference_field_patch(&a, a.expected().clone())
        .is_ok());
    let r = run(
        &mut s,
        E::Move {
            ids: vec![id(101)],
            destination: C::ModelParameterMembers { group_id: id(110) },
            before: None,
        },
    );
    assert_eq!(r.changes.reference_fields[0].before_owner, Some(id(100)));
    s.resolve_handle(&handle).unwrap();
    s.undo().unwrap();
    assert!(s
        .plan_reference_field_patch(&a, a.expected().clone())
        .is_err());
    let a = s.reference_field_address(&id(101)).unwrap();
    patch(
        &mut s,
        100,
        F::ModelParameterGroup {
            name: "LipSync".into(),
        },
    );
    assert!(s
        .plan_reference_field_patch(&a, a.expected().clone())
        .is_ok());
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
    let p = s
        .plan_reference_edit(E::Duplicate {
            ids: vec![id(100), id(101)],
            destination: C::ModelParameterGroups,
            before: None,
        })
        .unwrap();
    let g = p.copied_ids()[&id(100)].clone();
    let m = p.copied_ids()[&id(101)].clone();
    assert_ne!(g, id(100));
    s.execute_reference_plan(&p).unwrap();
    assert_eq!(
        s.document().get_model_parameter_member(&m).unwrap().target,
        ModelTargetRef::Resolved { object_id: id(10) }
    );
    assert_eq!(s.document().reference_identity(&m).unwrap().owner, Some(g));
    assert!(s
        .plan_reference_edit(E::Duplicate {
            ids: vec![id(101)],
            destination: C::ModelParameterMembers { group_id: id(100) },
            before: None
        })
        .is_err());
    let r = run(&mut s, E::Delete { ids: vec![id(100)] });
    assert!(r.changes.deleted.iter().any(|e| e.id == id(101)));
    assert!(s.document().get_parameter(&id(10)).is_some());
}
#[test]
fn dissolve_keeps_members_but_rejects_duplicate_business_targets_atomically() {
    let mut s = session();
    group(&mut s, 100);
    group(&mut s, 110);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    let h = s
        .handle(ObjectKind::ModelParameterMember, &id(101))
        .unwrap();
    run(
        &mut s,
        E::Dissolve {
            id: id(100),
            destination: id(110),
            before: None,
        },
    );
    s.resolve_handle(&h).unwrap();
    assert!(s.document().get_model_parameter_group(&id(100)).is_none());
    assert_eq!(
        s.document().reference_identity(&id(101)).unwrap().owner,
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
    assert!(s
        .plan_reference_edit(E::Dissolve {
            id: id(200),
            destination: id(210),
            before: None
        })
        .is_err());
    assert_eq!(s.version(), v);
    assert!(s.document().get_combined_set(&id(200)).is_some());
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
    let error = s
        .edit("Delete", None, |e| e.erase_object(&id(10)))
        .unwrap_err();
    assert!(error.referrers.contains(&id(101)));
    assert!(error.referrers.contains(&id(201)));
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
    assert_eq!(s.document().references_to(&id(20)), vec![id(300)]);
    assert_eq!(s.reference_projection().hit_areas[0].id, id(300));
    assert!(!s.scene_projection().drawing.contains(&id(20)));
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
    assert_eq!(s.reference_projection().groups[0].member_slots[&id(102)], 0);
    assert_eq!(
        s.document()
            .get_model_parameter_group(&id(100))
            .unwrap()
            .parameters[1]
            .id,
        id(102)
    );
    let a = s.reference_field_address(&id(102)).unwrap();
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Unresolved {
                runtime_id: "ImportedMissing".into(),
            },
        },
    );
    assert!(s
        .plan_reference_field_patch(&a, a.expected().clone())
        .is_ok());
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
    assert!(s.reference_projection().hit_areas.is_empty());
}
#[test]
fn selection_fields_use_native_validation_and_monotonic_addresses() {
    let mut s = session();
    mesh(&mut s, 20, true);
    mesh(&mut s, 21, true);
    let a = s
        .reference_selection_address(&id(20), S::MeshMasks)
        .unwrap();
    let p = s.plan_reference_selection_patch(&a, vec![id(21)]).unwrap();
    let r = s.execute_reference_plan(&p).unwrap();
    assert_eq!(r.changes.reference_selections[0].after, [id(21)]);
    assert_eq!(s.document().get_mesh(&id(20)).unwrap().masks, [id(21)]);
    let undo = s.undo().unwrap();
    assert_eq!(undo.changes.reference_selections[0].before, [id(21)]);
    assert!(s.plan_reference_selection_patch(&a, vec![id(21)]).is_err());
    let a = s
        .reference_selection_address(&id(20), S::MeshMasks)
        .unwrap();
    assert!(s.plan_reference_selection_patch(&a, vec![id(999)]).is_err());
    assert!(s
        .plan_reference_selection_patch(&a, vec![id(21), id(21)])
        .is_err());
}
#[test]
fn lock_guards_cover_raw_replacements_and_deleted_ids_cannot_revive() {
    let mut s = session();
    group(&mut s, 100);
    member(&mut s, 101, C::ModelParameterMembers { group_id: id(100) });
    lock(&mut s, 101);
    assert!(s
        .plan_reference_edit(E::Delete { ids: vec![id(100)] })
        .is_err());
    let mut settings = s.document().model3_settings().clone();
    settings.groups = Some(vec![]);
    assert_eq!(
        s.edit("Bypass", None, |e| e.set_model3_settings(settings))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    set(&mut s, 200);
    member(&mut s, 201, C::CdiCombinedMembers { set_id: id(200) });
    let h = s.handle(ObjectKind::CdiCombinedMember, &id(201)).unwrap();
    let a = s.reference_field_address(&id(201)).unwrap();
    run(&mut s, E::Delete { ids: vec![id(200)] });
    s.undo().unwrap();
    assert!(s.resolve_handle(&h).is_err());
    assert!(s
        .plan_reference_field_patch(&a, a.expected().clone())
        .is_err());
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
    assert!(!r.changed);
    assert_eq!(s.version(), v);
    assert!(s
        .plan_reference_edit(E::Move {
            ids: vec![id(100)],
            destination: C::CdiCombinedSets,
            before: Some(id(100)),
        })
        .is_err());
    let err = s
        .edit("Batch", None, |e| {
            e.edit_references(E::CreateCdiCombinedSet {
                id: id(200),
                before: None,
            })?;
            e.edit_references(E::Move {
                ids: vec![id(100)],
                destination: C::CdiCombinedSets,
                before: None,
            })
        })
        .unwrap_err();
    assert_eq!(err.code.as_ref(), "WRONG_REFERENCE_COLLECTION");
    assert_eq!(s.version(), v);
    assert!(s.document().get_combined_set(&id(200)).is_none());
    let p = s
        .plan_reference_edit(E::CreateModelHitArea {
            id: id(300),
            name: "".into(),
            before: None,
        })
        .unwrap();
    parameter(&mut s, 10);
    assert!(s.execute_reference_plan(&p).is_err());
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
    let h = s.handle(ObjectKind::CdiCombinedMember, &id(201)).unwrap();
    let a = s.reference_field_address(&id(201)).unwrap();
    let mut wire: serde_json::Value = serde_json::from_str(&s.export_cdi3().unwrap()).unwrap();
    wire["CombinedParameters"].as_array_mut().unwrap().reverse();
    let text = wire.to_string();
    s.edit("Import reordered", None, |e| e.import_cdi3(&text))
        .unwrap();
    assert_eq!(
        s.reference_collection(&C::CdiCombinedSets)
            .unwrap()
            .iter()
            .map(|h| h.id().to_owned())
            .collect::<Vec<_>>(),
        [id(210), id(200)]
    );
    s.resolve_handle(&h).unwrap();
    assert!(s
        .plan_reference_field_patch(&a, a.expected().clone())
        .is_ok());
    assert!(s.document().get_combined_member(&id(211)).is_some());
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
    let e = zipzi_project::model3::export_settings(s.document()).unwrap_err();
    assert!(e.message.contains(&id(101)));
    patch(
        &mut s,
        101,
        F::ModelParameterMember {
            target: ModelTargetRef::Resolved { object_id: id(10) },
        },
    );
    let e = zipzi_project::model3::export_settings(s.document()).unwrap_err();
    assert!(e.message.contains(&id(300)));
    patch(
        &mut s,
        300,
        F::ModelHitArea {
            name: "Head".into(),
            mesh: ModelTargetRef::Resolved { object_id: id(20) },
        },
    );
    let fields = zipzi_project::model3::export_settings(s.document()).unwrap();
    assert_eq!(fields["Groups"][0]["Ids"], serde_json::json!(["Param10"]));
    let text = serde_json::to_string(&fields).unwrap();
    for n in [100, 101, 300] {
        assert!(!text.contains(&id(n)));
    }
    let imported =
        zipzi_project::model3::import_settings(s.document(), &serde_json::Value::Object(fields))
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
    let a = s.reference_field_address(&id(102)).unwrap();
    assert!(s
        .plan_reference_field_patch(
            &a,
            F::ModelParameterMember {
                target: ModelTargetRef::Unresolved {
                    runtime_id: "Param10".into()
                }
            }
        )
        .is_err());
    let h = s
        .handle(ObjectKind::ModelParameterMember, &id(101))
        .unwrap();
    let settings = s.document().model3_settings().clone();
    s.edit("Replace identities", None, |e| {
        let mut empty = settings.clone();
        empty.groups = Some(vec![]);
        e.set_model3_settings(empty)?;
        e.set_model3_settings(settings)
    })
    .unwrap();
    assert!(s.resolve_handle(&h).is_err());
    assert!(s.document().get_model_parameter_member(&id(101)).is_some());
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
    let row = s
        .handle(ObjectKind::ModelParameterMember, &id(101))
        .unwrap();
    let a = s.reference_field_address(&id(101)).unwrap();
    let parameter = s.document().get_parameter(&id(10)).unwrap().clone();
    s.edit("Replace target incarnation", None, |e| {
        let pending = F::ModelParameterMember {
            target: ModelTargetRef::Unspecified,
        };
        e.edit_references(E::Patch {
            id: id(101),
            expected: configured.clone(),
            fields: pending.clone(),
        })?;
        e.erase_object(&id(10))?;
        e.create_parameter(parameter)?;
        e.edit_references(E::Patch {
            id: id(101),
            expected: pending,
            fields: configured,
        })
    })
    .unwrap();
    s.resolve_handle(&row).unwrap();
    assert!(s
        .plan_reference_field_patch(&a, a.expected().clone())
        .is_err());
    let fresh = s.reference_field_address(&id(101)).unwrap();
    assert!(s
        .plan_reference_field_patch(&fresh, fresh.expected().clone())
        .is_ok());
}
#[test]
fn offscreen_masks_and_blend_constraints_share_reference_validation_and_history() {
    let mut s = session();
    mesh(&mut s, 20, true);
    mesh(&mut s, 21, true);
    s.edit("Reference owners", None, |e| {
        e.create_part(Part {
            id: id(30),
            ..Default::default()
        })?;
        e.create_offscreen(Offscreen {
            id: id(31),
            part_id: id(30),
            ..Default::default()
        })?;
        e.create_parameter(Parameter {
            id: id(10),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_parameter(Parameter {
            id: id(11),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(40),
            parameter_id: id(10),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: Default::default(),
            base_sample_id: Default::default(),
        })?;
        e.create_blend_constraint(BlendShapeConstraint {
            id: id(41),
            parameter_id: id(11),
            keys: vec![0., 1.],
            weights: vec![1., 1.],
            samples: Default::default(),
        })?;
        e.create_blend_binding(BlendShapeBinding {
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
        })
    })
    .unwrap();
    for (owner, field, targets) in [
        (31, S::OffscreenMasks, vec![id(21)]),
        (42, S::BlendConstraints, vec![id(41)]),
    ] {
        let a = s.reference_selection_address(&id(owner), field).unwrap();
        assert!(s.plan_reference_selection_patch(&a, vec![id(999)]).is_err());
        assert!(s
            .plan_reference_selection_patch(&a, vec![targets[0].clone(); 2])
            .is_err());
        let p = s
            .plan_reference_selection_patch(&a, targets.clone())
            .unwrap();
        assert!(!p.effects().metadata_only);
        assert_eq!(p.effects().drawing_changed, field == S::OffscreenMasks);
        assert_eq!(
            p.effects().deformation_changed,
            field == S::BlendConstraints
        );
        let r = s.execute_reference_plan(&p).unwrap();
        assert_eq!(r.changes.reference_selections[0].after, targets);
        s.undo().unwrap();
        assert!(s
            .plan_reference_selection_patch(&a, targets.clone())
            .is_err());
        s.redo().unwrap();
    }
    assert_eq!(s.document().get_offscreen(&id(31)).unwrap().masks, [id(21)]);
    assert_eq!(
        s.document()
            .get_blend_binding(&id(42))
            .unwrap()
            .constraint_ids,
        [id(41)]
    );
    lock(&mut s, 30);
    let a = s
        .reference_selection_address(&id(31), S::OffscreenMasks)
        .unwrap();
    assert!(s.plan_reference_selection_patch(&a, vec![]).is_err());
    lock(&mut s, 20);
    let a = s
        .reference_selection_address(&id(42), S::BlendConstraints)
        .unwrap();
    assert!(s.plan_reference_selection_patch(&a, vec![]).is_err());
}
