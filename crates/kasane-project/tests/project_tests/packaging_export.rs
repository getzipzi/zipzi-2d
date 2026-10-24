use std::fs;
use std::path::Path;

use super::helpers::*;
use kasane_project::{content_sha256, decode_png, DocumentSession, DocumentStore};

#[test]
fn export_refuses_source_project_ancestor_and_source_asset_directories() {
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let manifest = fs::read(session.manifest()).unwrap();
    for target in [session.root(), tmp.0.clone(), session.root().join("assets")] {
        let result = session.export_package(&target);
        assert_eq!(result.status.code, "INVALID_DESTINATION", "{result:?}");
        assert_eq!(fs::read(session.manifest()).unwrap(), manifest);
    }
    assert!(session
        .export_package(&session.root().join("exports/model"))
        .status
        .is_ok());
}

#[cfg(unix)]
#[test]
fn path_aliases_do_not_bypass_export_guard_or_save_baseline() {
    let tmp = TestDirectory::new();
    let mut session = tmp.saved_session();
    let alias = tmp.0.join("alias");
    std::os::unix::fs::symlink(session.root(), &alias).unwrap();
    assert!(session
        .save(&alias.join("project.kasane.json"))
        .status
        .is_ok());
    assert_eq!(session.export_package(&alias).status.code, "INVALID_PATH");
    assert_eq!(
        session.export_package(&alias.join("assets/..")).status.code,
        "INVALID_DESTINATION"
    );
    let external = tmp.0.join("external");
    fs::create_dir(&external).unwrap();
    let assets = session.root().join("assets");
    fs::rename(&assets, external.join("assets")).unwrap();
    std::os::unix::fs::symlink(external.join("assets"), &assets).unwrap();
    assert_eq!(
        session
            .save(session.manifest().to_path_buf().as_path())
            .status
            .code,
        "INVALID_PATH"
    );
}

#[test]
fn export_rejects_hash_mismatch_and_keeps_existing_package() {
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let destination = tmp.0.join("package");
    assert!(session.export_package(&destination).status.is_ok());
    fs::write(destination.join("old-marker"), b"keep").unwrap();
    let asset = session.document().get_asset(&id(2)).unwrap();
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, 8, 8);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![255; 8 * 8 * 4])
            .unwrap();
    }
    fs::write(
        session.root().join(&asset.content.as_ref().unwrap().source),
        buf,
    )
    .unwrap();
    assert_eq!(
        session.export_package(&destination).status.code,
        "RESOURCE_HASH"
    );
    assert_eq!(fs::read(destination.join("old-marker")).unwrap(), b"keep");
}

#[test]
fn package_failures_rollback_and_report_recovery_copy_when_rollback_fails() {
    for fault in [
        Failure::ShortWrite,
        Failure::FileSync,
        Failure::DirectorySync(1),
        Failure::DirectorySync(2),
        Failure::Rename(vec![1]),
        Failure::Rename(vec![2]),
        Failure::Rename(vec![2, 3]),
    ] {
        let rollback_fails = matches!(&fault, Failure::Rename(v) if v.len() == 2);
        let tmp = TestDirectory::new();
        let initial = tmp.saved_session();
        let destination = tmp.0.join("package");
        assert!(initial.export_package(&destination).status.is_ok());
        fs::write(destination.join("old-marker"), b"original").unwrap();
        let mut session = DocumentSession::with_filesystem(FailingFileSystem::new(fault));
        assert!(session.open(initial.manifest()).status.is_ok());
        let result = session.export_package(&destination);
        assert!(!result.status.is_ok());
        assert!(!result.published);
        if rollback_fails {
            assert_eq!(result.status.code, "ROLLBACK_FAILED");
            let recovery = result.status.message.split("recover from ").nth(1).unwrap();
            assert_eq!(
                fs::read(Path::new(recovery).join("old-marker")).unwrap(),
                b"original"
            );
        } else {
            assert_eq!(
                fs::read(destination.join("old-marker")).unwrap(),
                b"original"
            );
        }
        assert!(initial.manifest().exists());
    }
}

#[test]
fn package_postcommit_sync_error_is_a_warning_not_a_failed_export() {
    let tmp = TestDirectory::new();
    let initial = tmp.saved_session();
    let mut session =
        DocumentSession::with_filesystem(FailingFileSystem::new(Failure::DirectorySync(3)));
    assert!(session.open(initial.manifest()).status.is_ok());
    let destination = tmp.0.join("package");
    let result = session.export_package(&destination);
    assert!(result.status.is_ok() && result.published);
    assert!(!result.durable && !result.warnings.is_empty());
    assert!(destination.join("model.moc3").exists());
}

#[test]
fn package_writes_the_same_bytes_that_were_verified() {
    let tmp = TestDirectory::new();
    let mut session = tmp.saved_session();
    let mut mesh = session.document().get_mesh(&id(4)).unwrap().clone();
    mesh.raw_blend_mode = Some(262);
    assert!(session.document_mut().replace_mesh(mesh).status.is_ok());
    let asset = session.document().get_asset(&id(2)).unwrap();
    let source = session.root().join(&asset.content.as_ref().unwrap().source);
    let before = fs::read(&source).unwrap();
    let destination = tmp.0.join("package");
    let options = kasane_project::PackageOptions {
        asset_root: session.root(),
        destination: destination.clone(),
        validate: Some(Box::new(move |_| {
            fs::write(&source, b"changed after verification").unwrap();
            Ok(kasane_project::PackageValidation::structural(
                kasane_project::RuntimeValidation::NotPerformed,
            ))
        })),
    };
    kasane_project::publish_package(session.document(), &options).unwrap();
    assert_eq!(
        fs::read(destination.join("textures/0.png")).unwrap(),
        before
    );
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(destination.join("export-report.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "published");
    assert_eq!(report["encoding"], "passed");
    assert_eq!(report["structural_validation"], "passed");
    assert_eq!(report["runtime_validation"], "not_performed");
    assert_eq!(report["moc_version"], 6);
}

#[test]
fn external_source_asset_directory_cannot_be_an_export_destination() {
    let tmp = TestDirectory::new();
    let mut session = tmp.saved_session();
    let external = tmp.0.join("external");
    let path = external.join("source.png");
    let (bytes, _) = create_test_png(&path);
    assert!(session.replace_asset(&id(2), &path).status.is_ok());
    assert_eq!(
        session.export_package(&external).status.code,
        "INVALID_DESTINATION"
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn a_failed_new_asset_write_can_be_retried_without_reusing_partial_bytes() {
    let tmp = TestDirectory::new();
    let (doc, input) = tmp.fixture();
    let destination = tmp.0.join("new-project");
    let failed = DocumentStore::with_filesystem(FailingFileSystem::new(Failure::ShortWrite));
    let result = failed.save(&doc, &input, &destination, None).0;
    assert_eq!(result.status.code, "PROJECT_IO");
    assert!(!destination.join("project.kasane.json").exists());
    let result = DocumentStore::new()
        .save(&doc, &input, &destination, None)
        .0;
    assert!(result.status.is_ok());
    assert!(DocumentStore::new()
        .open(&destination)
        .0
        .resources_complete());
}

#[test]
fn import_reports_truncated_pngs_for_both_entry_points() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let fixture = root.join("fixtures/external_v50");
    let tmp = TestDirectory::new();
    let png = fs::read(fixture.join("texture_00.png")).unwrap();
    fs::copy(fixture.join("model.moc3"), tmp.0.join("model.moc3")).unwrap();
    fs::copy(
        fixture.join("model.model3.json"),
        tmp.0.join("model.model3.json"),
    )
    .unwrap();
    for length in [24, png.len() - 12] {
        let path = tmp.0.join("texture_00.png");
        fs::write(&path, &png[..length]).unwrap();
        assert!(decode_png(&png[..length]).is_err());
        let map = std::collections::HashMap::from([(0, path)]);
        for result in [
            kasane_moc3::import_from_bare_moc3(&fs::read(tmp.0.join("model.moc3")).unwrap(), &map)
                .unwrap(),
            kasane_moc3::import_from_model3_file(&tmp.0.join("model.model3.json")).unwrap(),
        ] {
            assert!(!result.textures_complete);
            assert!(result
                .diagnostics
                .iter()
                .any(|d| d.code == "CORRUPT_TEXTURE"));
        }
        let mut session = DocumentSession::new();
        let (result, _) = session.import_model3(&tmp.0.join("model.model3.json"));
        assert!(result.status.is_ok());
        assert!(!result.resources_complete());
        let (result, _) = session.import_bare_moc3(&tmp.0.join("model.moc3"), &map);
        assert!(result.status.is_ok());
        assert!(!result.resources_complete());
    }
}

#[test]
fn export_plan_is_inspectable_and_publication_never_reopens_sources() {
    use kasane_project::{
        build_export_plan, publish_export_plan, PackageValidation, RuntimeValidation,
    };
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let plan = build_export_plan(session.document(), &session.root()).unwrap();
    let destination = tmp.0.join("planned-package");
    assert!(!destination.exists());
    assert_eq!(plan.source_revision(), session.document().revision());
    for reference in plan.references() {
        assert!(plan.files().contains_key(&reference.from));
        assert!(plan.files().contains_key(&reference.to));
    }
    let source = session.root().join(
        &session
            .document()
            .get_asset(&id(2))
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .source,
    );
    fs::remove_file(source).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("old"), b"preserve").unwrap();
    let error = publish_export_plan(&plan, &destination, &|_| {
        Err(kasane_core::Status::error("PACKAGE_REJECTED", "fixture"))
    })
    .unwrap_err();
    assert_eq!(error.code, "PACKAGE_REJECTED");
    assert_eq!(fs::read(destination.join("old")).unwrap(), b"preserve");
    publish_export_plan(&plan, &destination, &|candidate| {
        let model: serde_json::Value =
            serde_json::from_slice(candidate.files()["model.model3.json"].bytes()).unwrap();
        assert_eq!(model["FileReferences"]["DisplayInfo"], "model.cdi3.json");
        for file in candidate.files().values() {
            assert_eq!(content_sha256(file.bytes()), file.sha256());
        }
        Ok(PackageValidation::structural(
            RuntimeValidation::NotPerformed,
        ))
    })
    .unwrap();
    for (path, file) in plan.files() {
        assert_eq!(fs::read(destination.join(path)).unwrap(), file.bytes());
    }
    assert!(!destination.join("old").exists());
}

#[test]
fn transfer_replaces_removed_and_renamed_runtime_files_but_keeps_unmanaged_files() {
    use kasane_core::document::MotionClip;
    use kasane_project::TransferFormat;
    let tmp = TestDirectory::new();
    let mut session = tmp.saved_session();
    let motion = MotionClip {
        track_order: Vec::new(),
        id: id(800),
        name: "Original motion".into(),
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
    };
    assert!(session
        .document_mut()
        .create_motion(motion.clone())
        .status
        .is_ok());
    let output = tmp.0.join("transfer-runtime");
    let publish = |session: &DocumentSession| {
        session
            .transfer_snapshot()
            .unwrap()
            .publish(TransferFormat::Runtime, &output)
    };
    let motion_path = || {
        let model: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("model.model3.json")).unwrap()).unwrap();
        output.join(
            model["FileReferences"]["Motions"]["Default"][0]["File"]
                .as_str()
                .unwrap(),
        )
    };
    publish(&session).unwrap();
    let original = motion_path();
    let mut renamed = motion;
    renamed.name = "Renamed motion".into();
    assert!(session
        .document_mut()
        .replace_motion(renamed)
        .status
        .is_ok());
    publish(&session).unwrap();
    let renamed = motion_path();
    assert!(renamed.is_file());
    assert!(!original.exists());
    assert!(session.document_mut().erase_object(&id(800)).status.is_ok());
    // A user-created neighbor in the former managed directory must still block replacement.
    let notes = output.join("motions/notes.txt");
    fs::write(&notes, b"keep").unwrap();
    assert_eq!(publish(&session).unwrap_err().code, "UNMANAGED_OUTPUT");
    assert_eq!(fs::read(&notes).unwrap(), b"keep");
    assert!(renamed.is_file());
    fs::remove_file(notes).unwrap();
    let saved_motion = fs::read(&renamed).unwrap();
    fs::write(&renamed, b"independently edited motion").unwrap();
    assert_eq!(publish(&session).unwrap_err().code, "UNMANAGED_OUTPUT");
    assert_eq!(fs::read(&renamed).unwrap(), b"independently edited motion");
    fs::write(&renamed, saved_motion).unwrap();
    #[cfg(unix)]
    {
        let outside = tmp.0.join("keep.txt");
        fs::write(&outside, b"keep outside").unwrap();
        let alias = output.join("motions/alias");
        std::os::unix::fs::symlink(&outside, &alias).unwrap();
        assert_eq!(publish(&session).unwrap_err().code, "UNMANAGED_OUTPUT");
        assert_eq!(fs::read(&outside).unwrap(), b"keep outside");
        fs::remove_file(alias).unwrap();
    }
    publish(&session).unwrap();
    assert!(!renamed.exists());
    assert!(!output.join("motions").exists());
    publish(&session).unwrap();
}
