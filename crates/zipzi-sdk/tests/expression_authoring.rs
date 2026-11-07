use serde_json::Value;
use zipzi_core::{state::DocumentState, ModelSetup, Parameter};
use zipzi_sdk::{AuthoringSession, ResourceStore, ResourceView, Snapshot, Status};
const DOC: &str = "00000000-0000-4000-8000-000000000c01";
const PARAM: &str = "00000000-0000-4000-8000-000000000c02";
const EXP: &str = "00000000-0000-4000-8000-000000000c03";
fn session() -> AuthoringSession {
    let mut b = DocumentState::new(DOC, ModelSetup::default())
        .unwrap()
        .builder();
    b.create_parameter(Parameter {
        id: PARAM.into(),
        runtime_id: "ParamX".into(),
        name: "X".into(),
        ..Default::default()
    })
    .unwrap();
    AuthoringSession::new(
        Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
    )
}
fn import(
    s: &mut AuthoringSession,
    input: &str,
) -> Result<Vec<zipzi_project::ExpressionDiagnostic>, Status> {
    let mut d = s.begin_draft();
    let diagnostics = d.batch(d.source(), |c| {
        let imported = zipzi_project::import_expression3(&c.read_document()?, EXP, "Smile", input)
            .map_err(|e| Status::error(e.code, format!("{}: {}", e.path, e.message)))?;
        c.create_expression(imported.candidate.expression(EXP).unwrap().clone())?;
        Ok(imported.diagnostics)
    })?;
    s.publish(d.prepare("expression import")?)?;
    Ok(diagnostics)
}
#[test]
fn expression_import_is_undoable_and_survives_native_save() {
    let mut s = session();
    let content = s.snapshot().content_id();
    let input =
        r#"{"Type":"Live2D Expression","Parameters":[{"Id":"ParamX","Value":0.5,"Blend":"Add"}]}"#;
    assert!(import(&mut s, input).unwrap().is_empty());
    assert_ne!(s.snapshot().content_id(), content);
    let output: Value = serde_json::from_str(
        &zipzi_project::export_expression3(s.snapshot().document(), EXP).unwrap(),
    )
    .unwrap();
    assert_eq!(output["Parameters"][0]["Id"], "ParamX");
    let stale = s.begin_draft().prepare("before undo").unwrap();
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().expression(EXP).is_none());
    s.redo(s.version()).unwrap();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("model.zipzi");
    let io = zipzi_sdk::ProjectIo::new(root.path(), ResourceStore::default());
    s.save_as(&io, &path, None, || true).unwrap();
    let (reopened, diagnostics) = AuthoringSession::open(&io, &path, || true).unwrap();
    assert!(diagnostics.is_empty());
    assert_eq!(
        reopened.snapshot().document().expression(EXP).unwrap().name,
        "Smile"
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &zipzi_project::export_expression3(reopened.snapshot().document(), EXP).unwrap()
        )
        .unwrap(),
        output
    );
}
#[test]
fn failed_expression_import_keeps_publication_atomic_and_unresolved_targets_explicit() {
    let mut s = session();
    let version = s.version();
    let invalid = r#"{"Parameters":[{"Id":"ParamX","Value":0.5,"Blend":"Unknown"}]}"#;
    let diagnostic =
        zipzi_project::import_expression3(s.snapshot().document(), EXP, "Smile", invalid)
            .unwrap_err();
    assert_eq!(diagnostic.path, "$.Parameters[0].Blend");
    let error = import(&mut s, invalid).unwrap_err();
    assert_eq!(error.code, diagnostic.code);
    assert_eq!(s.version(), version);
    assert!(s.snapshot().document().expression_order().is_empty());
    let unresolved = r#"{"Parameters":[{"Id":"Missing","Value":0.5}]}"#;
    let diagnostics = import(&mut s, unresolved).unwrap();
    assert_eq!(diagnostics[0].path, "$.Parameters[0].Id");
    assert_eq!(
        zipzi_project::export_expression3(s.snapshot().document(), EXP)
            .unwrap_err()
            .code,
        "UNRESOLVED_PARAMETER"
    );
}
