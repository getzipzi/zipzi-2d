use kasane_core::{document::*, Canvas, Document, Vec2};
use kasane_project::{
    encode_project_cubism, publish_can3, publish_cubism, Can3Options, CubismOptions,
    DocumentSession,
};

#[test]
fn saved_projects_publish_a_consistent_animation_model_pair() {
    let temp = std::env::temp_dir().join(format!("kasane-can3-{}", uuid::Uuid::new_v4()));
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
            id: "ca030000-0000-4000-8000-000000000002".into(),
            name: "Empty timeline".into(),
            duration: 2.,
            fps: 24.,
            looping: false,
            restricted_beziers: false,
            fade_in: None,
            fade_out: None,
            tracks: Vec::new(),
            events: Vec::new(),
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
    let manifest = kasane_project::project_manifest(&source);
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
    let temp = std::env::temp_dir().join(format!("kasane-model-only-{}", uuid::Uuid::new_v4()));
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
            id: motion.clone(),
            name: "Motion".into(),
            duration: 1.,
            fps: 30.,
            looping: false,
            restricted_beziers: true,
            fade_in: None,
            fade_out: None,
            tracks: vec![],
            events: vec![],
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
            .publish(kasane_project::TransferFormat::Cubism, &output)
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
