use zipzi_core::document::{ExpressionDisabledReason, ExpressionTarget};
use zipzi_core::{Canvas, Parameter, Vec2};
use zipzi_sdk::{
    AuthoringSession, ChangeCategory, CollectionRef, ExpressionEdit as E, ExpressionEntryFields,
    ObjectKind,
};
const DOC: &str = "00000000-0000-4000-8000-00000000e001";
const PARAM: &str = "00000000-0000-4000-8000-00000000e002";
const A: &str = "00000000-0000-4000-8000-00000000e003";
const B: &str = "00000000-0000-4000-8000-00000000e004";
const X: &str = "00000000-0000-4000-8000-00000000e005";
const Y: &str = "00000000-0000-4000-8000-00000000e006";
const Z: &str = "00000000-0000-4000-8000-00000000e007";
fn session() -> AuthoringSession {
    let mut s = AuthoringSession::new(DOC, Canvas::new(100., 100., Vec2::default(), 10.)).unwrap();
    s.edit("fixture", None, |e| {
        e.create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamX".into(),
            name: "X".into(),
            ..Default::default()
        })?;
        for (id, name) in [(A, "A"), (B, "B")] {
            e.edit_expression(E::Create {
                id: id.into(),
                name: name.into(),
            })?;
        }
        for id in [X, Y] {
            e.edit_expression(E::CreateEntry {
                owner: A.into(),
                id: id.into(),
                before: None,
            })?;
        }
        Ok(())
    })
    .unwrap();
    s.drain_events();
    s
}
fn apply(s: &mut AuthoringSession, edit: E) -> zipzi_sdk::EditReceipt {
    let version = s.version();
    let history = s.history_lengths();
    let plan = s.plan_expression_edit(edit).unwrap();
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    s.execute_expression_plan(&plan).unwrap()
}
fn fields(value: f32) -> ExpressionEntryFields {
    ExpressionEntryFields {
        target: ExpressionTarget::Resolved {
            parameter_id: PARAM.into(),
        },
        value,
        blend: None,
    }
}
fn patch(s: &mut AuthoringSession, id: &str, value: f32) -> zipzi_sdk::EditReceipt {
    let address = s.expression_entry_address(id).unwrap();
    let plan = s
        .plan_expression_entry_patch(&address, fields(value))
        .unwrap();
    s.execute_expression_plan(&plan).unwrap()
}
#[test]
fn unconfigured_entries_project_to_no_slots_then_repair_by_stable_address() {
    let mut s = session();
    let p = s.document().expression_projection(A).unwrap();
    assert!(p.entries.is_empty());
    assert_eq!(p.native_to_runtime[X], None);
    assert_eq!(
        p.disabled[0].reason,
        ExpressionDisabledReason::NeedsConfiguration
    );
    assert_eq!(
        s.export_expression3(A).unwrap_err().code.as_ref(),
        "UNCONFIGURED_PARAMETER"
    );
    let r = patch(&mut s, Y, 0.5);
    assert!(r.changes.categories.contains(&ChangeCategory::References));
    assert!(r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    let p = s.document().expression_projection(A).unwrap();
    assert_eq!(p.native_to_runtime[Y], Some(0));
    assert_eq!(p.runtime_to_native, vec![Y]);
    assert_eq!(p.entries[0].id, Y);
    assert_eq!(s.document().references_to(PARAM), vec![Y]);
}
#[test]
fn moving_preserves_identity_order_and_invalidates_old_owner_context() {
    let mut s = session();
    patch(&mut s, X, 0.25);
    patch(&mut s, Y, 0.75);
    let handle = s.handle(ObjectKind::ExpressionEntry, X).unwrap();
    let address = s.expression_entry_address(X).unwrap();
    let r = apply(
        &mut s,
        E::MoveEntries {
            ids: vec![Y.into(), X.into()],
            destination: B.into(),
            before: None,
        },
    );
    assert!(s.resolve_handle(&handle).is_ok());
    assert_eq!(
        s.expression(B)
            .unwrap()
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec![X, Y]
    );
    assert!(s.expression(A).unwrap().entries.is_empty());
    assert_eq!(r.changes.collections.len(), 2);
    assert!(s
        .plan_expression_entry_patch(&address, fields(0.9))
        .is_err());
    patch(&mut s, X, 0.9);
    assert_eq!(s.expression(B).unwrap().entries[0].value, 0.9);
    s.undo().unwrap();
    s.undo().unwrap();
    assert!(s.resolve_handle(&handle).is_ok());
    assert_eq!(s.document().get_expression_entry(X).unwrap().0.id, A);
}
#[test]
fn duplicate_parameter_targets_keep_first_match_semantics() {
    let mut s = session();
    patch(&mut s, X, 0.25);
    patch(&mut s, Y, 0.75);
    let mut preview = zipzi_sdk::ExpressionPreview::new(s.document());
    preview.schedule_expression(A, 0.).unwrap();
    let frame = preview.seek(2.).unwrap();
    assert_eq!(frame.parameters[PARAM], 0.25);
    apply(
        &mut s,
        E::MoveEntries {
            ids: vec![Y.into()],
            destination: A.into(),
            before: Some(X.into()),
        },
    );
    let mut preview = zipzi_sdk::ExpressionPreview::new(s.document());
    preview.schedule_expression(A, 0.).unwrap();
    let frame = preview.seek(2.).unwrap();
    assert_eq!(frame.parameters[PARAM], 0.75);
}
#[test]
fn delete_undo_never_revives_handles_and_history_changes_have_actual_direction() {
    let mut s = session();
    let handle = s.handle(ObjectKind::ExpressionEntry, X).unwrap();
    let r = apply(
        &mut s,
        E::DeleteEntries {
            ids: vec![X.into()],
        },
    );
    assert_eq!(r.changes.deleted[0].id, X);
    assert!(s.resolve_handle(&handle).is_err());
    let u = s.undo().unwrap();
    assert_eq!(u.changes.created[0].id, X);
    assert!(!u.object_ids.is_empty());
    assert!(s.resolve_handle(&handle).is_err());
    let restored = s.handle(ObjectKind::ExpressionEntry, X).unwrap();
    assert_ne!(restored, handle);
    let redo = s.redo().unwrap();
    assert_eq!(redo.changes.deleted[0].id, X);
    assert!(s.resolve_handle(&restored).is_err());
}
#[test]
fn copies_get_fresh_ids_and_keep_pending_fields_and_extensions() {
    let mut s = session();
    let r = apply(
        &mut s,
        E::DuplicateEntries {
            ids: vec![X.into()],
            new_ids: vec![Z.into()],
            destination: B.into(),
            before: None,
        },
    );
    assert_eq!(r.changes.created[0].id, Z);
    let e = &s.expression(B).unwrap().entries[0];
    assert_eq!(e.target, ExpressionTarget::Unspecified);
    assert!(s
        .plan_expression_edit(E::DuplicateEntries {
            ids: vec![X.into()],
            new_ids: vec![X.into()],
            destination: B.into(),
            before: None
        })
        .is_err());
    assert!(s
        .plan_expression_edit(E::CreateEntry {
            owner: B.into(),
            id: PARAM.into(),
            before: None
        })
        .is_err());
}
#[test]
fn invalid_batches_stale_plans_and_noops_do_not_change_history_or_events() {
    let mut s = session();
    let plan = s
        .plan_expression_edit(E::DeleteEntries {
            ids: vec![X.into()],
        })
        .unwrap();
    patch(&mut s, Y, 0.5);
    s.undo().unwrap();
    s.drain_events();
    let v = s.version();
    let h = s.history_lengths();
    assert!(s.execute_expression_plan(&plan).is_err());
    assert!(s
        .plan_expression_edit(E::MoveEntries {
            ids: vec![X.into(), "missing".into()],
            destination: B.into(),
            before: None
        })
        .is_err());
    let r = apply(
        &mut s,
        E::MoveEntries {
            ids: vec![Y.into()],
            destination: A.into(),
            before: None,
        },
    );
    assert!(!r.changed);
    assert!(r.changes.created.is_empty());
    assert_eq!(s.version(), v);
    assert_eq!(s.history_lengths(), h);
    assert!(s.drain_events().is_empty());
}
#[test]
fn candidate_invalid_preview_cancel_roundtrip_and_commit_obey_single_history() {
    let mut s = session();
    let address = s.expression_entry_address(X).unwrap();
    let v = s.version();
    let h = s.history_lengths();
    let mut c = s.begin_expression_entry_edit(&address).unwrap();
    c.preview(fields(0.2)).unwrap();
    assert!(c.preview(fields(f32::NAN)).is_err());
    assert_eq!(c.projection().unwrap().entries[0].value, 0.2);
    c.cancel();
    assert_eq!(s.version(), v);
    assert_eq!(s.history_lengths(), h);
    assert!(s.drain_events().is_empty());
    let mut c = s.begin_expression_entry_edit(&address).unwrap();
    c.preview(fields(0.3)).unwrap();
    c.preview(address.expected().clone()).unwrap();
    assert!(!c.commit(&mut s).unwrap().changed);
    assert_eq!(s.history_lengths(), h);
    let mut c = s.begin_expression_entry_edit(&address).unwrap();
    c.preview(fields(0.1)).unwrap();
    c.preview(fields(0.8)).unwrap();
    let r = c.commit(&mut s).unwrap();
    assert!(r.changed);
    assert_eq!(s.history_lengths().0, h.0 + 1);
    assert_eq!(s.drain_events(), vec![r]);
}
#[test]
fn pending_and_unresolved_entries_roundtrip_with_ids_order_and_no_external_ids() {
    let mut s = session();
    let a = s.expression_entry_address(Y).unwrap();
    let plan = s
        .plan_expression_entry_patch(
            &a,
            ExpressionEntryFields {
                target: ExpressionTarget::Unresolved {
                    runtime_id: "Missing".into(),
                },
                value: 0.7,
                blend: None,
            },
        )
        .unwrap();
    s.execute_expression_plan(&plan).unwrap();
    let root =
        std::env::temp_dir().join(format!("expression-entry-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let project = root.join("project");
    s.save_project(&project, None).unwrap();
    let expected = s.expression(A).unwrap();
    let mut reopened = session();
    reopened.open_project(&project, None).unwrap();
    assert_eq!(reopened.expression(A).unwrap(), expected);
    patch(&mut reopened, X, 0.25);
    patch(&mut reopened, Y, 0.5);
    let text = reopened.export_expression3(A).unwrap();
    assert!(!text.contains(X));
    assert!(!text.contains(Y));
    assert!(!text.contains("parameter_id"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).unwrap()["Parameters"][0]["Id"],
        "ParamX"
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn rename_and_library_order_preserve_runtime_caches_and_report_only_labels_or_order() {
    let mut s = session();
    let revision = s.evaluation_revision();
    let mut a = s.expression(A).unwrap();
    a.name = "Renamed".into();
    let (_, r) = s.edit("rename", None, |e| e.replace_expression(a)).unwrap();
    assert_eq!(s.evaluation_revision(), revision);
    assert_eq!(
        r.changes.categories.into_iter().collect::<Vec<_>>(),
        vec![ChangeCategory::Labels]
    );
    assert!(r
        .changes
        .modified
        .iter()
        .all(|e| e.kind == ObjectKind::Expression));
    let r = apply(
        &mut s,
        E::Reorder {
            ids: vec![B.into(), A.into()],
        },
    );
    assert_eq!(s.evaluation_revision(), revision);
    assert_eq!(
        r.changes.collections,
        vec![CollectionRef::Expressions {
            document_id: DOC.into()
        }]
    );
    let r = s.undo().unwrap();
    assert!(!r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert_eq!(s.evaluation_revision(), revision);
}
#[test]
fn sdk_alternative_writes_cannot_bypass_locks_or_leave_orphan_flags() {
    let mut s = session();
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        X.into(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    s.edit("lock", None, |e| e.replace_editor_state(state))
        .unwrap();
    assert!(s
        .plan_expression_edit(E::DeleteEntries {
            ids: vec![X.into()]
        })
        .is_err());
    assert!(s.edit("bypass", None, |e| e.erase_object(A)).is_err());
    let mut a = s.expression(A).unwrap();
    a.entries[0].value = 0.7;
    assert!(s
        .edit("bypass replace", None, |e| e.replace_expression(a))
        .is_err());
}

#[test]
fn available_export_lists_loss_and_rejects_a_stale_acceptance() {
    let mut s = session();
    patch(&mut s, Y, 0.4);
    let plan = s.plan_available_expression_export(A).unwrap();
    assert_eq!(plan.omitted().len(), 1);
    assert_eq!(plan.omitted()[0].entry_id, X);
    let text = s.export_expression_plan(&plan).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(wire["Parameters"].as_array().unwrap().len(), 1);
    patch(&mut s, X, 0.2);
    assert!(s.export_expression_plan(&plan).is_err());
    assert!(s
        .plan_available_expression_export(A)
        .unwrap()
        .omitted()
        .is_empty());
}
#[test]
fn removing_then_recreating_identical_content_in_one_transaction_changes_incarnation() {
    let mut s = session();
    let handle = s.handle(ObjectKind::ExpressionEntry, Y).unwrap();
    let expected = s.expression(A).unwrap();
    s.edit("replace incarnation", None, |e| {
        e.edit_expression(E::DeleteEntries {
            ids: vec![Y.into()],
        })?;
        e.replace_expression(expected)
    })
    .unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    s.undo().unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    s.redo().unwrap();
    assert!(s.resolve_handle(&handle).is_err());
}
#[test]
fn unresolved_entries_do_not_block_other_preview_entries() {
    let mut s = session();
    patch(&mut s, X, 0.45);
    let address = s.expression_entry_address(Y).unwrap();
    let plan = s
        .plan_expression_entry_patch(
            &address,
            ExpressionEntryFields {
                target: ExpressionTarget::Unresolved {
                    runtime_id: "Missing".into(),
                },
                value: 0.9,
                blend: None,
            },
        )
        .unwrap();
    s.execute_expression_plan(&plan).unwrap();
    let mut p = zipzi_sdk::ExpressionPreview::new(s.document());
    p.schedule_expression(A, 0.).unwrap();
    assert_eq!(p.seek(2.).unwrap().parameters[PARAM], 0.45);
    let p = s.document().expression_projection(A).unwrap();
    assert!(
        matches!(&p.disabled[0].reason,ExpressionDisabledReason::UnresolvedParameter {runtime_id} if runtime_id=="Missing")
    );
    assert_eq!(
        s.export_expression3(A).unwrap_err().code.as_ref(),
        "UNRESOLVED_PARAMETER"
    );
}
#[test]
fn whole_expression_copy_and_delete_include_owned_entries_in_one_history_step() {
    let mut s = session();
    let h = s.history_lengths().0;
    let copy = "00000000-0000-4000-8000-00000000e008";
    let child = "00000000-0000-4000-8000-00000000e009";
    let r = apply(
        &mut s,
        E::Duplicate {
            source: A.into(),
            id: copy.into(),
            name: "Copy".into(),
            entry_ids: vec![Z.into(), child.into()],
        },
    );
    assert_eq!(r.changes.created.len(), 3);
    assert_eq!(s.history_lengths().0, h + 1);
    let a = s.expression(copy).unwrap();
    assert_eq!(a.entries[0].target, ExpressionTarget::Unspecified);
    let handle = s.handle(ObjectKind::ExpressionEntry, Z).unwrap();
    let r = apply(&mut s, E::Delete { id: copy.into() });
    assert_eq!(r.changes.deleted.len(), 3);
    assert!(s.resolve_handle(&handle).is_err());
    s.undo().unwrap();
    assert!(s.resolve_handle(&handle).is_err());
}

#[test]
fn delayed_candidate_cannot_write_after_delete_undo_or_into_another_session() {
    let mut s = session();
    let address = s.expression_entry_address(X).unwrap();
    let mut candidate = s.begin_expression_entry_edit(&address).unwrap();
    candidate.preview(fields(0.9)).unwrap();
    apply(
        &mut s,
        E::DeleteEntries {
            ids: vec![X.into()],
        },
    );
    s.undo().unwrap();
    let version = s.version();
    let history = s.history_lengths();
    assert!(candidate.commit(&mut s).is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_lengths(), history);
    assert_eq!(s.document().get_expression_entry(X).unwrap().1.value, 0.);
    let other = session();
    let plan = other
        .plan_expression_edit(E::DeleteEntries {
            ids: vec![X.into()],
        })
        .unwrap();
    assert!(s.execute_expression_plan(&plan).is_err());
    assert!(other
        .plan_expression_entry_patch(&address, fields(0.1))
        .is_err());
}

#[test]
fn expression_copy_remaps_imported_source_scene_owned_identities() {
    use zipzi_core::document::{MotionClip, MotionKey, MotionTrack, MotionTrackTarget};
    let mut s = session();
    let mut expression = s.expression(A).unwrap();
    let track_id = "00000000-0000-4000-8000-00000000e010";
    let key_id = "00000000-0000-4000-8000-00000000e011";
    expression.preserve_source_scene(&MotionClip {
        track_order: Vec::new(),
        id: A.into(),
        name: "A source".into(),
        duration: 1.,
        fps: 30.,
        looping: false,
        restricted_beziers: true,
        fade_in: None,
        fade_out: None,
        tracks: vec![MotionTrack {
            id: track_id.into(),
            target: MotionTrackTarget::Parameter {
                parameter_id: PARAM.into(),
            },
            initial: MotionKey {
                id: key_id.into(),
                time: 0.,
                value: 0.4,
            },
            segments: Default::default(),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        }]
        .into(),
        events: Default::default(),
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    });
    let (_, receipt) = s
        .edit("source scene", None, |edit| {
            edit.replace_expression(expression)
        })
        .unwrap();
    assert!(receipt
        .changes
        .categories
        .contains(&ChangeCategory::Content));
    let id = "00000000-0000-4000-8000-00000000e012";
    let child = "00000000-0000-4000-8000-00000000e013";
    apply(
        &mut s,
        E::Duplicate {
            source: A.into(),
            id: id.into(),
            name: "Copied scene".into(),
            entry_ids: vec![Z.into(), child.into()],
        },
    );
    let copy = s.expression(id).unwrap().source_scene().unwrap();
    assert_eq!(copy.id, id);
    assert_ne!(copy.tracks[0].id, track_id);
    assert_ne!(copy.tracks[0].initial.id, key_id);
    assert_eq!(copy.tracks[0].initial.value, 0.4);
    assert!(s.validate_structure().is_empty());
}

#[test]
fn lock_change_must_identify_affected_entry() {
    let mut s = session();
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        X.into(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    let (_, receipt) = s
        .edit("lock", None, |e| e.replace_editor_state(state))
        .unwrap();
    assert!(
        receipt.changes.modified.iter().any(|e| e.id == X),
        "lock state changes must invalidate entry capability consumers"
    );
}
#[test]
fn unrelated_entry_edit_must_not_invalidate_field_address() {
    let mut s = session();
    let address = s.expression_entry_address(X).unwrap();
    patch(&mut s, Y, 0.5);
    let result = s.plan_expression_entry_patch(&address, fields(0.2));
    assert!(result.is_ok(), "unrelated Y edit invalidated X: {result:?}");
}
#[test]
fn direct_core_plan_cannot_overwrite_diverged_snapshot_at_same_revision() {
    let s = session();
    let mut one = s.document().fork_candidate();
    let mut two = s.document().fork_candidate();
    let p = one
        .plan_expression_edit(E::PatchEntry {
            id: X.into(),
            expected: one.get_expression_entry(X).unwrap().1.into(),
            fields: fields(0.8),
        })
        .unwrap();
    assert!(one.apply_expression_edit(&p).status.is_ok());
    let p = two
        .plan_expression_edit(E::PatchEntry {
            id: Y.into(),
            expected: two.get_expression_entry(Y).unwrap().1.into(),
            fields: fields(0.4),
        })
        .unwrap();
    assert!(two.apply_expression_edit(&p).status.is_ok());
    assert_eq!(one.revision(), two.revision());
    let p = one
        .plan_expression_edit(E::CreateEntry {
            owner: A.into(),
            id: Z.into(),
            before: None,
        })
        .unwrap();
    let result = two.apply_expression_edit(&p);
    assert!(
        !result.status.is_ok(),
        "foreign candidate plan overwrote Y={} with X={}",
        two.get_expression_entry(Y).unwrap().1.value,
        two.get_expression_entry(X).unwrap().1.value
    );
}

#[test]
fn lock_plan_from_fork_must_not_bypass_current_lock() {
    let s = session();
    let mut one = s.document().fork_candidate();
    let mut two = s.document().fork_candidate();
    let mut a = one.get_expression(A).unwrap().clone();
    a.name = "Renamed".into();
    assert!(one.replace_expression(a).status.is_ok());
    let mut state = two.editor_state().clone();
    state.objects.insert(
        X.into(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    assert!(two.replace_editor_state(state).status.is_ok());
    assert_eq!(one.revision(), two.revision());
    let plan = one
        .plan_expression_edit(E::PatchEntry {
            id: X.into(),
            expected: one.get_expression_entry(X).unwrap().1.into(),
            fields: fields(0.7),
        })
        .unwrap();
    let result = two.apply_expression_edit(&plan);
    assert!(!result.status.is_ok(), "foreign plan wrote through lock");
}

#[test]
fn field_addresses_survive_unrelated_edits_but_not_owner_roundtrips_or_field_conflicts() {
    let mut s = session();
    let address = s.expression_entry_address(X).unwrap();
    patch(&mut s, Y, 0.5);
    let mut candidate = s.begin_expression_entry_edit(&address).unwrap();
    candidate.preview(fields(0.2)).unwrap();
    candidate.commit(&mut s).unwrap();
    assert_eq!(s.document().get_expression_entry(X).unwrap().1.value, 0.2);
    assert!(s
        .plan_expression_entry_patch(&address, fields(0.3))
        .is_err());

    let address = s.expression_entry_address(X).unwrap();
    apply(
        &mut s,
        E::MoveEntries {
            ids: vec![X.into()],
            destination: B.into(),
            before: None,
        },
    );
    s.undo().unwrap();
    assert_eq!(s.document().get_expression_entry(X).unwrap().0.id, A);
    assert!(s
        .plan_expression_entry_patch(&address, fields(0.3))
        .is_err());

    let address = s.expression_entry_address(X).unwrap();
    let plan = s
        .plan_expression_entry_patch(&address, fields(0.3))
        .unwrap();
    patch(&mut s, Y, 0.7);
    assert!(s.execute_expression_plan(&plan).is_err());
    assert!(s.plan_expression_entry_patch(&address, fields(0.3)).is_ok());
}

#[test]
fn lock_deltas_report_dependants_and_history_without_evaluation_invalidation() {
    let mut s = session();
    let evaluation = s.document().evaluation_revision();
    for id in [X, A] {
        let mut state = s.document().editor_state().clone();
        state.objects.insert(
            id.into(),
            zipzi_core::document::ObjectEditorState {
                locked: true,
                hidden_in_editor: false,
            },
        );
        let (_, receipt) = s
            .edit("lock", None, |edit| edit.replace_editor_state(state))
            .unwrap();
        assert!(receipt.changed);
        assert_eq!(
            receipt.changes.categories,
            [ChangeCategory::EditorState].into()
        );
        assert!(receipt.changes.modified.iter().any(|e| e.id == X));
        assert!(receipt.changes.modified.iter().any(|e| e.id == A));
        if id == A {
            assert!(receipt.changes.modified.iter().any(|e| e.id == Y));
        }
        assert_eq!(s.document().evaluation_revision(), evaluation);
        let undo = s.undo().unwrap();
        assert_eq!(undo.changes, receipt.changes);
        assert!(!s.document().editor_state().get(id).locked);
        assert_eq!(s.redo().unwrap().changes, receipt.changes);
        assert!(s.document().editor_state().get(id).locked);
        s.undo().unwrap();
    }
}

#[test]
fn core_plan_accepts_an_unchanged_fork_but_never_revives_after_restore() {
    let s = session();
    let original = s.document();
    let plan = original
        .plan_expression_edit(E::CreateEntry {
            owner: A.into(),
            id: Z.into(),
            before: None,
        })
        .unwrap();
    let mut candidate = original.fork_candidate();
    assert!(candidate.apply_expression_edit(&plan).status.is_ok());
    assert!(original.get_expression_entry(Z).is_none());
    assert!(candidate.get_expression_entry(Z).is_some());
    candidate.restore_from(original);
    let unchanged = candidate.clone();
    assert!(!candidate.apply_expression_edit(&plan).status.is_ok());
    assert!(candidate.same_content(&unchanged));
}

#[test]
fn core_plans_detect_changes_during_batch_build_without_revision_advances() {
    let mut document = zipzi_core::Document::new();
    assert!(document.begin_batch_build().is_ok());
    assert!(document
        .initialize(DOC, Canvas::new(100., 100., Vec2::default(), 10.))
        .is_ok());
    let plan = document
        .plan_expression_edit(E::Create {
            id: A.into(),
            name: "A".into(),
        })
        .unwrap();
    let other = document
        .plan_expression_edit(E::Create {
            id: B.into(),
            name: "B".into(),
        })
        .unwrap();
    let revision = document.revision();
    assert!(document.apply_expression_edit(&other).status.is_ok());
    assert_eq!(document.revision(), revision);
    assert!(!document.apply_expression_edit(&plan).status.is_ok());
    assert!(document.get_expression(B).is_some());
    assert!(document.get_expression(A).is_none());
}
