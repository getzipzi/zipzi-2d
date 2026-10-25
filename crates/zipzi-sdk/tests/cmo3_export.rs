use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, Can3Options, Cmo3Options, CubismOptions};

#[test]
fn unsaved_psd_draft_exports_without_publishing_or_editing_the_session() {
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../zipzi-psd/tests/fixtures/layered.psd");
    let mut session = AuthoringSession::new(
        "00000000-0000-4000-8000-000000003001",
        Canvas::new(128., 128., Vec2::new(64., 64.), 64.),
    )
    .unwrap();
    session.import_psd_in_memory(&source, None).unwrap();
    let before = session.read_snapshot();
    let modified = session.modified();
    let artifact = session.encode_cmo3(&Cmo3Options::default()).unwrap();
    assert_eq!(&artifact.bytes[..4], b"CAFF");
    assert!(!artifact.report.has_loss());
    let pair = session.encode_cubism(&CubismOptions::default()).unwrap();
    assert_eq!(pair.cmo3.bytes, artifact.bytes);
    assert_eq!(&pair.can3.bytes[..4], b"CAFF");
    assert!(!pair.report.has_loss());
    let animation = session
        .encode_can3(&Can3Options {
            animation_name: Cmo3Options::default().model_name,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(animation.bytes, pair.can3.bytes);
    assert_eq!(session.version(), before.version());
    assert_eq!(session.modified(), modified);
    let after = session.read_snapshot();
    assert_eq!(after.project_path(), before.project_path());
    assert!(after.document().same_content(before.document()));
}

#[test]
fn boundless_draft_exports_all_editable_cubism_formats_without_mutation() {
    let session = AuthoringSession::new(
        "00000000-0000-4000-8000-000000003002",
        zipzi_core::ModelSetup::default(),
    )
    .unwrap();
    let before = session.read_snapshot();
    let modified = session.modified();
    let model = session.encode_cmo3(&Cmo3Options::default()).unwrap();
    let animation = session
        .encode_can3(&Can3Options {
            animation_name: Cmo3Options::default().model_name,
            ..Default::default()
        })
        .unwrap();
    let pair = session.encode_cubism(&CubismOptions::default()).unwrap();
    assert!(!pair.report.has_loss());
    assert_eq!(pair.cmo3.bytes, model.bytes);
    assert_eq!(pair.can3.bytes, animation.bytes);
    assert_eq!(session.version(), before.version());
    assert_eq!(session.modified(), modified);
    assert!(
        session
            .read_snapshot()
            .document()
            .same_content(before.document())
    );
}

#[test]
fn static_model_linked_animation_has_a_distinct_importable_scene_identity() {
    let root = std::env::temp_dir().join(format!("zipzi-static-cubism-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut sdk = AuthoringSession::new(
        "00000000-0000-4000-8000-000000003003",
        zipzi_core::ModelSetup::default(),
    )
    .unwrap();
    let artifact = sdk.encode_cubism(&CubismOptions::default()).unwrap();
    zipzi_project::publish_cubism(&artifact, &root.join("paired")).unwrap();
    sdk.import_cubism(&root.join("paired/model.cmo3"), None)
        .unwrap();
    assert!(sdk.document().validate_structure().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
