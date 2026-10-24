use super::support;
use zipzi_core::ParameterKind;
use zipzi_editor_core::{
    Document, EditorCommand, NewParameter, ObjectKind, ObjectRef, ParameterGroupDraft,
};
fn group(name: &str, parent: Option<String>) -> ParameterGroupDraft {
    ParameterGroupDraft {
        name: name.into(),
        runtime_id: name.into(),
        parent_id: parent,
    }
}
fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}

#[test]
fn edit_is_atomic_preserves_identity_and_persists_group_order() -> anyhow::Result<()> {
    let (mut doc, assets) = support::sample_document_with_parameter_groups()?;
    let p = id(1);
    let old = doc.parameter_draft(&p)?;
    let mut next = old.clone();
    next.runtime_id = "ParamRenamed".into();
    next.name = "眼睛".into();
    next.group_id = Some(id(102));
    next.minimum = -2.;
    next.maximum = 2.;
    next.default_value = 0.375;
    doc.set_preview_parameter(&p, 0.7)?;
    let revision = doc.parameter_model_revision();
    let history = doc.history_lengths();
    doc.update_parameter(&p, next.clone(), revision)?;
    assert_eq!(doc.parameter_draft(&p)?, next);
    assert_eq!(doc.history_lengths().0, history.0 + 1);
    assert_eq!(
        doc.parameters()
            .iter()
            .filter(|p| p.group_id == Some(id(102)))
            .map(|p| p.id.clone())
            .collect::<Vec<_>>(),
        vec![id(3), id(4), p.clone()]
    );
    assert_eq!(
        doc.parameters().iter().find(|x| x.id == p).unwrap().value,
        0.7
    );
    let unchanged = doc.history_lengths();
    doc.update_parameter(&p, next.clone(), doc.parameter_model_revision())?;
    assert_eq!(doc.history_lengths(), unchanged);
    assert!(doc.update_parameter(&p, old.clone(), revision).is_err());
    doc.undo()?;
    assert_eq!(doc.parameter_draft(&p)?, old);
    doc.redo()?;
    assert_eq!(doc.parameter_draft(&p)?, next);
    let receipt = doc.save(&assets.path().join("edited.zipzi"))?;
    let reopened = Document::open(&receipt.manifest)?;
    assert_eq!(reopened.parameter_draft(&p)?, next);
    Ok(())
}

#[test]
fn dependency_failures_preserve_model_history_and_preview() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_binding(true)?;
    let p = doc.parameters()[0].id.clone();
    let old = doc.parameter_draft(&p)?;
    let history = doc.history_lengths();
    let mut invalid = old.clone();
    invalid.maximum = 0.5;
    assert!(
        doc.update_parameter(&p, invalid, doc.parameter_model_revision())
            .is_err()
    );
    let mut invalid = old.clone();
    invalid.kind = ParameterKind::BlendShape;
    assert!(
        doc.update_parameter(&p, invalid, doc.parameter_model_revision())
            .is_err()
    );
    assert!(!doc.parameter_delete_references(&p)?.is_empty());
    assert!(
        doc.delete_parameter(&p, doc.parameter_model_revision())
            .is_err()
    );
    assert_eq!(doc.parameter_draft(&p)?, old);
    assert_eq!(doc.history_lengths(), history);
    Ok(())
}

#[test]
fn deletion_cleans_owned_display_entry_and_restores_uuid_on_undo() -> anyhow::Result<()> {
    let mut doc = Document::new_blank()?;
    let p = doc.create_parameter(NewParameter::default())?;
    doc.set_preview_parameter(&p, 0.8)?;
    assert!(doc.parameter_delete_references(&p)?.is_empty());
    doc.delete_parameter(&p, doc.parameter_model_revision())?;
    assert!(doc.parameters().is_empty());
    assert!(doc.context().parameter_focus.is_none());
    assert_eq!(doc.history_lengths().0, 2);
    doc.undo()?;
    assert_eq!(doc.parameters()[0].id, p);
    assert_eq!(doc.parameters()[0].value, 0.);
    doc.redo()?;
    assert!(doc.parameters().is_empty());
    Ok(())
}

#[test]
fn groups_reject_cycles_and_dissolve_only_one_level() -> anyhow::Result<()> {
    let (mut doc, assets) = support::sample_document_with_parameter_groups()?;
    let pupil = doc.create_parameter_group(
        group("Pupil", Some(id(102))),
        doc.parameter_model_revision(),
    )?;
    let p = doc.create_parameter(NewParameter {
        group_id: Some(pupil.clone()),
        ..Default::default()
    })?;
    let before = doc.history_lengths();
    assert!(
        doc.update_parameter_group(
            &id(101),
            group("Face", Some(pupil.clone())),
            doc.parameter_model_revision()
        )
        .is_err()
    );
    assert!(
        doc.update_parameter_group(
            &id(101),
            group("Face", Some(id(101))),
            doc.parameter_model_revision()
        )
        .is_err()
    );
    assert_eq!(doc.history_lengths(), before);
    assert_eq!(doc.parameter_group_delete_counts(&id(102))?, (2, 0, 1));
    doc.delete_parameter_group(&id(102), doc.parameter_model_revision())?;
    assert_eq!(doc.parameter_group_draft(&pupil)?.parent_id, Some(id(101)));
    assert_eq!(doc.parameter_draft(&id(3))?.group_id, Some(id(101)));
    assert_eq!(doc.parameter_draft(&p)?.group_id, Some(pupil.clone()));
    assert_eq!(doc.history_lengths().0, before.0 + 1);
    doc.undo()?;
    assert_eq!(doc.parameter_group_draft(&pupil)?.parent_id, Some(id(102)));
    doc.redo()?;
    doc.delete_parameter_group(&id(101), doc.parameter_model_revision())?;
    assert_eq!(doc.parameter_draft(&id(3))?.group_id, None);
    assert_eq!(doc.parameter_group_draft(&pupil)?.parent_id, None);
    let saved = doc.save(&assets.path().join("groups.zipzi"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(reopened.parameter_groups(), doc.parameter_groups());
    assert_eq!(reopened.parameters(), doc.parameters());
    Ok(())
}

#[test]
fn preview_is_clamped_on_edit_and_redo_and_quick_edit_uses_shared_validation() -> anyhow::Result<()>
{
    let mut doc = Document::new_blank()?;
    let p = doc.create_parameter(NewParameter::default())?;
    doc.set_preview_parameter(&p, 0.8)?;
    let mut draft = doc.parameter_draft(&p)?;
    draft.maximum = 0.5;
    doc.update_parameter(&p, draft, doc.parameter_model_revision())?;
    assert_eq!(doc.parameters()[0].value, 0.5);
    doc.undo()?;
    doc.set_preview_parameter(&p, 0.9)?;
    doc.redo()?;
    assert_eq!(doc.parameters()[0].value, 0.5);
    assert!(
        doc.set_object_property(
            &ObjectRef::new(ObjectKind::Parameter, &p),
            "name",
            serde_json::json!("   ")
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn all_management_commands_reject_stale_document() -> anyhow::Result<()> {
    let mut doc = Document::new_blank()?;
    let p = doc.create_parameter(NewParameter::default())?;
    let g = doc.create_parameter_group(group("Face", None), doc.parameter_model_revision())?;
    let draft = doc.parameter_draft(&p)?;
    let version = doc.parameter_model_revision();
    let mut editor = crate::support::editor_from_document(doc);
    let old = editor.active_id().unwrap();
    editor.new_document()?;
    for command in [
        EditorCommand::UpdateParameter {
            id: p.clone(),
            draft: draft.clone(),
            expected_version: version,
        },
        EditorCommand::DeleteParameter {
            id: p,
            expected_version: version,
        },
        EditorCommand::CreateParameterChecked {
            draft,
            expected_version: version,
        },
        EditorCommand::CreateParameterGroup {
            draft: group("Other", None),
            expected_version: version,
        },
        EditorCommand::UpdateParameterGroup {
            id: g.clone(),
            draft: group("Changed", None),
            expected_version: version,
        },
        EditorCommand::DeleteParameterGroup {
            id: g,
            expected_version: version,
        },
    ] {
        assert!(editor.execute_for(old, command).is_err());
    }
    assert!(editor.document().unwrap().parameters().is_empty());
    Ok(())
}

#[test]
fn imported_unresolved_members_and_extensions_survive_group_dissolution_and_export_preflight()
-> anyhow::Result<()> {
    use zipzi_core::{Canvas, Parameter, Vec2, document::CdiParameterEntry};
    use zipzi_sdk::AuthoringSession;
    let assets = tempfile::tempdir()?;
    let p = id(800);
    let mut session =
        AuthoringSession::new(&id(801), Canvas::new(256., 256., Vec2::default(), 1.))?;
    session.edit("Fixture", None, |edit| {
        edit.create_parameter(Parameter { id: p.clone(), runtime_id: "ParamX".into(), ..Default::default() })?;
        edit.import_cdi3(r#"{"Version":3,"Custom":{"keep":true},"ParameterGroups":[{"Id":"Face","GroupId":"","Name":"Face"},{"Id":"Eyes","GroupId":"Face","Name":"Eyes"}],"Parameters":[{"Id":"ParamX","GroupId":"Eyes","Name":"X","CustomEntry":7},{"Id":"Missing","GroupId":"Eyes","Name":"?","UnresolvedNote":"keep"}]}"#)?;
        Ok(())
    })?;
    let info = session.document().display_info().clone();
    let face = info.parameter_groups.as_ref().unwrap()[0].id.clone();
    let eyes = info.parameter_groups.as_ref().unwrap()[1].id.clone();
    let saved = session.save_project(&assets.path().join("imported.zipzi"), None)?;
    let mut doc = Document::open(&saved.manifest)?;
    let mut draft = doc.parameter_draft(&p)?;
    draft.runtime_id = "Changed".into();
    let history = doc.history_lengths();
    let warnings = doc.parameter_export_diagnostics(&EditorCommand::UpdateParameter {
        id: p.clone(),
        draft,
        expected_version: doc.parameter_model_revision(),
    });
    assert!(
        warnings
            .iter()
            .any(|w| w.introduced && w.code == "OPAQUE_NAMESPACE_CHANGED")
    );
    assert_eq!(doc.history_lengths(), history);
    assert_eq!(doc.parameter_draft(&p)?.runtime_id, "ParamX");
    assert_eq!(doc.parameter_group_delete_counts(&eyes)?, (1, 1, 0));
    doc.delete_parameter_group(&eyes, doc.parameter_model_revision())?;
    let saved = doc.save(&saved.manifest)?;
    session.open_project(&saved.manifest, None)?;
    let after = session.document().display_info();
    assert_eq!(after.extensions, info.extensions);
    assert_eq!(after.opaque_source_ids, info.opaque_source_ids);
    for (before, after) in info
        .parameters
        .as_ref()
        .unwrap()
        .iter()
        .zip(after.parameters.as_ref().unwrap())
    {
        assert_eq!(before.extensions(), after.extensions());
        assert_eq!(after.group_id(), Some(face.as_str()));
    }
    assert!(
        matches!(&after.parameters.as_ref().unwrap()[1], CdiParameterEntry::Unresolved { runtime_id, .. } if runtime_id == "Missing")
    );
    doc.undo()?;
    assert_eq!(doc.parameter_group_delete_counts(&eyes)?, (1, 1, 0));
    Ok(())
}

#[test]
fn core_still_supports_nested_group_creation_and_reparenting() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document_with_parameter_groups()?;
    let nested = doc.create_parameter_group(
        group("Nested", Some(id(101))),
        doc.parameter_model_revision(),
    )?;
    assert_eq!(doc.parameter_group_draft(&nested)?.parent_id, Some(id(101)));
    doc.update_parameter_group(
        &nested,
        group("Nested", Some(id(102))),
        doc.parameter_model_revision(),
    )?;
    assert_eq!(doc.parameter_group_draft(&nested)?.parent_id, Some(id(102)));
    doc.undo()?;
    assert_eq!(doc.parameter_group_draft(&nested)?.parent_id, Some(id(101)));
    Ok(())
}

#[test]
fn promoting_a_legacy_group_to_root_preserves_members_and_supports_history_and_save()
-> anyhow::Result<()> {
    let (mut doc, assets) = support::sample_document_with_parameter_groups()?;
    let before = doc.parameter_groups();
    let members = doc.parameters();
    doc.reorder_parameter_group(
        &id(102),
        &id(101),
        zipzi_editor_core::ReorderPosition::Before,
    )?;
    assert_eq!(doc.parameter_group_draft(&id(102))?.parent_id, None);
    assert_eq!(doc.parameter_groups()[0].id, id(102));
    assert_eq!(doc.parameters(), members);
    assert_eq!(doc.history_lengths().0, 1);
    doc.undo()?;
    assert_eq!(doc.parameter_groups(), before);
    doc.redo()?;
    assert_eq!(doc.parameter_group_draft(&id(102))?.parent_id, None);
    let receipt = doc.save(&assets.path().join("promoted.zipzi"))?;
    assert_eq!(
        Document::open(&receipt.manifest)?.parameter_groups(),
        doc.parameter_groups()
    );
    Ok(())
}
