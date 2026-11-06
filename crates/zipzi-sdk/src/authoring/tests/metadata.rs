use super::*;
use zipzi_core::document::{
    AttachmentRef, DocumentAuthoringEdit, DocumentValue, Model3Settings, ModelTargetRef,
    ObjectEditorState, PackageAttachment,
};
fn apply(s: &mut AuthoringSession, f: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>) {
    let mut d = s.begin_draft();
    d.batch(d.source(), f).unwrap();
    s.publish(d.prepare("metadata").unwrap()).unwrap();
}
#[test]
fn attachment_repair_replacement_and_undo_keep_blob_identity() {
    let mut b = snapshot().document().builder();
    b.set_model3_settings(Model3Settings {
        user_data: Some(AttachmentRef::Unresolved {
            path: "data.json".into(),
        }),
        ..Default::default()
    })
    .unwrap();
    let mut s = AuthoringSession::new(
        Snapshot::new(b.freeze().unwrap().state, Default::default()).unwrap(),
    );
    let bytes = Arc::new(b"{}".to_vec());
    apply(&mut s, |c| {
        c.edit_attachment(AttachmentEdit::Create {
            id: id(20),
            path: "data.json".into(),
        })?;
        c.edit_attachment(AttachmentEdit::Replace {
            expected: PackageAttachment {
                id: id(20),
                path: "data.json".into(),
                bytes: None,
            },
            attachment: PackageAttachment {
                id: id(20),
                path: "data.json".into(),
                bytes: Some(bytes.clone()),
            },
        })?;
        c.edit_document(DocumentAuthoringEdit::RepairMissingAttachment {
            path: "data.json".into(),
            attachment_id: Some(id(20)),
        })?;
        c.edit_attachment(AttachmentEdit::Duplicate {
            source: id(20),
            id: id(21),
            path: "backup.json".into(),
        })
    });
    let before = s.snapshot().clone();
    assert!(Arc::ptr_eq(
        before
            .document()
            .attachment(&id(21))
            .unwrap()
            .bytes
            .as_ref()
            .unwrap(),
        &bytes
    ));
    apply(&mut s, |c| {
        c.edit_attachment(AttachmentEdit::DeleteWithReplacement {
            id: id(20),
            replacement: Some(id(21)),
        })
    });
    assert_eq!(
        s.snapshot().document().model3_settings().user_data,
        Some(AttachmentRef::Resolved {
            attachment_id: id(21)
        })
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    let mut d = s.begin_draft();
    assert!(d
        .batch(d.source(), |c| c.edit_attachment(AttachmentEdit::Create {
            id: id(30),
            path: "../bad".into()
        }))
        .is_err());
    assert!(d.prepare("bad").is_err());
}
#[test]
fn reference_fields_honor_owner_locks_and_delete_retires_editor_flags() {
    let mut s = AuthoringSession::new(snapshot());
    apply(&mut s, |c| {
        c.edit_reference(ReferenceEdit::CreateModelParameterGroup {
            id: id(20),
            name: "EyeBlink".into(),
            before: None,
        })?;
        c.edit_reference(ReferenceEdit::CreateMember {
            id: id(21),
            collection: ReferenceCollection::ModelParameterMembers { group_id: id(20) },
            before: None,
        })?;
        c.edit_reference(ReferenceEdit::Patch {
            id: id(21),
            expected: ReferenceFields::ModelParameterMember {
                target: ModelTargetRef::Unspecified,
            },
            fields: ReferenceFields::ModelParameterMember {
                target: ModelTargetRef::Resolved { object_id: id(2) },
            },
        })?;
        Ok(())
    });
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        id(20),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    apply(&mut s, |c| c.replace_editor_state(flags));
    let mut d = s.begin_draft();
    let error = d
        .batch(d.source(), |c| {
            c.edit_reference(ReferenceEdit::Delete { ids: vec![id(21)] })?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.code, "OBJECT_LOCKED");
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        id(20),
        ObjectEditorState {
            locked: false,
            hidden_in_editor: true,
        },
    );
    flags.objects.insert(
        id(21),
        ObjectEditorState {
            locked: false,
            hidden_in_editor: true,
        },
    );
    apply(&mut s, |c| c.replace_editor_state(flags));
    apply(&mut s, |c| {
        c.edit_reference(ReferenceEdit::Delete { ids: vec![id(20)] })?;
        Ok(())
    });
    assert!(!s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .contains_key(&id(20)));
    assert!(!s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .contains_key(&id(21)));
}
#[test]
fn image_duplicate_shares_version_and_property_failure_aborts_previous_writes() {
    let mut s = AuthoringSession::new(snapshot());
    let image = ResourceStore::default()
        .prepare_rgba(1, 1, vec![255; 4].into())
        .unwrap();
    let key = image.key();
    apply(&mut s, |c| {
        c.replace_image(&id(5), image)?;
        c.duplicate_asset(&id(5), id(20), String::new())
    });
    assert_eq!(s.snapshot().resources().len(), 1);
    assert!(
        matches!(s.snapshot().document().asset(&id(20)).unwrap().content,AssetContent::Image{resource,..} if resource==key)
    );
    let before = s.snapshot().content_id();
    let mut d = s.begin_draft();
    assert!(d
        .batch(d.source(), |c| {
            c.rename(NameTarget::Mesh(&id(4)), "pending".into())?;
            c.patch_property(zipzi_core::state::PropertyPatch::Part {
                id: id(3),
                draw_order: f32::NAN,
            })
        })
        .is_err());
    assert_eq!(s.snapshot().content_id(), before);
    assert!(d.prepare("invalid").is_err());
}
#[test]
fn document_patch_precondition_rejects_stale_value_in_same_batch() {
    let mut s = AuthoringSession::new(snapshot());
    let old = s.snapshot().document().setup().export;
    let mut next = old;
    next.padding_pixels += 1.;
    let mut d = s.begin_draft();
    let error = d
        .batch(d.source(), |c| {
            c.edit_document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::ExportSettings(old),
                value: DocumentValue::ExportSettings(next),
            })?;
            c.edit_document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::ExportSettings(old),
                value: DocumentValue::ExportSettings(next),
            })?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.code, "STALE_FIELD");
    assert_eq!(s.snapshot().document().setup().export, old);
    apply(&mut s, |c| {
        c.edit_document(DocumentAuthoringEdit::Patch {
            expected: DocumentValue::ExportSettings(old),
            value: DocumentValue::ExportSettings(next),
        })?;
        Ok(())
    });
    assert_eq!(s.snapshot().document().setup().export, next);
}
