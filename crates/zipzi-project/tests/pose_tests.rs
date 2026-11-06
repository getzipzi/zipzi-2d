mod support;
use serde_json::Value;
use support::*;
use zipzi_core::state::DocumentState;
use zipzi_core::{Canvas, Part, Vec2};
use zipzi_project::{export_pose3, import_pose3};
use zipzi_project::{ProjectImporter, TransferFormat, TransferSnapshot};

const DOC: &str = "00000000-0000-4000-8000-000000000e01";
const PART_A: &str = "00000000-0000-4000-8000-000000000e02";
const PART_B: &str = "00000000-0000-4000-8000-000000000e03";
const POSE: &str = "00000000-0000-4000-8000-000000000e04";

fn document() -> DocumentState {
    let mut doc =
        DocumentState::new(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0).into()).unwrap();
    for (id, runtime) in [(PART_A, "Part0"), (PART_B, "Part1")] {
        doc = prepare(&doc, |builder| {
            builder.create_part(Part {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                ..Default::default()
            })
        });
    }
    doc
}

#[test]
fn pose3_import_roundtrips_uuid_refs_and_native_project() {
    let source = include_str!("../../../fixtures/animation_cpu/minimal.pose3.json");
    let imported = import_pose3(&document(), POSE, source).unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut reopened = roundtrip(&imported.candidate);
    assert!(reopened.builder().freeze().is_ok());
    let encoded = export_pose3(&reopened).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(source).unwrap(),
        serde_json::from_str::<Value>(&encoded).unwrap()
    );
    let mut invalid = reopened.builder();
    invalid.remove_part(PART_A).unwrap();
    assert!(invalid.freeze().is_err());
    reopened = prepare(&reopened, |builder| builder.set_pose(None));
    reopened = prepare(&reopened, |builder| builder.remove_part(PART_A));
    assert!(reopened.part(PART_A).is_none());
}

#[test]
fn unresolved_and_opaque_pose_refs_block_strict_export() {
    let input = r#"{"Groups":[[{"Id":"Missing"}]]}"#;
    let imported = import_pose3(&document(), POSE, input).unwrap();
    assert_eq!(imported.diagnostics[0].path, "$.Groups[0][0].Id");
    assert_eq!(
        export_pose3(&imported.candidate).unwrap_err().code,
        "UNRESOLVED_PART"
    );
    let input = r#"{"Groups":[[{"Id":"Part0"}]],"Future":"keep"}"#;
    let mut candidate = import_pose3(&document(), POSE, input).unwrap().candidate;
    let mut part = candidate.part(PART_A).unwrap().clone();
    part.runtime_id = "Changed".into();
    candidate = prepare(&candidate, |builder| builder.replace_part(part));
    assert_eq!(
        export_pose3(&candidate).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn model3_pose_package_roundtrip_and_corrupt_import_rollback() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let importer = ProjectImporter::default();
    let (result, session, _) =
        importer.import_model3(&root.join("fixtures/external_v50/model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    let session = session.unwrap();
    let part_id = session.document.part_order()[0].clone();
    let runtime_id = session.document.part(&part_id).unwrap().runtime_id.clone();
    let source =
        serde_json::json!({"Type":"Live2D Pose","FadeInTime":0.5,"Groups":[[{"Id":runtime_id}]]})
            .to_string();
    let mut imported = import_pose3(&session.document, POSE, &source).unwrap();
    imported.candidate = prepare(&imported.candidate, |builder| {
        builder.set_missing_attachments(Vec::new())
    });
    let exported_state = imported.candidate;
    let temporary = std::env::temp_dir().join(format!("zipzi-pose-model3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    TransferSnapshot::new(exported_state, session.resources, session.protected_paths)
        .unwrap()
        .publish(TransferFormat::Runtime, &package)
        .unwrap();
    let model3: Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(model3["FileReferences"]["Pose"], "model.pose3.json");
    assert!(package.join("model.pose3.json").is_file());

    let (result, reopened, _) = importer.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    let reopened = reopened.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&export_pose3(&reopened.document).unwrap().unwrap()).unwrap(),
        serde_json::from_str::<Value>(&source).unwrap()
    );
    let revision = reopened.document.content_id();
    std::fs::write(package.join("model.pose3.json"), "{broken").unwrap();
    let (result, failed, _) = importer.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert!(failed.is_none());
    assert_eq!(reopened.document.content_id(), revision);
    std::fs::remove_dir_all(temporary).unwrap();
}
