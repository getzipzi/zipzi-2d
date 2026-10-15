use super::support;
use editor_core::{
    Document, Editor, EditorCommand, NewParameter, ObjectKind, ObjectRef,
    ParameterCreateError as Error,
};
use kasane_core::{
    Canvas, Parameter, ParameterKind, Vec2,
    document::{CdiParameterEntry, CdiParameterGroup, DisplayInfo, DisplayInfoOrigin},
};
use kasane_sdk::AuthoringSession;
use std::sync::Arc;
use uuid::Uuid;

fn uuid() -> String {
    Uuid::new_v4().to_string()
}

#[test]
fn new_parameter_is_one_history_entry_with_stable_identity_and_exact_persistence()
-> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let id = document.create_parameter(NewParameter {
        name: "  新参数  ".into(),
        runtime_id: "  ParamExact  ".into(),
        default_value: 0.375,
        kind: ParameterKind::BlendShape,
        repeat: true,
        decimal_places: 4,
        ..Default::default()
    })?;
    assert!(Uuid::parse_str(&id).is_ok());
    assert!(document.modified());
    assert_eq!(document.history_lengths(), (1, 0));
    let parameter = document.parameters().remove(0);
    assert_eq!(parameter.id, id);
    assert_eq!(parameter.name, "新参数");
    assert_eq!(parameter.value, 0.375);
    assert_eq!(parameter.decimal_places, 4);
    assert_eq!(document.context().parameter_focus.as_ref().unwrap().id, id);
    assert_eq!(
        document.parameter_visuals()[&id].kind,
        ParameterKind::BlendShape
    );
    assert!(document.parameter_visuals()[&id].repeat);
    document.undo()?;
    assert!(document.parameters().is_empty());
    assert!(document.context().parameter_focus.is_none());
    assert!(document.context().inspected.is_none());
    document.redo()?;
    assert_eq!(document.parameters()[0], parameter);
    let destination = tempfile::tempdir()?;
    let saved = document.save(&destination.path().join("parameter.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(reopened.parameters()[0], parameter);
    let mut session = AuthoringSession::new(&uuid(), Canvas::new(256., 256., Vec2::default(), 1.))?;
    session.open_project(&saved.manifest, None)?;
    let model = session.document();
    assert_eq!(model.get_parameter(&id).unwrap().runtime_id, "ParamExact");
    assert!(
        matches!(&model.display_info().parameters.as_ref().unwrap()[0], CdiParameterEntry::Resolved { parameter_id, group_id: None, .. } if parameter_id == &id)
    );
    Ok(())
}

#[test]
fn creation_preserves_target_active_binding_and_other_preview_values() -> anyhow::Result<()> {
    let (mut document, _assets) = support::sample_document_with_binding(true)?;
    let old = document.parameters()[0].id.clone();
    let binding = document.object_index().bindings[0].reference.clone();
    document.open_binding(binding.clone())?;
    document.set_preview_parameter(&old, 0.7)?;
    let selection = document.context().selections.clone();
    let id = document.create_parameter(NewParameter::default())?;
    assert_eq!(document.context().selections, selection);
    assert_eq!(document.context().active_binding, Some(binding));
    assert_eq!(
        document.context().inspected,
        Some(ObjectRef::new(ObjectKind::Parameter, &id))
    );
    assert_eq!(document.parameters()[0].value, 0.7);
    assert!(document.object_index().parameter(&id).regular.is_empty());
    Ok(())
}

#[test]
fn invalid_creation_preserves_content_history_redo_and_projection() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    document.create_parameter(NewParameter::default())?;
    document.undo()?;
    let history = document.history_lengths();
    let index = document.object_index();
    let invalid = [
        (
            NewParameter {
                name: "  ".into(),
                ..Default::default()
            },
            Error::NameRequired,
        ),
        (
            NewParameter {
                runtime_id: " ".into(),
                ..Default::default()
            },
            Error::IdRequired,
        ),
        (
            NewParameter {
                minimum: f32::NAN,
                ..Default::default()
            },
            Error::MinimumInvalid,
        ),
        (
            NewParameter {
                maximum: f32::INFINITY,
                ..Default::default()
            },
            Error::MaximumInvalid,
        ),
        (
            NewParameter {
                default_value: f32::NEG_INFINITY,
                ..Default::default()
            },
            Error::DefaultInvalid,
        ),
        (
            NewParameter {
                minimum: 1.,
                ..Default::default()
            },
            Error::RangeInvalid,
        ),
        (
            NewParameter {
                minimum: 2.,
                ..Default::default()
            },
            Error::RangeInvalid,
        ),
        (
            NewParameter {
                minimum: -f32::MAX,
                maximum: f32::MAX,
                ..Default::default()
            },
            Error::RangeOverflow,
        ),
        (
            NewParameter {
                default_value: 2.,
                ..Default::default()
            },
            Error::DefaultOutOfRange,
        ),
        (
            NewParameter {
                decimal_places: 10,
                ..Default::default()
            },
            Error::PrecisionInvalid,
        ),
        (
            NewParameter {
                group_id: Some(uuid()),
                ..Default::default()
            },
            Error::MissingGroup,
        ),
    ];
    for (draft, expected) in invalid {
        assert_eq!(document.validate_new_parameter(&draft), Err(expected));
        assert_eq!(
            document
                .create_parameter(draft)
                .unwrap_err()
                .downcast_ref::<Error>(),
            Some(&expected)
        );
        assert_eq!(document.history_lengths(), history);
        assert!(document.parameters().is_empty());
        assert!(Arc::ptr_eq(&index, &document.object_index()));
    }
    document.redo()?;
    let duplicate = NewParameter::default();
    assert_eq!(
        document.validate_new_parameter(&duplicate),
        Err(Error::DuplicateId)
    );
    assert_eq!(document.suggested_parameter_id(), "ParamNew2");
    for value in [-1., 1.] {
        document.create_parameter(NewParameter {
            runtime_id: document.suggested_parameter_id(),
            default_value: value,
            ..Default::default()
        })?;
    }
    Ok(())
}

#[test]
fn group_creation_preserves_imported_entries_and_extensions_atomically() -> anyhow::Result<()> {
    let source = tempfile::tempdir()?;
    let parent = uuid();
    let child = uuid();
    let existing = uuid();
    let mut session = AuthoringSession::new(&uuid(), Canvas::new(256., 256., Vec2::default(), 1.))?;
    let info = DisplayInfo {
        origin: DisplayInfoOrigin::Imported,
        parameters: Some(vec![
            CdiParameterEntry::Resolved {
                parameter_id: existing.clone(),
                group_id: Some(parent.clone()),
                extensions: [("entryNote".into(), serde_json::json!("kept"))].into(),
            },
            CdiParameterEntry::Unresolved {
                runtime_id: "ParamNew2".into(),
                name: "Missing".into(),
                group_id: None,
                extensions: Default::default(),
            },
        ]),
        parameter_groups: Some(vec![
            CdiParameterGroup {
                id: parent.clone(),
                runtime_id: "Face".into(),
                name: "Face".into(),
                parent_id: None,
                extensions: Default::default(),
            },
            CdiParameterGroup {
                id: child.clone(),
                runtime_id: "Eyes".into(),
                name: "Eyes".into(),
                parent_id: Some(parent.clone()),
                extensions: Default::default(),
            },
        ]),
        ..Default::default()
    };
    session.edit("fixture", None, |edit| {
        edit.create_parameter(Parameter {
            id: existing.clone(),
            runtime_id: "ParamNew".into(),
            ..Default::default()
        })?;
        edit.replace_display_info(info.clone())
    })?;
    let receipt = session.save_project(&source.path().join("fixture.kasane"), None)?;
    let mut document = Document::open(&receipt.manifest)?;
    assert_eq!(document.suggested_parameter_id(), "ParamNew3");
    assert_eq!(
        document.validate_new_parameter(&NewParameter {
            runtime_id: "ParamNew2".into(),
            ..Default::default()
        }),
        Err(Error::DuplicateId)
    );
    assert_eq!(document.parameter_group_options()[1].path, "Face / Eyes");
    assert_eq!(
        document.parameter_groups()[1].parent_id.as_ref(),
        Some(&parent)
    );
    assert_eq!(document.parameters()[0].group_id.as_ref(), Some(&parent));
    let history = document.history_lengths();
    let id = document.create_parameter(NewParameter {
        runtime_id: "ParamNew3".into(),
        group_id: Some(child.clone()),
        ..Default::default()
    })?;
    assert_eq!(document.history_lengths().0, history.0 + 1);
    assert!(
        document
            .object_index()
            .relations
            .iter()
            .any(|edge| edge.source.id == id && edge.target.id == child)
    );
    document.undo()?;
    assert!(
        !document
            .object_index()
            .relations
            .iter()
            .any(|edge| edge.source.id == id)
    );
    document.redo()?;
    let saved = document.save(&source.path().join("created.kasane"))?;
    session.open_project(&saved.manifest, None)?;
    let final_info = session.document().display_info();
    assert_eq!(final_info.origin, info.origin);
    assert_eq!(final_info.parameter_groups, info.parameter_groups);
    assert_eq!(
        &final_info.parameters.as_ref().unwrap()[..2],
        info.parameters.as_ref().unwrap()
    );
    assert!(
        matches!(&final_info.parameters.as_ref().unwrap()[2], CdiParameterEntry::Resolved { parameter_id, group_id: Some(group_id), .. } if parameter_id == &id && group_id == &child)
    );
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(reopened.parameter_groups(), document.parameter_groups());
    assert_eq!(
        reopened.parameters().last().unwrap().group_id.as_ref(),
        Some(&child)
    );
    Ok(())
}

#[test]
fn creation_command_rejects_stale_document_and_busy_target() -> anyhow::Result<()> {
    let mut editor = Editor::from_document(Document::new_blank()?);
    let first = editor.active_id().unwrap();
    let command = editor.command(editor_core::CommandId::CreateParameter);
    assert!(editor.command_available(command));
    editor.new_document()?;
    assert!(!editor.command_available(command));
    assert!(
        editor
            .execute_for(
                first,
                EditorCommand::CreateParameter(NewParameter::default())
            )
            .is_err()
    );
    editor.activate(first)?;
    let destination = tempfile::tempdir()?;
    let save = editor.prepare_save(first, &destination.path().join("busy.kasane"), false)?;
    assert!(!editor.command_available(command));
    assert!(
        editor
            .execute_for(
                first,
                EditorCommand::CreateParameter(NewParameter::default())
            )
            .is_err()
    );
    editor.finish_save(&mut save.run())?;
    assert!(editor.document().unwrap().parameters().is_empty());
    Ok(())
}
