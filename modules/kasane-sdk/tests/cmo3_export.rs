use kasane_core::{Canvas, Vec2};
use kasane_sdk::{AuthoringSession, Can3Options, Cmo3Options, CubismOptions};

#[test]
fn unsaved_psd_draft_exports_without_publishing_or_editing_the_session() {
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../kasane-psd/tests/fixtures/layered.psd");
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
