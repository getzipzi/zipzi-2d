//! Ported expression-owned behavior, using Draft sources instead of handles or
//! field-address/plan registries. Runtime playback/export cases remain separate.
use zipzi_core::{
    document::{ExpressionAsset, ExpressionEntry, ExpressionTarget, ObjectEditorState},
    state::{DocumentState, OwnedDocument, RecordDomain, RootDomain},
    Parameter,
};
use zipzi_sdk::{
    AuthoringSession, Commands, ExpressionEdit as E, ExpressionEntryFields, ProjectIo, Publication,
    ResourceStore, ResourceView, Snapshot, Status,
};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn asset(n: u32, name: &str, entries: Vec<ExpressionEntry>) -> ExpressionAsset {
    ExpressionAsset {
        id: id(n),
        name: name.into(),
        file_type: Some("Live2D Expression".into()),
        fade_in: None,
        fade_out: None,
        entries,
        extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    }
}
fn entry(n: u32) -> ExpressionEntry {
    ExpressionEntry {
        id: id(n),
        target: ExpressionTarget::Unspecified,
        value: 0.,
        blend: None,
        extensions: [("custom".into(), serde_json::json!({"source": n}))].into(),
    }
}
fn session() -> AuthoringSession {
    let mut input = OwnedDocument {
        id: id(1),
        parameters: vec![Parameter {
            id: id(2),
            runtime_id: "ParamX".into(),
            ..Default::default()
        }],
        expressions: vec![
            asset(3, "A", vec![entry(5), entry(6)]),
            asset(4, "B", vec![]),
        ],
        ..Default::default()
    };
    input.editor_state.objects.insert(
        id(5),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    AuthoringSession::new(
        Snapshot::new(
            DocumentState::from_owned(input).unwrap(),
            ResourceView::default(),
        )
        .unwrap(),
    )
}
fn edit(
    s: &mut AuthoringSession,
    f: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>,
) -> Publication {
    let mut draft = s.begin_draft();
    draft.batch(draft.source(), f).unwrap();
    s.publish(draft.prepare("expressions").unwrap()).unwrap()
}
fn apply(s: &mut AuthoringSession, op: E) {
    edit(s, |candidate_commands| {
        candidate_commands.edit_expression(op)
    });
}
fn fields(value: f32) -> ExpressionEntryFields {
    ExpressionEntryFields {
        target: ExpressionTarget::Resolved {
            parameter_id: id(2),
        },
        value,
        blend: None,
    }
}
fn patch(s: &mut AuthoringSession, n: u32, value: f32) {
    let expected =
        ExpressionEntryFields::from(s.snapshot().document().expression_entry(&id(n)).unwrap().1);
    apply(
        s,
        E::PatchEntry {
            id: id(n),
            expected,
            fields: fields(value),
        },
    );
}

#[test]
fn expression_creation_and_multiple_entry_edits_publish_one_step() {
    let mut s = session();
    let original = s.snapshot().clone();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.edit_expression(E::Create {
                id: id(7),
                name: "Created".into(),
            })?;
            c.edit_expression(E::CreateEntry {
                owner: id(7),
                id: id(8),
                before: None,
            })?;
            c.edit_expression(E::PatchEntry {
                id: id(8),
                expected: ExpressionEntryFields::from(&entry(8)),
                fields: fields(0.5),
            })
        })
        .unwrap();
    let prepared = draft.prepare("create configured entry").unwrap();
    assert_eq!(prepared.writes().records().len(), 1);
    assert_eq!(
        prepared.writes().records()[0].domain,
        RecordDomain::Expression
    );
    s.publish(prepared).unwrap();
    assert_eq!(
        s.snapshot().document().expression(&id(7)).unwrap().entries[0].value,
        0.5
    );
    assert_eq!(s.snapshot().document().references_to(&id(2)), [id(8)]);
    assert!(original.document().expression(&id(7)).is_none());
    assert_eq!(s.take_events().len(), 1);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), original.content_id());
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .expression_entry(&id(8))
            .unwrap()
            .0
            .id,
        id(7)
    );
}

#[test]
fn moving_uses_document_order_preserves_metadata_and_undo_restores_owners() {
    let mut s = session();
    patch(&mut s, 5, 0.25);
    patch(&mut s, 6, 0.75);
    let before = s.snapshot().clone();
    apply(
        &mut s,
        E::MoveEntries {
            ids: vec![id(6), id(5)],
            destination: id(4),
            before: None,
        },
    );
    let document = s.snapshot().document();
    assert_eq!(
        document
            .expression(&id(4))
            .unwrap()
            .entries
            .iter()
            .map(|e| e.id.clone())
            .collect::<Vec<_>>(),
        [id(5), id(6)]
    );
    assert!(document.expression(&id(3)).unwrap().entries.is_empty());
    assert!(document.editor_state().get(&id(5)).hidden_in_editor);
    assert_eq!(
        document.expression_entry(&id(5)).unwrap().1.extensions,
        entry(5).extensions
    );
    assert_eq!(document.references_to(&id(2)), [id(5), id(6)]);
    patch(&mut s, 5, 0.9);
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(
        s.snapshot()
            .document()
            .expression_entry(&id(5))
            .unwrap()
            .0
            .id,
        id(3)
    );
}

#[test]
fn copying_entries_and_whole_assets_remaps_identity_and_preserves_fields() {
    let mut s = session();
    apply(
        &mut s,
        E::DuplicateEntries {
            ids: vec![id(6), id(5)],
            new_ids: vec![id(7), id(8)],
            destination: id(4),
            before: None,
        },
    );
    let copied = &s.snapshot().document().expression(&id(4)).unwrap().entries;
    assert_eq!(copied[0].id, id(7));
    assert_eq!(copied[0].extensions, entry(5).extensions);
    assert_eq!(copied[1].extensions, entry(6).extensions);
    assert_eq!(copied[0].target, ExpressionTarget::Unspecified);
    apply(
        &mut s,
        E::Duplicate {
            source: id(3),
            id: id(9),
            name: "Copied".into(),
            entry_ids: vec![id(10), id(11)],
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .expression_entry(&id(10))
            .unwrap()
            .0
            .id,
        id(9)
    );
    assert_eq!(
        s.snapshot()
            .document()
            .expression_entry(&id(11))
            .unwrap()
            .1
            .extensions,
        entry(6).extensions
    );
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .get(&id(7))
        .is_none());
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().expression(&id(9)).is_none());
}

#[test]
fn deletion_and_full_replacement_remove_only_orphan_editor_flags() {
    for mode in 0..3 {
        let mut s = session();
        let original = s.snapshot().clone();
        let mut draft = s.begin_draft();
        let mut replacement = original.document().expression(&id(3)).unwrap().clone();
        replacement.entries.remove(0);
        draft
            .batch(draft.source(), |c| match mode {
                0 => c.edit_expression(E::DeleteEntries { ids: vec![id(5)] }),
                1 => c.edit_expression(E::Delete { id: id(3) }),
                _ => c.replace_expression(replacement),
            })
            .unwrap();
        let prepared = draft.prepare("delete").unwrap();
        assert!(prepared.writes().root_changed(RootDomain::EditorState));
        s.publish(prepared).unwrap();
        assert!(s.snapshot().document().expression_entry(&id(5)).is_none());
        assert!(s
            .snapshot()
            .document()
            .editor_state()
            .objects
            .get(&id(5))
            .is_none());
        s.undo(s.version()).unwrap();
        assert_eq!(s.snapshot().content_id(), original.content_id());
        assert!(
            s.snapshot()
                .document()
                .editor_state()
                .get(&id(5))
                .hidden_in_editor
        );
    }
}

#[test]
fn invalid_or_ignored_expression_commands_abort_all_previous_changes() {
    let operations = [
        E::MoveEntries {
            ids: vec![id(5), id(99)],
            destination: id(4),
            before: None,
        },
        E::MoveEntries {
            ids: vec![id(5)],
            destination: id(3),
            before: Some(id(5)),
        },
        E::DuplicateEntries {
            ids: vec![id(5), id(6)],
            new_ids: vec![id(7), id(7)],
            destination: id(4),
            before: None,
        },
        E::CreateEntry {
            owner: id(4),
            id: id(2),
            before: None,
        },
        E::Create {
            id: id(9),
            name: "A".into(),
        },
        E::Reorder {
            ids: vec![id(3), id(3)],
        },
        E::PatchEntry {
            id: id(5),
            expected: fields(123.),
            fields: fields(0.5),
        },
        E::PatchEntry {
            id: id(5),
            expected: ExpressionEntryFields::from(&entry(5)),
            fields: fields(f32::NAN),
        },
    ];
    for op in operations {
        let mut s = session();
        let before = s.snapshot().content_id();
        let mut draft = s.begin_draft();
        assert!(draft
            .batch(draft.source(), |c| {
                c.edit_expression(E::Create {
                    id: id(20),
                    name: "Never published".into(),
                })?;
                assert!(c.edit_expression(op).is_err());
                Ok(())
            })
            .is_err());
        assert!(!draft.is_active());
        assert!(draft.prepare("failed").is_err());
        assert_eq!(s.snapshot().content_id(), before);
        assert!(s.take_events().is_empty());
    }
}

#[test]
fn owner_and_entry_locks_cover_copy_move_delete_patch_and_whole_replacement() {
    for locked in [3, 5] {
        let base = session();
        let mut b = base.snapshot().document().builder();
        let mut state = b.editor_state().clone();
        state.objects.entry(id(locked)).or_default().locked = true;
        b.set_editor_state(state).unwrap();
        let snapshot = Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap();
        for op in [
            E::MoveEntries {
                ids: vec![id(5)],
                destination: id(4),
                before: None,
            },
            E::DuplicateEntries {
                ids: vec![id(5)],
                new_ids: vec![id(7)],
                destination: id(4),
                before: None,
            },
            E::Duplicate {
                source: id(3),
                id: id(7),
                name: "Copy".into(),
                entry_ids: vec![id(8), id(9)],
            },
            E::Delete { id: id(3) },
            E::DeleteEntries { ids: vec![id(5)] },
            E::PatchEntry {
                id: id(5),
                expected: ExpressionEntryFields::from(&entry(5)),
                fields: fields(0.5),
            },
        ] {
            let s = AuthoringSession::new(snapshot.clone());
            let mut draft = s.begin_draft();
            assert_eq!(
                draft
                    .batch(draft.source(), |candidate_commands| candidate_commands
                        .edit_expression(op))
                    .unwrap_err()
                    .code,
                "OBJECT_LOCKED"
            );
        }
        let mut replacement = snapshot.document().expression(&id(3)).unwrap().clone();
        replacement.entries[0].value = 0.8;
        let s = AuthoringSession::new(snapshot);
        let mut draft = s.begin_draft();
        assert_eq!(
            draft
                .batch(draft.source(), |c| c.replace_expression(replacement))
                .unwrap_err()
                .code,
            "OBJECT_LOCKED"
        );
    }
}

#[test]
fn noops_preserve_redo_and_stale_sources_never_revive_after_undo() {
    let mut s = session();
    let mut stale = s.begin_draft();
    stale
        .batch(stale.source(), |c| {
            c.edit_expression(E::DeleteEntries { ids: vec![id(5)] })
        })
        .unwrap();
    let stale = stale.prepare("stale delete").unwrap();
    patch(&mut s, 5, 0.5);
    s.undo(s.version()).unwrap();
    s.take_events();
    let version = s.version();
    let history = s.history_state();
    apply(
        &mut s,
        E::MoveEntries {
            ids: vec![id(6)],
            destination: id(3),
            before: None,
        },
    );
    apply(
        &mut s,
        E::Reorder {
            ids: vec![id(3), id(4)],
        },
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s.take_events().is_empty());
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .expression_entry(&id(5))
            .unwrap()
            .1
            .value,
        0.5
    );
}

#[test]
fn library_reorder_tracks_order_and_shares_unchanged_records() {
    let mut s = session();
    let initial = s.snapshot().clone();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.edit_expression(E::Reorder {
                ids: vec![id(4), id(3)],
            })
        })
        .unwrap();
    let prepared = draft.prepare("reorder").unwrap();
    assert!(prepared.writes().order_changed(RecordDomain::Expression));
    assert!(prepared.writes().records().is_empty());
    s.publish(prepared).unwrap();
    assert_eq!(s.snapshot().document().expression_order(), [id(4), id(3)]);
    assert!(std::ptr::eq(
        initial.document().expression(&id(3)).unwrap(),
        s.snapshot().document().expression(&id(3)).unwrap()
    ));
}

#[test]
fn pending_and_unresolved_entries_preserve_order_extensions_and_ids_on_save() {
    let mut s = session();
    let expected = ExpressionEntryFields::from(&entry(6));
    apply(
        &mut s,
        E::PatchEntry {
            id: id(6),
            expected,
            fields: ExpressionEntryFields {
                target: ExpressionTarget::Unresolved {
                    runtime_id: "Missing".into(),
                },
                value: 0.7,
                blend: None,
            },
        },
    );
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("expressions.zipzi");
    s.save_as(&io, &path, None, || true).unwrap();
    let (mut loaded, _) = AuthoringSession::open(&io, &path, || true).unwrap();
    assert_eq!(
        loaded.snapshot().document().expression(&id(3)),
        s.snapshot().document().expression(&id(3))
    );
    assert_eq!(
        loaded.snapshot().document().editor_state(),
        s.snapshot().document().editor_state()
    );
    patch(&mut loaded, 6, 0.9);
    assert_eq!(loaded.snapshot().document().references_to(&id(2)), [id(6)]);
    assert!(loaded.is_dirty());
    loaded.undo(loaded.version()).unwrap();
    assert!(!loaded.is_dirty());
}

#[test]
fn expression_copy_remaps_imported_source_scene_owned_identities() {
    use zipzi_core::document::{MotionClip, MotionKey, MotionTrack, MotionTrackTarget};
    let mut s = session();
    let mut expression = s.snapshot().document().expression(&id(3)).unwrap().clone();
    let track_id = "00000000-0000-4000-8000-00000000e010";
    let key_id = "00000000-0000-4000-8000-00000000e011";
    expression.preserve_source_scene(&MotionClip {
        track_order: Vec::new(),
        id: id(3),
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
                parameter_id: id(2),
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
    edit(&mut s, |c| c.replace_expression(expression));
    let copied_id = "00000000-0000-4000-8000-00000000e012";
    let child = "00000000-0000-4000-8000-00000000e013";
    apply(
        &mut s,
        E::Duplicate {
            source: id(3),
            id: copied_id.into(),
            name: "Copied scene".into(),
            entry_ids: vec![id(7), child.into()],
        },
    );
    let copy = s
        .snapshot()
        .document()
        .expression(copied_id)
        .unwrap()
        .source_scene()
        .unwrap();
    assert_eq!(copy.id, copied_id);
    assert_ne!(copy.tracks[0].id, track_id);
    assert_ne!(copy.tracks[0].initial.id, key_id);
    assert_eq!(copy.tracks[0].initial.value, 0.4);
    assert!(s
        .snapshot()
        .document()
        .expression_projection(copied_id)
        .is_ok());
}

#[test]
fn projection_omits_disabled_entries_and_keeps_duplicate_target_order() {
    use zipzi_core::state::ExpressionDisabledReason;
    let mut s = session();
    let initial = s.snapshot().clone();
    let p = initial.document().expression_projection(&id(3)).unwrap();
    assert!(p.entries.is_empty());
    assert_eq!(p.native_to_runtime[&id(5)], None);
    assert_eq!(
        p.disabled[0].reason,
        ExpressionDisabledReason::NeedsConfiguration
    );
    patch(&mut s, 5, 0.25);
    patch(&mut s, 6, 0.75);
    let p = s
        .snapshot()
        .document()
        .expression_projection(&id(3))
        .unwrap();
    assert_eq!(p.runtime_to_native, [id(5), id(6)]);
    assert_eq!(p.entries[0].value, 0.25);
    apply(
        &mut s,
        E::MoveEntries {
            ids: vec![id(6)],
            destination: id(3),
            before: Some(id(5)),
        },
    );
    let p = s
        .snapshot()
        .document()
        .expression_projection(&id(3))
        .unwrap();
    assert_eq!(p.runtime_to_native, [id(6), id(5)]);
    assert_eq!(p.entries[0].value, 0.75);
    assert_eq!(p.native_to_runtime[&id(5)], Some(1));
    let expected = fields(0.75);
    apply(
        &mut s,
        E::PatchEntry {
            id: id(6),
            expected,
            fields: ExpressionEntryFields {
                target: ExpressionTarget::Unresolved {
                    runtime_id: "Missing".into(),
                },
                value: 1.,
                blend: None,
            },
        },
    );
    let p = s
        .snapshot()
        .document()
        .expression_projection(&id(3))
        .unwrap();
    assert_eq!(p.runtime_to_native, [id(5)]);
    assert_eq!(
        p.disabled[0].reason,
        ExpressionDisabledReason::UnresolvedParameter {
            runtime_id: "Missing".into()
        }
    );
    assert!(initial
        .document()
        .expression_projection(&id(3))
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn full_record_writes_validate_at_batch_boundary_and_share_other_owners() {
    let mut s = session();
    let before = s.snapshot().clone();
    edit(&mut s, |c| {
        c.create_expression(asset(7, "Full record", vec![entry(8)]))
    });
    let mut replacement = s.snapshot().document().expression(&id(7)).unwrap().clone();
    replacement.fade_in = Some(0.4);
    replacement.name = "Renamed".into();
    edit(&mut s, |c| c.replace_expression(replacement));
    let current = s.snapshot().clone();
    assert_eq!(
        current.document().expression(&id(7)).unwrap().fade_in,
        Some(0.4)
    );
    assert!(std::ptr::eq(
        before.document().expression(&id(3)).unwrap(),
        current.document().expression(&id(3)).unwrap()
    ));
    let mut invalid = current.document().expression(&id(7)).unwrap().clone();
    invalid.fade_in = Some(-1.);
    let mut draft = s.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| c.replace_expression(invalid))
            .unwrap_err()
            .code,
        "INVALID_EXPRESSION_FADE"
    );
    assert!(!draft.is_active());
    assert_eq!(s.snapshot().content_id(), current.content_id());
}

#[test]
fn available_export_uses_the_captured_projection_and_stale_candidates_cannot_accept_new_content() {
    let mut s = session();
    edit(&mut s, |c| {
        let mut asset = c.read_document()?.expression(&id(3)).unwrap().clone();
        for entry in &mut asset.entries {
            entry.extensions.clear();
        }
        c.replace_expression(asset)
    });
    patch(&mut s, 6, 0.4);
    let captured = s.read_snapshot();
    let projection = captured
        .snapshot
        .document()
        .expression_projection(&id(3))
        .unwrap();
    assert_eq!(projection.disabled.len(), 1);
    assert_eq!(projection.disabled[0].entry_id, id(5));
    let export =
        zipzi_project::export_available_expression3(captured.snapshot.document(), &id(3)).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&export).unwrap();
    assert_eq!(wire["Parameters"].as_array().unwrap().len(), 1);
    let stale = s.begin_draft().prepare("captured acceptance").unwrap();
    patch(&mut s, 5, 0.2);
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
    assert!(s
        .snapshot()
        .document()
        .expression_projection(&id(3))
        .unwrap()
        .disabled
        .is_empty());
    assert_eq!(
        zipzi_project::export_available_expression3(captured.snapshot.document(), &id(3)).unwrap(),
        export
    );
}
