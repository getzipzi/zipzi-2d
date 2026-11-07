use zipzi_project::{Can3Options, Cmo3Options, CubismOptions, ProjectImporter};
use zipzi_sdk::{AuthoringSession, Snapshot};

#[test]
fn unsaved_psd_draft_exports_without_publishing_or_editing_the_session() {
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../zipzi-psd/tests/fixtures/layered.psd");
    let (imported, _) = ProjectImporter::default().import_psd(&source).unwrap();
    let session =
        AuthoringSession::new(Snapshot::new(imported.document, imported.resources).unwrap());
    let before = session.read_snapshot();
    let modified = session.is_dirty();
    let artifact = zipzi_project::encode_document_cmo3(
        session.snapshot().document(),
        session.snapshot().resources(),
        &Cmo3Options::default(),
    )
    .unwrap();
    assert_eq!(&artifact.bytes[..4], b"CAFF");
    assert!(!artifact.report.has_loss());
    let pair = zipzi_project::encode_document_cubism(
        session.snapshot().document(),
        session.snapshot().resources(),
        &CubismOptions::default(),
    )
    .unwrap();
    assert_eq!(pair.cmo3.bytes, artifact.bytes);
    assert_eq!(&pair.can3.bytes[..4], b"CAFF");
    assert!(!pair.report.has_loss());
    let animation = zipzi_cmo3::encode_can3(
        session.snapshot().document(),
        &Can3Options {
            animation_name: Cmo3Options::default().model_name,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(animation.bytes, pair.can3.bytes);
    assert_eq!(session.read_snapshot().source, before.source);
    assert_eq!(session.is_dirty(), modified);
    let after = session.read_snapshot();
    assert!(session.project().is_none());
    assert_eq!(after.snapshot.content_id(), before.snapshot.content_id());
}

#[test]
fn boundless_draft_exports_all_editable_cubism_formats_without_mutation() {
    let session = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                "00000000-0000-4000-8000-000000003002",
                (zipzi_core::ModelSetup::default()).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let before = session.read_snapshot();
    let modified = session.is_dirty();
    let model = zipzi_project::encode_document_cmo3(
        session.snapshot().document(),
        session.snapshot().resources(),
        &Cmo3Options::default(),
    )
    .unwrap();
    let animation = zipzi_cmo3::encode_can3(
        session.snapshot().document(),
        &Can3Options {
            animation_name: Cmo3Options::default().model_name,
            ..Default::default()
        },
    )
    .unwrap();
    let pair = zipzi_project::encode_document_cubism(
        session.snapshot().document(),
        session.snapshot().resources(),
        &CubismOptions::default(),
    )
    .unwrap();
    assert!(!pair.report.has_loss());
    assert_eq!(pair.cmo3.bytes, model.bytes);
    assert_eq!(pair.can3.bytes, animation.bytes);
    assert_eq!(session.read_snapshot().source, before.source);
    assert_eq!(session.is_dirty(), modified);
    assert_eq!(
        session.snapshot().content_id(),
        before.snapshot.content_id()
    );
}

#[test]
fn static_model_linked_animation_has_a_distinct_importable_scene_identity() {
    let root = std::env::temp_dir().join(format!("zipzi-static-cubism-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                "00000000-0000-4000-8000-000000003003",
                (zipzi_core::ModelSetup::default()).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let artifact = zipzi_project::encode_document_cubism(
        sdk.snapshot().document(),
        sdk.snapshot().resources(),
        &CubismOptions::default(),
    )
    .unwrap();
    zipzi_project::publish_cubism(&artifact, &root.join("paired")).unwrap();
    let imported = ProjectImporter::default()
        .import_cubism_authoring(&root.join("paired/model.cmo3"))
        .unwrap();
    sdk.replace(
        sdk.version(),
        Snapshot::new(imported.document, imported.resources).unwrap(),
    )
    .unwrap();
    assert!(sdk.snapshot().document().builder().freeze().is_ok());
    std::fs::remove_dir_all(root).unwrap();
}
