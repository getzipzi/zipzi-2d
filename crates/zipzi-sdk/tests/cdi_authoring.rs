use serde_json::Value;
use zipzi_contract::authoring::ObjectKind;
use zipzi_core::document::{
    CdiCombinedSet, CdiParameterGroup, CdiParameterRef, OrganizationEdit, ReferenceEntry,
    ReferenceEntryAllocation,
};
use zipzi_core::{Canvas, Parameter, Vec2};
use zipzi_sdk::AuthoringSession;

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000a01";
const FIRST: &str = "00000000-0000-4000-8000-000000000a02";
const SECOND: &str = "00000000-0000-4000-8000-000000000a03";
const THIRD: &str = "00000000-0000-4000-8000-000000000a04";
const GROUP: &str = "00000000-0000-4000-8000-000000000a05";
const SET: &str = "00000000-0000-4000-8000-000000000a06";

fn session() -> AuthoringSession {
    let mut session = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                DOCUMENT,
                (Canvas::new(100.0, 100.0, Vec2::default(), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                for (id, runtime_id) in [(FIRST, "ParamX"), (SECOND, "ParamY")] {
                    edit.create_parameter(Parameter {
                        id: id.into(),
                        runtime_id: runtime_id.into(),
                        name: runtime_id.into(),
                        ..Default::default()
                    })?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("parameters")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    session
}

#[test]
fn generated_cdi_group_edit_keeps_other_parameters_and_future_parameters() {
    let mut session = session();
    let eval_revision = session.snapshot().content_id();
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.edit_organization(OrganizationEdit::CreateGroup {
                    group: CdiParameterGroup {
                        id: GROUP.into(),
                        runtime_id: "Face".into(),
                        name: "Face".into(),
                        parent_id: None,
                        extensions: Default::default(),
                    },
                    before: None,
                })?;
                edit.edit_organization(OrganizationEdit::SetParameterGroup {
                    parameter_id: FIRST.into(),
                    group_id: Some(GROUP.into()),
                })?;
                edit.rename(zipzi_sdk::NameTarget::Parameter(FIRST), "角度".into())?;
                {
                    let mut display = edit.read_document()?.display_info().clone();
                    display
                        .combined_parameters
                        .get_or_insert_default()
                        .push(CdiCombinedSet {
                            id: SET.into(),
                            members: vec![
                                ReferenceEntry::new(CdiParameterRef::Resolved {
                                    parameter_id: FIRST.into(),
                                }),
                                ReferenceEntry::new(CdiParameterRef::Resolved {
                                    parameter_id: SECOND.into(),
                                }),
                            ],
                        });
                    edit.replace_display_info(display)
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("CDI")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_ne!(session.snapshot().content_id(), eval_revision);
    let group_handle = {
        let kind = ObjectKind::CdiParameterGroup;
        let object_id = (GROUP).to_owned();
        session
            .snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, session.read_snapshot().source))
    }
    .unwrap();
    let set_handle = {
        let kind = ObjectKind::CdiCombinedSet;
        let object_id = (SET).to_owned();
        session
            .snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, session.read_snapshot().source))
    }
    .unwrap();
    let output: Value =
        serde_json::from_str(&zipzi_project::export_cdi3(session.snapshot().document()).unwrap())
            .unwrap();
    assert_eq!(output["Parameters"].as_array().unwrap().len(), 2);
    assert_eq!(output["Parameters"][0]["GroupId"], "Face");
    assert_eq!(output["Parameters"][1]["Id"], "ParamY");
    assert_eq!(
        output["CombinedParameters"][0],
        serde_json::json!(["ParamX", "ParamY"])
    );
    session.undo(session.version()).unwrap();
    assert!((session.read_snapshot().source != group_handle.2));
    session.redo(session.version()).unwrap();
    assert!((session.read_snapshot().source != group_handle.2));
    assert!((session.read_snapshot().source != set_handle.2));
    assert_ne!(session.snapshot().content_id(), eval_revision);

    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_parameter(Parameter {
                    id: THIRD.into(),
                    runtime_id: "ParamZ".into(),
                    name: "Z".into(),
                    ..Default::default()
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("new parameter")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let output: Value =
        serde_json::from_str(&zipzi_project::export_cdi3(session.snapshot().document()).unwrap())
            .unwrap();
    assert_eq!(output["Parameters"].as_array().unwrap().len(), 3);
    assert_eq!(output["Parameters"][2]["Id"], "ParamZ");
}

#[test]
fn imported_cdi_is_atomic_and_preserves_field_diagnostics() {
    let mut session = session();
    let input = r#"{"Version":3,"Parameters":[{"Id":"ParamX","GroupId":"","Name":"X"},{"Id":"Missing","GroupId":"","Name":"?"}]}"#;
    let (diagnostics, _) = {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                let imported = zipzi_project::import_cdi3(&edit.read_document()?, input)
                    .map_err(|error| zipzi_sdk::Status::error(error.code, error.path))?;
                for id in imported.candidate.parameter_order() {
                    edit.replace_parameter(imported.candidate.parameter(id).unwrap().clone())?;
                }
                edit.replace_display_info(imported.candidate.display_info().clone())?;
                Ok(imported.diagnostics)
            })
            .and_then(|value| {
                let prepared = draft.prepare("import CDI")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].path, "$.Parameters[1].Id");
    assert_eq!(
        zipzi_project::export_cdi3(session.snapshot().document())
            .unwrap_err()
            .path,
        "$.Parameters[1].Id"
    );
    let version = session.version();
    let bad = r#"{"Version":3,"Parameters":[{"Id":"ParamX","GroupId":"Unknown","Name":"X"}]}"#;
    assert!({
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                let imported = zipzi_project::import_cdi3(&edit.read_document()?, bad)
                    .map_err(|error| zipzi_sdk::Status::error(error.code, error.path))?;
                for id in imported.candidate.parameter_order() {
                    edit.replace_parameter(imported.candidate.parameter(id).unwrap().clone())?;
                }
                edit.replace_display_info(imported.candidate.display_info().clone())?;
                Ok(imported.diagnostics)
            })
            .and_then(|value| {
                let prepared = draft.prepare("bad CDI")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(session.version(), version);
}
