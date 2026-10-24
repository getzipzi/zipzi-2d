use zipzi_core::{document::*, Canvas, Document, Vec2};
use zipzi_project::{
    encode_project_cubism, publish_can3, publish_cubism, Can3Options, CubismOptions,
    DocumentSession,
};

#[test]
fn expression_companions_keep_runtime_blends_and_publish_as_one_managed_group() {
    let temp = std::env::temp_dir().join(format!("zipzi-expressions-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&temp).unwrap();
    let output = temp.join("avatar");
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(128., 64., Vec2::default(), 32.)
        )
        .is_ok());
    let parameter = uuid::Uuid::new_v4().to_string();
    assert!(doc
        .create_parameter(zipzi_core::Parameter {
            id: parameter.clone(),
            runtime_id: "ParamX".into(),
            name: "X".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let expression = uuid::Uuid::new_v4().to_string();
    assert!(doc
        .create_expression(ExpressionAsset {
            id: expression.clone(),
            name: "Smile".into(),
            file_type: Some("Live2D Expression".into()),
            fade_in: Some(0.3),
            fade_out: Some(0.5),
            entries: vec![ExpressionEntry {
                id: uuid::Uuid::new_v4().to_string(),
                target: ExpressionTarget::Resolved {
                    parameter_id: parameter
                },
                value: 0.2,
                blend: Some(ExpressionBlend::Multiply),
                extensions: Default::default()
            }],
            extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
        .status
        .is_ok());
    let options = CubismOptions {
        base_name: "avatar".into(),
        ..Default::default()
    };
    let encode = |doc: &Document| {
        zipzi_cmo3::encode_cubism(doc, &|_: &zipzi_core::ImageAsset| unreachable!(), &options)
            .unwrap()
    };
    let artifact = encode(&doc);
    assert_eq!(artifact.can3.expression_files.len(), 2);
    let file = format!("expression-{expression}.exp3.json");
    let expected: serde_json::Value =
        serde_json::from_str(&zipzi_project::export_expression3(&doc, &expression).unwrap())
            .unwrap();
    let actual: serde_json::Value =
        serde_json::from_slice(&artifact.can3.expression_files[&file]).unwrap();
    assert_eq!(expected, actual);
    assert!(publish_cubism(&artifact, &output).unwrap().durable());
    assert_eq!(
        std::fs::read(output.join(&file)).unwrap(),
        artifact.can3.expression_files[&file]
    );
    publish_cubism(&artifact, &output).unwrap();
    let mut unsafe_artifact = artifact.clone();
    unsafe_artifact
        .can3
        .expression_files
        .insert("../escape.exp3.json".into(), b"keep".to_vec());
    assert!(publish_cubism(&unsafe_artifact, &output).is_err());
    assert!(!temp.join("escape.exp3.json").exists());
    std::fs::write(output.join("unrelated.exp3.json"), b"keep").unwrap();
    assert!(publish_cubism(&artifact, &output).is_err());
    assert_eq!(
        std::fs::read(output.join("unrelated.exp3.json")).unwrap(),
        b"keep"
    );
    std::fs::remove_file(output.join("unrelated.exp3.json")).unwrap();
    assert!(doc.erase_object(&expression).status.is_ok());
    publish_cubism(&encode(&doc), &output).unwrap();
    assert!(!output.join(&file).exists());
    assert!(!output.join("expressions.json").exists());
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn saved_projects_publish_a_consistent_animation_model_pair() {
    let temp = std::env::temp_dir().join(format!("zipzi-can3-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&temp).unwrap();
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            "ca030000-0000-4000-8000-000000000001",
            Canvas::new(128., 64., Vec2::new(64., 32.), 32.)
        )
        .is_ok());
    assert!(doc
        .create_motion(MotionClip {
            track_order: Vec::new(),
            id: "ca030000-0000-4000-8000-000000000002".into(),
            name: "Empty timeline".into(),
            duration: 2.,
            fps: 24.,
            looping: false,
            restricted_beziers: false,
            fade_in: None,
            fade_out: None,
            tracks: Default::default(),
            events: Default::default(),
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None
        })
        .status
        .is_ok());
    let mut session = DocumentSession::new();
    session.reset_authoring_document(doc);
    let revision = session.document().revision();
    let options = CubismOptions {
        base_name: "avatar".into(),
        ..Default::default()
    };
    let artifact = session.encode_cubism(&options).unwrap();
    assert!(!artifact.report.has_loss());
    let source = temp.join("source");
    assert!(session.save(&source).status.is_ok());
    let saved = encode_project_cubism(&source, &options).unwrap();
    assert_eq!(artifact.cmo3.bytes, saved.cmo3.bytes);
    assert_eq!(artifact.can3.bytes, saved.can3.bytes);
    let output = temp.join("editable");
    let publication = publish_cubism(&artifact, &output).unwrap();
    assert!(publication.durable());
    for (name, bytes) in [
        (&artifact.model_file_name, &artifact.cmo3.bytes),
        (&artifact.animation_file_name, &artifact.can3.bytes),
    ] {
        assert_eq!(std::fs::read(output.join(name)).unwrap(), *bytes);
    }
    assert!(output.join("export-report.json").is_file());
    // Replace an existing managed export as one package; preserve unrelated data.
    std::fs::write(output.join("avatar.cmo3"), b"old model").unwrap();
    std::fs::write(output.join("avatar.can3"), b"old animation").unwrap();
    publish_cubism(&artifact, &output).unwrap();
    assert_eq!(
        std::fs::read(output.join("avatar.can3")).unwrap(),
        artifact.can3.bytes
    );
    std::fs::write(output.join("notes.txt"), b"keep").unwrap();
    assert!(publish_cubism(&artifact, &output).is_err());
    assert_eq!(std::fs::read(output.join("notes.txt")).unwrap(), b"keep");
    let manifest = zipzi_project::project_manifest(&source);
    let saved_source = std::fs::read(&manifest).unwrap();
    assert!(publish_cubism(&artifact, &source).is_err());
    assert_eq!(std::fs::read(manifest).unwrap(), saved_source);
    let animation = session
        .encode_can3(&Can3Options {
            model_file_name: "avatar.cmo3".into(),
            animation_name: options.model.model_name.clone(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(artifact.can3.bytes, animation.bytes);
    let standalone = temp.join("standalone.can3");
    publish_can3(&animation, &standalone).unwrap();
    assert_eq!(std::fs::read(standalone).unwrap(), artifact.can3.bytes);
    assert_eq!(session.document().revision(), revision);
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn removing_last_motion_replaces_pair_with_model_only_without_deleting_unmanaged_files() {
    let temp = std::env::temp_dir().join(format!("zipzi-model-only-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&temp).unwrap();
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(100., 100., Vec2::default(), 1.)
        )
        .is_ok());
    let motion = uuid::Uuid::new_v4().to_string();
    assert!(doc
        .create_motion(MotionClip {
            track_order: Vec::new(),
            id: motion.clone(),
            name: "Motion".into(),
            duration: 1.,
            fps: 30.,
            looping: false,
            restricted_beziers: true,
            fade_in: None,
            fade_out: None,
            tracks: vec![].into(),
            events: vec![].into(),
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
        .status
        .is_ok());
    let mut session = DocumentSession::from_authoring_document(doc);
    let output = temp.join("avatar");
    let publish = |session: &DocumentSession| {
        session
            .transfer_snapshot()
            .unwrap()
            .publish(zipzi_project::TransferFormat::Cubism, &output)
    };
    publish(&session).unwrap();
    assert!(output.join("avatar.can3").is_file());
    assert!(session.document_mut().erase_object(&motion).status.is_ok());
    std::fs::write(output.join("other.can3"), b"keep").unwrap();
    assert!(publish(&session).is_err());
    assert_eq!(std::fs::read(output.join("other.can3")).unwrap(), b"keep");
    assert!(output.join("avatar.can3").is_file());
    std::fs::remove_file(output.join("other.can3")).unwrap();
    publish(&session).unwrap();
    assert!(output.join("avatar.cmo3").is_file());
    assert!(!output.join("avatar.can3").exists());
    publish(&session).unwrap();
    std::fs::remove_dir_all(temp).unwrap();
}
