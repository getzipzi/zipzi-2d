mod support;
use serde_json::{json, Value};
use support::*;
use zipzi_core::state::DocumentState;
use zipzi_core::{Canvas, Parameter, Vec2};
use zipzi_project::{export_expression3, import_expression3};
use zipzi_project::{ProjectImporter, TransferFormat, TransferSnapshot};

const DOC: &str = "00000000-0000-4000-8000-000000000b01";
const PARAM: &str = "00000000-0000-4000-8000-000000000b02";
const EXP: &str = "00000000-0000-4000-8000-000000000b03";

fn document() -> DocumentState {
    let mut document =
        DocumentState::new(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0).into()).unwrap();
    document = prepare(&document, |builder| {
        builder.create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamX".into(),
            name: "X".into(),
            ..Default::default()
        })
    });
    document
}

#[test]
fn expression_import_roundtrips_native_project_and_runtime_id_renames() {
    let input = r#"{"Type":"Live2D Expression","FadeInTime":0.25,"Parameters":[{"Id":"ParamX","Value":0.75,"Blend":"Overwrite"}]}"#;
    let imported = import_expression3(&document(), EXP, "Smile", input).unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut reopened = roundtrip(&imported.candidate);
    assert_eq!(reopened.expression_order(), &[EXP]);
    let output: Value = serde_json::from_str(&export_expression3(&reopened, EXP).unwrap()).unwrap();
    assert_eq!(output, serde_json::from_str::<Value>(input).unwrap());
    let mut parameter = reopened.parameter(PARAM).unwrap().clone();
    parameter.runtime_id = "ParamChanged".into();
    reopened = prepare(&reopened, |builder| builder.replace_parameter(parameter));
    let output: Value = serde_json::from_str(&export_expression3(&reopened, EXP).unwrap()).unwrap();
    assert_eq!(output["Parameters"][0]["Id"], "ParamChanged");
    let mut invalid = reopened.builder();
    invalid.remove_parameter(PARAM).unwrap();
    assert!(invalid.freeze().is_err());
}

#[test]
fn unresolved_and_opaque_expression_targets_have_precise_export_errors() {
    let input = r#"{"Parameters":[{"Id":"Missing","Value":0.5}]}"#;
    let imported = import_expression3(&document(), EXP, "Missing", input).unwrap();
    assert_eq!(imported.diagnostics[0].path, "$.Parameters[0].Id");
    let reopened = roundtrip(&imported.candidate);
    assert_eq!(
        export_expression3(&reopened, EXP).unwrap_err().code,
        "UNRESOLVED_PARAMETER"
    );
    let input = r#"{"Parameters":[{"Id":"ParamX","Value":0.5}],"Future":{"Label":"保留"}}"#;
    let imported = import_expression3(&document(), EXP, "Smile", input)
        .unwrap()
        .candidate;
    let mut renamed = roundtrip(&imported);
    let mut parameter = renamed.parameter(PARAM).unwrap().clone();
    parameter.runtime_id = "Changed".into();
    renamed = prepare(&renamed, |builder| builder.replace_parameter(parameter));
    assert_eq!(
        export_expression3(&renamed, EXP).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );

    let source = r#"{"Parameters":[{"Id":"ParamX","Value":0.5}],"Future":{"Label":"保留"}}"#;
    let mut candidate = import_expression3(&document(), EXP, "Smile", source)
        .unwrap()
        .candidate;
    let mut edited = candidate.expression(EXP).unwrap().clone();
    edited.fade_in = Some(0.25);
    candidate = prepare(&candidate, |builder| builder.replace_expression(edited));
    assert_eq!(
        export_expression3(&candidate, EXP).unwrap_err().code,
        "OPAQUE_CONTENT_CHANGED"
    );
    let reimported = import_expression3(&candidate, EXP, "Smile", source).unwrap();
    assert!(export_expression3(&reimported.candidate, EXP).is_ok());
}

#[test]
fn model3_expression_package_import_is_atomic() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = root.join("fixtures/external_v50/model.model3.json");
    let importer = ProjectImporter::default();
    let (result, session, _) = importer.import_model3(&source);
    assert!(result.status.is_ok(), "{result:?}");
    let session = session.unwrap();
    let runtime_id = session
        .document
        .parameter(&session.document.parameter_order()[0])
        .unwrap()
        .runtime_id
        .clone();
    let expression = json!({"Type":"Live2D Expression","Parameters":[{
        "Id":runtime_id,"Value":0.25,"Blend":"Add"
    }]})
    .to_string();
    let mut imported = import_expression3(&session.document, EXP, "Smile", &expression).unwrap();
    imported.candidate = prepare(&imported.candidate, |builder| {
        builder.set_missing_attachments(Vec::new())
    });
    let exported_state = imported.candidate;
    let temporary =
        std::env::temp_dir().join(format!("zipzi-expression-model3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    TransferSnapshot::new(exported_state, session.resources, session.protected_paths)
        .unwrap()
        .publish(TransferFormat::Runtime, &package)
        .unwrap();
    let model3: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(model3["FileReferences"]["Expressions"][0]["Name"], "Smile");
    let relative = model3["FileReferences"]["Expressions"][0]["File"]
        .as_str()
        .unwrap();
    assert_eq!(relative, "expressions/Smile.exp3.json");
    assert!(package.join(relative).is_file());

    let (result, reopened, report) = importer.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    let reopened = reopened.unwrap();
    assert!(!report
        .unwrap()
        .unimported_attachments
        .iter()
        .any(|item| item.starts_with("Expressions:")));
    assert_eq!(reopened.document.expression_order().len(), 1);
    let id = &reopened.document.expression_order()[0];
    assert_eq!(reopened.document.expression(id).unwrap().name, "Smile");
    let before = reopened.document.content_id();
    std::fs::write(package.join(relative), "{broken").unwrap();
    let (result, failed, _) = importer.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert!(failed.is_none());
    assert_eq!(reopened.document.content_id(), before);
    assert_eq!(reopened.document.expression_order().len(), 1);
    std::fs::remove_dir_all(temporary).unwrap();
}
