mod support;
use support::*;
use zipzi_core::state::DocumentState;
use zipzi_core::{Canvas, Parameter, Vec2};
use zipzi_project::{export_physics3, import_physics3};
use zipzi_project::{ProjectImporter, TransferFormat, TransferSnapshot};

const DOC: &str = "00000000-0000-4000-8000-000000000f11";
const X: &str = "00000000-0000-4000-8000-000000000f12";
const Y: &str = "00000000-0000-4000-8000-000000000f13";
const PHYSICS: &str = "00000000-0000-4000-8000-000000000f14";

fn document() -> DocumentState {
    let mut doc =
        DocumentState::new(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0).into()).unwrap();
    for (id, runtime) in [(X, "ParamX"), (Y, "ParamY")] {
        doc = prepare(&doc, |builder| {
            builder.create_parameter(Parameter {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                minimum: -1.0,
                maximum: 1.0,
                default_value: 0.0,
                ..Default::default()
            })
        });
    }
    doc
}

#[test]
fn physics3_roundtrip_project_and_reference_protection() {
    let source = include_str!("../../../fixtures/animation_cpu/thirty.physics3.json");
    let imported = import_physics3(&document(), PHYSICS, source).unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut reopened = roundtrip(&imported.candidate);
    assert!(reopened.builder().freeze().is_ok());
    let actual: serde_json::Value =
        serde_json::from_str(&export_physics3(&reopened).unwrap().unwrap()).unwrap();
    let expected: serde_json::Value = serde_json::from_str(source).unwrap();
    assert_eq!(
        actual["Meta"]["PhysicsSettingCount"],
        expected["Meta"]["PhysicsSettingCount"]
    );
    assert_eq!(
        actual["PhysicsSettings"][0]["Input"][0]["Source"]["Id"],
        "ParamX"
    );
    let mut invalid = reopened.builder();
    invalid.remove_parameter(X).unwrap();
    assert!(invalid.freeze().is_err());
    reopened = prepare(&reopened, |builder| builder.set_physics(None));
    reopened = prepare(&reopened, |builder| builder.remove_parameter(X));
    assert!(reopened.parameter(X).is_none());
}

#[test]
fn physics_group_enabled_state_roundtrips_and_filters_exports() {
    use zipzi_core::document::{PhysicsPatch, PhysicsProperty, PhysicsRigPatch};
    use zipzi_project::export_available_physics3;

    let mut source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/animation_cpu/multi.physics3.json"
    ))
    .unwrap();
    source["PhysicsSettings"][1]["Input"][0]["Source"]["Id"] = "ParamMissing".into();
    let doc = import_physics3(&document(), PHYSICS, &source.to_string())
        .unwrap()
        .candidate;
    let original = doc.physics().as_deref().unwrap();
    let first = &original.data.settings[0];
    let second = &original.data.settings[1];
    let native = native_bytes(&doc);
    let mut json = project_json(&native);
    let group_path = "/document/animation_assets/physics/data/settings/1";
    assert!(json.pointer(&format!("{group_path}/enabled")).is_none());

    let disable = |state: &DocumentState, group_id: &str| {
        prepare(state, |builder| {
            builder.patch_physics(&PhysicsPatch {
                asset_id: PHYSICS.into(),
                asset_changes: vec![],
                rig: Some(PhysicsRigPatch {
                    group_id: group_id.into(),
                    changes: vec![PhysicsProperty::Enabled(false)],
                }),
            })
        })
    };
    for version in [22, 23, 24] {
        json["format_version"] = version.into();
        let legacy = load_native(&replace_project_json(&native, &json)).unwrap();
        assert_eq!(legacy.physics(), doc.physics());
        let disabled = disable(&legacy, &second.id);
        let saved = native_bytes(&disabled);
        let saved_json = project_json(&saved);
        assert_eq!(saved_json["format_version"], 24);
        assert_eq!(
            saved_json.pointer(&format!("{group_path}/enabled")),
            Some(&serde_json::Value::Bool(false))
        );
        let reopened = load_native(&saved).unwrap();
        let asset = reopened.physics().as_deref().unwrap();
        assert_eq!(reopened.physics(), disabled.physics());
        assert_eq!(asset.data.settings[1].inputs, second.inputs);
        assert_eq!(asset.data.settings[1].outputs, second.outputs);

        let strict = export_physics3(&reopened).unwrap().unwrap();
        assert_eq!(
            export_available_physics3(&reopened).unwrap().unwrap(),
            strict
        );
        let exported: serde_json::Value = serde_json::from_str(&strict).unwrap();
        assert_eq!(exported["Meta"]["PhysicsSettingCount"], 1);
        assert_eq!(exported["Meta"]["TotalInputCount"], 1);
        assert_eq!(exported["Meta"]["TotalOutputCount"], 1);
        assert_eq!(exported["Meta"]["VertexCount"], 2);
        assert_eq!(
            exported["Meta"]["PhysicsDictionary"][0]["Id"],
            first.external_id
        );
        assert_eq!(exported["PhysicsSettings"][0]["Id"], first.external_id);

        let all_disabled = disable(&reopened, &first.id);
        assert!(export_physics3(&all_disabled).unwrap().is_none());
        assert!(export_available_physics3(&all_disabled).unwrap().is_none());
    }

    let disabled = disable(&doc, &second.id);
    let cubism = zipzi_cmo3::encode_cmo3(
        &disabled,
        &|_: &zipzi_core::state::AssetRecord| unreachable!("document has no image assets"),
        &Default::default(),
    )
    .unwrap();
    let imported = zipzi_cmo3::import_model(&cubism.bytes).unwrap();
    let groups = &imported
        .document
        .physics()
        .as_deref()
        .unwrap()
        .data
        .settings;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].external_id, first.external_id);
}

#[test]
fn physics3_unresolved_and_opaque_edits_block_strict_export() {
    let source = include_str!("../../../fixtures/animation_cpu/thirty.physics3.json");
    let mut missing = document();
    missing = prepare(&missing, |builder| builder.remove_parameter(Y));
    let imported = import_physics3(&missing, PHYSICS, source).unwrap();
    assert_eq!(imported.diagnostics[0].code, "UNRESOLVED_PARAMETER");
    assert_eq!(
        export_physics3(&imported.candidate).unwrap_err().code,
        "UNRESOLVED_PARAMETER"
    );

    let mut value: serde_json::Value = serde_json::from_str(source).unwrap();
    value["Future"] = "keep".into();
    let mut candidate = import_physics3(&document(), PHYSICS, &value.to_string())
        .unwrap()
        .candidate;
    let mut parameter = candidate.parameter(X).unwrap().clone();
    parameter.runtime_id = "Renamed".into();
    candidate = prepare(&candidate, |builder| builder.replace_parameter(parameter));
    assert_eq!(
        export_physics3(&candidate).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn model3_physics_package_roundtrip_and_bad_attachment_rollback() {
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
    let source = include_str!("../../../fixtures/animation_cpu/thirty.physics3.json");
    let mut imported = import_physics3(&session.document, PHYSICS, source).unwrap();
    assert!(imported.diagnostics.is_empty());
    imported.candidate = prepare(&imported.candidate, |builder| {
        builder.set_missing_attachments(Vec::new())
    });
    let exported_state = imported.candidate;
    let temporary =
        std::env::temp_dir().join(format!("zipzi-physics-model3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temporary);
    std::fs::create_dir_all(&temporary).unwrap();
    let package = temporary.join("package");
    TransferSnapshot::new(exported_state, session.resources, session.protected_paths)
        .unwrap()
        .publish(TransferFormat::Runtime, &package)
        .unwrap();
    let model3: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join("model.model3.json")).unwrap()).unwrap();
    assert_eq!(model3["FileReferences"]["Physics"], "model.physics3.json");
    assert!(package.join("model.physics3.json").is_file());

    let (result, reopened, _) = importer.import_model3(&package.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    let reopened = reopened.unwrap();
    assert!(reopened.document.physics().is_some());
    let revision = reopened.document.content_id();
    std::fs::write(package.join("model.physics3.json"), "{broken").unwrap();
    let (result, failed, _) = importer.import_model3(&package.join("model.model3.json"));
    assert!(!result.status.is_ok());
    assert!(failed.is_none());
    assert_eq!(reopened.document.content_id(), revision);
    std::fs::remove_dir_all(temporary).unwrap();
}
