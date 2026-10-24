use zipzi_core::document::ExpressionTarget;
use zipzi_editor_core::{Document, ObjectKind, ObjectRef};
use zipzi_sdk::{ChangeCategory, ExpressionEdit as E, ExpressionEntryFields};
const A: &str = "00000000-0000-4000-8000-00000000a101";
const B: &str = "00000000-0000-4000-8000-00000000a102";
const X: &str = "00000000-0000-4000-8000-00000000a103";
fn apply(d: &mut Document, e: E) -> anyhow::Result<zipzi_editor_core::OperationResult> {
    let p = d.plan_expression_authoring(e)?;
    d.execute_expression_authoring(&p)
}
#[test]
fn editor_adapts_sdk_results_and_projects_pending_entries_without_changing_selection()
-> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let r = apply(
        &mut d,
        E::Create {
            id: A.into(),
            name: "A".into(),
        },
    )?;
    assert_eq!(
        r.suggested_selection,
        vec![ObjectRef::new(ObjectKind::Expression, A)]
    );
    let r = apply(
        &mut d,
        E::CreateEntry {
            owner: A.into(),
            id: X.into(),
            before: None,
        },
    )?;
    assert_eq!(
        r.suggested_selection,
        vec![ObjectRef::new(ObjectKind::ExpressionEntry, X)]
    );
    let index = d.object_index();
    let object = index
        .get(&ObjectRef::new(ObjectKind::ExpressionEntry, X))
        .unwrap();
    assert!(object.diagnostic.is_some());
    assert_eq!(d.expression_resources()[0].disabled_entries.len(), 1);
    let address = d.expression_entry_address(X)?;
    apply(
        &mut d,
        E::Create {
            id: B.into(),
            name: "B".into(),
        },
    )?;
    let r = apply(
        &mut d,
        E::MoveEntries {
            ids: vec![X.into()],
            destination: B.into(),
            before: None,
        },
    )?;
    assert!(
        r.receipt
            .changes
            .categories
            .contains(&ChangeCategory::Order)
    );
    assert!(
        d.plan_expression_entry_patch(
            &address,
            ExpressionEntryFields {
                target: ExpressionTarget::Unspecified,
                value: 0.5,
                blend: None
            }
        )
        .is_err()
    );
    let address = d.expression_entry_address(X)?;
    assert_eq!(address.owner_id(), B);
    let candidate = d.begin_expression_entry_edit(&address)?;
    d.preview_expression_entry_edit(
        candidate,
        ExpressionEntryFields {
            target: ExpressionTarget::Unspecified,
            value: 0.5,
            blend: None,
        },
    )?;
    let r = d.commit_expression_entry_edit(candidate)?;
    assert!(r.receipt.changed);
    d.undo()?;
    assert_eq!(d.expression_definition(B).unwrap().entries[0].value, 0.);
    d.undo()?;
    assert_eq!(d.expression_definition(A).unwrap().entries[0].id, X);
    d.redo()?;
    assert_eq!(d.expression_definition(B).unwrap().entries[0].id, X);
    Ok(())
}

fn entry_document() -> anyhow::Result<Document> {
    let mut document = Document::new_blank()?;
    apply(
        &mut document,
        E::Create {
            id: A.into(),
            name: "A".into(),
        },
    )?;
    apply(
        &mut document,
        E::CreateEntry {
            owner: A.into(),
            id: X.into(),
            before: None,
        },
    )?;
    apply(
        &mut document,
        E::Create {
            id: B.into(),
            name: "Unrelated".into(),
        },
    )?;
    document.select_object(ObjectRef::new(ObjectKind::ExpressionEntry, X))?;
    Ok(document)
}
fn entry_fields(value: f32) -> ExpressionEntryFields {
    ExpressionEntryFields {
        target: ExpressionTarget::Unspecified,
        value,
        blend: None,
    }
}
fn entry_candidate(
    document: &mut Document,
    value: f32,
) -> anyhow::Result<zipzi_editor_core::ExpressionEntryEditToken> {
    let address = document.expression_entry_address(X)?;
    let token = document.begin_expression_entry_edit(&address)?;
    document.preview_expression_entry_edit(token, entry_fields(value))?;
    Ok(token)
}

#[test]
fn cancel_retires_candidate_and_preserves_redo_and_published_state() -> anyhow::Result<()> {
    let mut document = entry_document()?;
    document.undo()?;
    let history = document.history_lengths();
    let token = entry_candidate(&mut document, 0.8)?;
    assert!(document.has_pending_edit());
    assert_eq!(
        document.expression_definition(A).unwrap().entries[0].value,
        0.8
    );
    assert_eq!(
        document
            .expression_authoring_capabilities(&ObjectRef::new(ObjectKind::ExpressionEntry, X))?
            .session_reason,
        Some(zipzi_editor_core::AuthoringSessionReason::CandidateActive)
    );
    document.cancel_edit();
    assert!(!document.has_pending_edit());
    assert_eq!(
        document.expression_definition(A).unwrap().entries[0].value,
        0.0
    );
    assert_eq!(document.history_lengths(), history);
    assert!(document.commit_expression_entry_edit(token).is_err());
    assert!(
        document
            .preview_expression_entry_edit(token, entry_fields(0.4))
            .is_err()
    );
    document.redo()?;
    assert!(document.expression_definition(B).is_some());
    Ok(())
}

#[test]
fn context_changes_cancel_expression_gestures_and_tokens_cannot_cross_documents()
-> anyhow::Result<()> {
    let mut document = entry_document()?;
    let history = document.history_lengths();
    let token = entry_candidate(&mut document, 0.8)?;
    document.select_object(ObjectRef::new(ObjectKind::Expression, B))?;
    assert!(document.commit_expression_entry_edit(token).is_err());
    assert_eq!(
        document.expression_definition(A).unwrap().entries[0].value,
        0.0
    );
    assert_eq!(document.history_lengths(), history);
    let token = entry_candidate(&mut document, 0.8)?;
    let mut other = entry_document()?;
    let other_token = entry_candidate(&mut other, 0.4)?;
    assert!(other.commit_expression_entry_edit(token).is_err());
    assert!(other.has_pending_edit());
    other.commit_expression_entry_edit(other_token)?;
    assert_eq!(
        other.expression_definition(A).unwrap().entries[0].value,
        0.4
    );
    document.cancel_edit();
    Ok(())
}

#[test]
fn recoverable_preview_noop_and_finish_share_the_editor_lifecycle() -> anyhow::Result<()> {
    let mut document = entry_document()?;
    document.undo()?;
    let history = document.history_lengths();
    let token = entry_candidate(&mut document, 0.8)?;
    assert!(
        document
            .preview_expression_entry_edit(token, entry_fields(f32::NAN))
            .is_err()
    );
    assert_eq!(
        document.expression_definition(A).unwrap().entries[0].value,
        0.8
    );
    document.preview_expression_entry_edit(token, entry_fields(0.0))?;
    let receipt = document.commit_expression_entry_edit(token)?;
    assert!(!receipt.receipt.changed);
    assert_eq!(document.history_lengths(), history);
    assert!(!document.has_pending_edit());
    let token = entry_candidate(&mut document, 0.6)?;
    document.finish_edit()?;
    assert!(!document.has_pending_edit());
    assert!(document.commit_expression_entry_edit(token).is_err());
    assert_eq!(document.history_lengths(), (history.0 + 1, 0));
    document.undo()?;
    assert_eq!(
        document.expression_definition(A).unwrap().entries[0].value,
        0.0
    );
    document.redo()?;
    assert_eq!(
        document.expression_definition(A).unwrap().entries[0].value,
        0.6
    );
    Ok(())
}

#[test]
fn scalar_preview_commit_cancel_and_history_share_unrelated_index_rows() -> anyhow::Result<()> {
    let mut document = entry_document()?;
    let unrelated = ObjectRef::new(ObjectKind::Expression, B);
    let entry = ObjectRef::new(ObjectKind::ExpressionEntry, X);
    let before = document.object_index();
    let token = entry_candidate(&mut document, 0.8)?;
    for expected in [0.8, 0.8, 0.0, 0.8] {
        let after = document.object_index();
        assert!(std::ptr::eq(
            before.get(&unrelated).unwrap(),
            after.get(&unrelated).unwrap()
        ));
        assert_eq!(
            after.get(&entry).unwrap().properties["value"],
            serde_json::json!(expected as f32)
        );
        if document.has_pending_edit() {
            document.commit_expression_entry_edit(token)?;
        } else if expected == 0.0 {
            document.redo()?;
        } else {
            document.undo()?;
        }
    }
    let token = entry_candidate(&mut document, 0.4)?;
    document.cancel_edit();
    assert!(document.commit_expression_entry_edit(token).is_err());
    let after = document.object_index();
    assert!(std::ptr::eq(
        before.get(&unrelated).unwrap(),
        after.get(&unrelated).unwrap()
    ));
    assert_eq!(
        before.get(&entry).unwrap().properties["value"],
        serde_json::json!(0.0)
    );
    Ok(())
}

#[test]
fn workspace_lifecycle_cancels_expression_candidates_except_for_save() -> anyhow::Result<()> {
    use zipzi_editor_core::{EditingLifecycle, Editor};
    for reason in [
        EditingLifecycle::ContextChange,
        EditingLifecycle::BlockingUi,
        EditingLifecycle::WindowDeactivated,
        EditingLifecycle::Close,
        EditingLifecycle::Save,
    ] {
        let mut document = entry_document()?;
        let history = document.history_lengths();
        entry_candidate(&mut document, 0.8)?;
        let mut editor = Editor::from_document(document);
        assert!(editor.editing_active());
        editor.end_editing(editor.active_id().unwrap(), reason)?;
        let document = editor.document().unwrap();
        assert!(!document.has_pending_edit());
        let saved = reason == EditingLifecycle::Save;
        assert_eq!(
            document.expression_definition(A).unwrap().entries[0].value,
            if saved { 0.8 } else { 0.0 }
        );
        assert_eq!(
            document.history_lengths(),
            (history.0 + usize::from(saved), history.1)
        );
    }
    Ok(())
}

#[test]
fn save_finishes_expression_candidate_and_roundtrips_once() -> anyhow::Result<()> {
    let mut document = entry_document()?;
    let history = document.history_lengths();
    let token = entry_candidate(&mut document, 0.8)?;
    let dir = tempfile::tempdir()?;
    let saved = document.save(&dir.path().join("expression.zipzi"))?;
    assert!(!document.has_pending_edit());
    assert!(!document.modified());
    assert!(document.commit_expression_entry_edit(token).is_err());
    assert_eq!(document.history_lengths(), (history.0 + 1, 0));
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(
        reopened.expression_definition(A).unwrap().entries[0].value,
        0.8
    );
    Ok(())
}

#[test]
fn scalar_refresh_updates_relation_labels_and_reference_cancel_restores_diagnostics()
-> anyhow::Result<()> {
    use zipzi_core::document::ExpressionBlend;
    use zipzi_editor_core::RelationKind;
    let mut document = entry_document()?;
    let owner = ObjectRef::new(ObjectKind::Expression, A);
    let entry = ObjectRef::new(ObjectKind::ExpressionEntry, X);
    let unrelated = ObjectRef::new(ObjectKind::Expression, B);
    let fields = ExpressionEntryFields {
        target: ExpressionTarget::Unresolved {
            runtime_id: "Missing".into(),
        },
        value: 0.0,
        blend: None,
    };
    let address = document.expression_entry_address(X)?;
    let plan = document.plan_expression_entry_patch(&address, fields.clone())?;
    document.execute_expression_authoring(&plan)?;
    let before = document.object_index();
    let address = document.expression_entry_address(X)?;
    let token = document.begin_expression_entry_edit(&address)?;
    document.preview_expression_entry_edit(
        token,
        ExpressionEntryFields {
            value: 0.75,
            blend: Some(ExpressionBlend::Multiply),
            ..fields.clone()
        },
    )?;
    let preview = document.object_index();
    assert_eq!(
        preview
            .outgoing(&owner)
            .find(|edge| edge.kind == RelationKind::ExpressionWrites)
            .unwrap()
            .detail,
        "0.75 · Multiply"
    );
    assert!(std::ptr::eq(
        before.get(&unrelated).unwrap(),
        preview.get(&unrelated).unwrap()
    ));
    document.commit_expression_entry_edit(token)?;
    document.undo()?;
    let restored = document.object_index();
    assert_eq!(
        restored
            .outgoing(&owner)
            .find(|edge| edge.kind == RelationKind::ExpressionWrites)
            .unwrap()
            .detail,
        "0 · Add"
    );
    assert!(std::ptr::eq(
        before.get(&unrelated).unwrap(),
        restored.get(&unrelated).unwrap()
    ));
    let token = entry_candidate(&mut document, 0.5)?;
    assert_eq!(
        document
            .object_index()
            .get(&entry)
            .unwrap()
            .diagnostic
            .as_deref(),
        Some("Parameter needs configuration")
    );
    let preview_catalog = document.animation_catalog();
    document.cancel_edit();
    assert!(document.commit_expression_entry_edit(token).is_err());
    assert_eq!(
        document
            .object_index()
            .get(&entry)
            .unwrap()
            .diagnostic
            .as_deref(),
        Some("Unresolved parameter: Missing")
    );
    assert!(!std::sync::Arc::ptr_eq(
        &preview_catalog,
        &document.animation_catalog()
    ));
    assert_eq!(
        document.expression_definition(A).unwrap().entries[0].target,
        fields.target
    );
    Ok(())
}
