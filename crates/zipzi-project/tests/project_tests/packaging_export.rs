use super::helpers::*;
use std::{fs, path::Path};
use zipzi_core::state::*;
use zipzi_project::*;

#[test]
fn runtime_package_roundtrips_cursor_preview_companion_and_reports_sdk_limit() {
    use zipzi_core::document::{CursorPreviewAxis, CursorPreviewBinding};
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let standard_moc = zipzi_moc3::encode_moc3(&doc).unwrap().bytes;
    let binding = CursorPreviewBinding {
        axis: CursorPreviewAxis::RightX,
        influence_percent: 57.12500001,
        reversed: true,
    };
    let mut state = doc.editor_state().clone();
    state.cursor_preview_bindings = Some([(id(6), binding)].into());
    let doc = prepare(&doc, |builder| builder.replace_editor_state(state));
    let output = tmp.0.join("runtime");
    let publication = TransferSnapshot::new(doc, resources, vec![])
        .unwrap()
        .publish(TransferFormat::Runtime, &output)
        .unwrap();
    assert!(publication.durable());
    assert_eq!(publication.notices.len(), 1);
    assert_eq!(fs::read(output.join("model.moc3")).unwrap(), standard_moc);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("export-report.json")).unwrap()).unwrap();
    assert_eq!(
        report["cursor_preview"]["code"],
        "CURSOR_PREVIEW_RUNTIME_UNSUPPORTED"
    );
    let importer = ProjectImporter::default();
    let (result, imported, _) = importer.import_model3(&output.join("model.model3.json"));
    let imported = imported.unwrap_or_else(|| panic!("{result:?}"));
    let document = &imported.document;
    let parameter = document.parameter_by_runtime_id("ParamAngle").unwrap();
    assert_eq!(
        document.editor_state().cursor_preview_bindings,
        Some([(parameter.id.clone(), binding)].into())
    );
    assert!(
        !document
            .model3_settings()
            .extensions
            .contains_key("ZipziCursorPreview")
    );
    assert!(build_export_plan(document, &imported.resources).is_ok());
}

#[test]
fn captured_transfers_protect_source_ancestors_and_external_images() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let source = tmp.0.join("input/assets/0.png").canonicalize().unwrap();
    let original = fs::read(&source).unwrap();
    for target in [&tmp.0, &tmp.0.join("input"), &tmp.0.join("input/assets")] {
        let transfer =
            TransferSnapshot::new(doc.clone(), resources.clone(), vec![source.clone()]).unwrap();
        assert_eq!(
            transfer
                .publish(TransferFormat::Runtime, target)
                .unwrap_err()
                .code,
            "INVALID_DESTINATION"
        );
        assert_eq!(fs::read(&source).unwrap(), original);
    }
    // A resource captured earlier remains readable even when its external file disappears.
    fs::remove_file(&source).unwrap();
    let output = tmp.0.join("export");
    TransferSnapshot::new(doc.clone(), resources.clone(), vec![source])
        .unwrap()
        .publish(TransferFormat::Runtime, &output)
        .unwrap();
    assert_eq!(
        fs::read(output.join("textures/0.png")).unwrap(),
        resources
            .read_asset(doc.asset(&id(2)).unwrap())
            .unwrap()
            .bytes
    );
}

#[cfg(unix)]
#[test]
fn path_aliases_do_not_bypass_transfer_guard_or_native_save() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let source = tmp.0.join("input/assets/0.png").canonicalize().unwrap();
    let alias = tmp.0.join("alias");
    std::os::unix::fs::symlink(tmp.0.join("input"), &alias).unwrap();
    for target in [&alias, &alias.join("assets")] {
        let result = TransferSnapshot::new(doc.clone(), resources.clone(), vec![source.clone()])
            .unwrap()
            .publish(TransferFormat::Runtime, target);
        assert_eq!(result.unwrap_err().code, "INVALID_DESTINATION");
    }
    let path = tmp.0.join("model.zipzi");
    let receipt = tmp
        .io()
        .save(&doc, &resources, &path, None, || true)
        .unwrap();
    let link = tmp.0.join("model-alias.zipzi");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    assert!(tmp
        .io()
        .save(&doc, &resources, &link, Some(&receipt.fingerprint), || true)
        .is_err());
    assert_eq!(
        tmp.io().fingerprint(&path, || true).unwrap(),
        receipt.fingerprint
    );
}

#[test]
fn missing_or_mismatched_captured_resources_reject_export_before_publication() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let destination = tmp.0.join("output");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"keep").unwrap();
    assert_eq!(
        build_export_plan(&doc, &ResourceView::default())
            .unwrap_err()
            .code,
        "MISSING_RESOURCE"
    );
    let mut asset = doc.asset(&id(2)).unwrap().clone();
    if let AssetContent::Image { width, .. } = &mut asset.content {
        *width = 9;
    }
    let bad = prepare(&doc, |b| b.replace_asset(asset));
    assert!(TransferSnapshot::new(bad, resources, vec![]).is_err());
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"keep");
}

#[test]
fn package_writes_exact_captured_bytes_when_external_source_changes_during_validation() {
    let tmp = TestDirectory::new();
    let (mut doc, resources) = tmp.fixture();
    let mut mesh = doc.mesh(&id(4)).unwrap().clone();
    mesh.raw_blend_mode = Some(262);
    doc = prepare(&doc, |b| b.replace_mesh(mesh));
    let source = tmp.0.join("input/assets/0.png");
    let before = resources
        .read_asset(doc.asset(&id(2)).unwrap())
        .unwrap()
        .bytes;
    let destination = tmp.0.join("package");
    publish_package(
        &doc,
        &PackageOptions {
            resources,
            destination: destination.clone(),
            validate: Some(Box::new(move |_| {
                fs::write(&source, b"changed after verification").unwrap();
                Ok(PackageValidation::structural(
                    RuntimeValidation::NotPerformed,
                ))
            })),
        },
    )
    .unwrap();
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
fn failed_new_project_write_can_retry_without_partial_bytes() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let destination = tmp.0.join("new.zipzi");
    let failing = tmp
        .io()
        .with_filesystem(FailingFileSystem::new(Failure::ShortWrite));
    assert!(failing
        .save(&doc, &resources, &destination, None, || true)
        .is_err());
    assert!(!destination.exists());
    tmp.io()
        .save(&doc, &resources, &destination, None, || true)
        .unwrap();
    assert!(tmp
        .io()
        .load(&destination, || true)
        .unwrap()
        .diagnostics
        .is_empty());
}

#[test]
fn import_reports_truncated_pngs_for_both_entry_points() {
    let tmp = TestDirectory::new();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/external_v50");
    fs::copy(source.join("model.moc3"), tmp.0.join("model.moc3")).unwrap();
    let png = fs::read(source.join("texture_00.png")).unwrap();
    fs::write(tmp.0.join("broken.png"), &png[..png.len() / 2]).unwrap();
    let model = tmp.0.join("model.model3.json");
    fs::write(
        &model,
        r#"{"Version":3,"FileReferences":{"Moc":"model.moc3","Textures":["broken.png"]}}"#,
    )
    .unwrap();
    let importer = ProjectImporter::default();
    for (result, imported, _) in [
        importer.import_model3(&model),
        importer.import_bare_moc3(
            &tmp.0.join("model.moc3"),
            &std::collections::HashMap::from([(0, tmp.0.join("broken.png"))]),
        ),
    ] {
        assert!(!result.resources_complete(), "{result:?}");
        if let Some(imported) = imported {
            assert!(build_export_plan(&imported.document, &imported.resources).is_err());
        }
    }
}

#[test]
fn export_plan_is_inspectable_and_publication_never_reopens_sources() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let plan = build_export_plan(&doc, &resources).unwrap();
    let destination = tmp.0.join("planned-package");
    assert!(!destination.exists());
    for reference in plan.references() {
        assert!(plan.files().contains_key(&reference.from));
        assert!(plan.files().contains_key(&reference.to));
    }
    fs::remove_dir_all(tmp.0.join("input")).unwrap();
    drop(resources);
    drop(doc);
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("old"), b"preserve").unwrap();
    assert_eq!(
        publish_export_plan(&plan, &destination, &|_| Err(zipzi_core::Status::error(
            "PACKAGE_REJECTED",
            "fixture"
        )))
        .unwrap_err()
        .code,
        "PACKAGE_REJECTED"
    );
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
    use zipzi_core::document::MotionClip;
    use zipzi_project::TransferFormat;
    let tmp = TestDirectory::new();
    let (mut doc, resources) = tmp.fixture();
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
    doc = prepare(&doc, |b| {
        b.create_motion(motion.clone())?;
        b.set_motion_roots(vec![zipzi_core::document::MotionRegistration {
            id: id(801),
            clip_id: id(800),
            fade_in: None,
            fade_out: None,
            sound: None,
            extensions: Default::default(),
        }])
    });
    let output = tmp.0.join("transfer-runtime");
    let publish = |doc: &DocumentState| {
        TransferSnapshot::new(doc.clone(), resources.clone(), vec![])
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
    publish(&doc).unwrap();
    let original = motion_path();
    let mut renamed = motion;
    renamed.name = "Renamed motion".into();
    doc = prepare(&doc, |b| b.replace_motion(renamed));
    publish(&doc).unwrap();
    let renamed = motion_path();
    assert!(renamed.is_file());
    assert!(!original.exists());
    doc = prepare(&doc, |b| {
        b.set_motion_roots(vec![])?;
        b.remove_motion(&id(800))
    });
    // A user-created neighbor in the former managed directory must still block replacement.
    let notes = output.join("motions/notes.txt");
    fs::write(&notes, b"keep").unwrap();
    assert_eq!(publish(&doc).unwrap_err().code, "UNMANAGED_OUTPUT");
    assert_eq!(fs::read(&notes).unwrap(), b"keep");
    assert!(renamed.is_file());
    fs::remove_file(notes).unwrap();
    let saved_motion = fs::read(&renamed).unwrap();
    fs::write(&renamed, b"independently edited motion").unwrap();
    assert_eq!(publish(&doc).unwrap_err().code, "UNMANAGED_OUTPUT");
    assert_eq!(fs::read(&renamed).unwrap(), b"independently edited motion");
    fs::write(&renamed, saved_motion).unwrap();
    #[cfg(unix)]
    {
        let outside = tmp.0.join("keep.txt");
        fs::write(&outside, b"keep outside").unwrap();
        let alias = output.join("motions/alias");
        std::os::unix::fs::symlink(&outside, &alias).unwrap();
        assert_eq!(publish(&doc).unwrap_err().code, "UNMANAGED_OUTPUT");
        assert_eq!(fs::read(&outside).unwrap(), b"keep outside");
        fs::remove_file(alias).unwrap();
    }
    publish(&doc).unwrap();
    assert!(!renamed.exists());
    assert!(!output.join("motions").exists());
    publish(&doc).unwrap();
}
