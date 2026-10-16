use kasane_project::{encode_project_cmo3, publish_cmo3, Cmo3Options, DocumentSession};
use std::path::PathBuf;

#[test]
fn imported_models_and_saved_projects_share_the_codec() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    for model in ["external_v42", "external_cyclic", "external_v50_bs_glue"] {
        let mut session = DocumentSession::new();
        let (result, _) =
            session.import_model3_authoring(&fixtures.join(model).join("model.model3.json"));
        assert!(result.status.is_ok(), "{model}: {:?}", result);
        let artifact = session
            .encode_cmo3(&Cmo3Options::default())
            .unwrap_or_else(|error| panic!("{model}: {error:?}"));
        assert!(!artifact.report.has_loss());
        assert_eq!(&artifact.bytes[..4], b"CAFF");
        let temp = std::env::temp_dir().join(format!(
            "kasane-cmo3-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&temp).unwrap();
        let project = temp.join("source");
        let result = session.save(&project);
        assert!(result.status.is_ok(), "{result:?}");
        let saved = encode_project_cmo3(&project, &Cmo3Options::default()).unwrap();
        assert_eq!(saved.bytes, artifact.bytes);
        let output = temp.join("model.cmo3");
        std::fs::write(&output, b"existing output").unwrap();
        publish_cmo3(&artifact, &output).unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), artifact.bytes);
        let mut cases = Vec::new();
        let doc = session.document();
        let mut poses = vec![std::collections::HashMap::new()];
        for id in doc.parameter_order().iter().take(3) {
            let p = doc.get_parameter(id).unwrap();
            poses.push(std::collections::HashMap::from([(id.clone(), p.minimum)]));
            poses.push(std::collections::HashMap::from([(id.clone(), p.maximum)]));
        }
        for pose in poses {
            let mut frame = kasane_core::DrawableFrame::default();
            assert!(kasane_core::evaluation::evaluate_frame(doc, &pose, &mut frame).is_ok());
            let canvas = doc.canvas();
            let params: std::collections::BTreeMap<_, _> = pose
                .iter()
                .map(|(id, value)| (doc.get_parameter(id).unwrap().runtime_id.clone(), *value))
                .collect();
            let positions: std::collections::BTreeMap<_, Vec<[f32; 2]>> = frame
                .drawables
                .iter()
                .filter(|d| d.visible)
                .map(|d| {
                    (
                        d.runtime_id.clone(),
                        d.positions
                            .iter()
                            .map(|p| {
                                [
                                    p.x * canvas.pixels_per_unit + canvas.origin.x,
                                    canvas.origin.y - p.y * canvas.pixels_per_unit,
                                ]
                            })
                            .collect(),
                    )
                })
                .collect();
            cases.push(serde_json::json!({"parameters":params,"positions":positions}));
        }
        // Export artifacts stay local to target for optional external-editor probes.
        let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/cmo3-acceptance");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join(format!("{model}.cmo3")), &artifact.bytes).unwrap();
        std::fs::write(
            target.join(format!("{model}.expected.json")),
            serde_json::to_vec_pretty(&cases).unwrap(),
        )
        .unwrap();
        std::fs::remove_dir_all(temp).unwrap();
    }
}

#[test]
fn failed_publication_preserves_existing_output() {
    let temp = std::env::temp_dir().join(format!("kasane-cmo3-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&temp).unwrap();
    let output = temp.join("existing.cmo3");
    std::fs::create_dir(&output).unwrap();
    std::fs::write(output.join("marker"), b"untouched").unwrap();
    let artifact = kasane_project::Cmo3Artifact {
        bytes: b"test".to_vec(),
        report: Default::default(),
    };
    assert!(publish_cmo3(&artifact, &output).is_err());
    assert_eq!(std::fs::read(output.join("marker")).unwrap(), b"untouched");
    assert_eq!(std::fs::read_dir(&temp).unwrap().count(), 1);
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn editable_physics_uses_bound_parameter_guids() {
    use kasane_core::{Canvas, Document, Parameter, Vec2};
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            "00000000-0000-4000-8000-000000002001",
            Canvas::new(128., 128., Vec2::new(64., 64.), 64.)
        )
        .is_ok());
    for (id, runtime) in [
        ("00000000-0000-4000-8000-000000002002", "ParamX"),
        ("00000000-0000-4000-8000-000000002003", "ParamY"),
    ] {
        assert!(doc
            .create_parameter(Parameter {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let source = include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json");
    let imported =
        kasane_project::import_physics3(&doc, "00000000-0000-4000-8000-000000002004", source)
            .unwrap();
    let session = DocumentSession::from_authoring_document(imported.candidate);
    let artifact = session.encode_cmo3(&Default::default()).unwrap();
    assert!(!artifact.report.has_loss());
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/cmo3-acceptance");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("advanced-physics.cmo3"), artifact.bytes).unwrap();
}
