use std::collections::HashMap;
use std::fs;
use std::path::Path;
use zipzi_core::{TransformData, WarpTransform};

use super::helpers::*;
use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};
use zipzi_core::types::{Transform, TransformKind, Vec2};
use zipzi_moc3::encode_moc3;
use zipzi_project::{
    content_sha256, decode_project, encode_project, DocumentSession, DocumentStore,
};

#[test]
fn test_document_session_lifecycle() {
    let base = std::env::temp_dir().join(format!("zipzi-session-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let assets_dir = base.join("source_assets");
    let (_, sha1) = create_test_png(&assets_dir.join("assets/0.png"));
    let (_, sha2) = create_test_png(&assets_dir.join("assets/1.png"));

    let mut doc = fixture_doc(&sha1, &sha2);
    // Update asset sources to relative to source_assets
    let mut a1 = doc.get_asset(&id(2)).unwrap().clone();
    a1.content.as_mut().unwrap().source = "assets/0.png".to_string();
    doc.replace_asset(a1);
    let mut a2 = doc.get_asset(&id(3)).unwrap().clone();
    a2.content.as_mut().unwrap().source = "assets/1.png".to_string();
    doc.replace_asset(a2);

    // Save project
    let project_dir = base.join("project");
    let store = DocumentStore::new();
    let (res, snapshot) = store.save(&doc, &assets_dir, &project_dir, None);
    assert!(res.status.is_ok(), "Save failed: {:?}", res.status);
    let snapshot = snapshot.unwrap();
    assert!(snapshot.manifest.exists());
    let short_source = format!("assets/{}.png", &sha1[..16]);
    assert_eq!(
        snapshot
            .document
            .get_asset(&id(2))
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .source,
        short_source
    );
    assert_eq!(
        snapshot
            .document
            .get_asset(&id(3))
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .source,
        short_source
    );
    assert_eq!(fs::read_dir(project_dir.join("assets")).unwrap().count(), 1);

    // Open project in session
    let mut session = DocumentSession::new();
    let open_res = session.open(&snapshot.manifest);
    assert!(
        open_res.status.is_ok(),
        "Open failed: {:?}",
        open_res.status
    );
    assert!(session.document().same_content(&snapshot.document));

    // Edit and save
    assert!(session
        .document_mut()
        .rename_mesh(&id(4), "RenamedQuad".to_string())
        .status
        .is_ok());
    assert!(session.document().modified());

    let save_res = session.save(&snapshot.manifest);
    assert!(save_res.status.is_ok());
    assert!(!session.document().modified());
    assert_eq!(
        session
            .document()
            .get_asset(&id(2))
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .source,
        short_source
    );

    // Second session attempting concurrent overwrite with stale sha
    let (conf_res, _) = store.save(
        &doc,
        &project_dir.join("assets"),
        &snapshot.manifest,
        Some("stale_hash_that_does_not_match_current"),
    );
    assert_eq!(conf_res.status.code, "PROJECT_CONFLICT");

    // Package publication
    let package_dir = base.join("published_package");
    let pub_res = session.export_package(&package_dir);
    assert!(
        pub_res.status.is_ok(),
        "export_package failed: {:?}",
        pub_res
    );
    assert!(package_dir.join("model.moc3").exists());
    assert!(package_dir.join("model.model3.json").exists());
    assert!(package_dir.join("textures/0.png").exists());

    let _ = fs::remove_dir_all(base);
}

// Regression coverage for the C++ -> Rust persistence and editing contracts.

#[test]
fn published_format_preserves_cpp_transform_tags_and_omits_legacy_fields() {
    let mut doc = fixture_doc(&"0".repeat(64), &"0".repeat(64));
    assert!(doc
        .create_transform(Transform {
            id: sid(3),
            runtime_id: "Warp".into(),
            name: "Warp".into(),
            part_id: zipzi_core::PartId::optional(sid(1)),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0),
                    Vec2::new(1.0, 1.0),
                ],
            }),
            ..Default::default()
        })
        .status
        .is_ok());
    let text = encode_project(&doc).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&text).unwrap();
    let tags = wire["document"]["transforms"].as_array().unwrap();
    assert_eq!(tags.iter().find(|t| t["id"] == sid(2)).unwrap()["kind"], 1);
    assert_eq!(tags.iter().find(|t| t["id"] == sid(3)).unwrap()["kind"], 0);
    let doc = decode_project(&text).unwrap();
    for t in wire["document"]["transforms"].as_array().unwrap() {
        let expected = if t["kind"] == 0 {
            TransformKind::Warp
        } else {
            TransformKind::Rotation
        };
        assert_eq!(
            doc.get_transform(t["id"].as_str().unwrap()).unwrap().kind(),
            expected
        );
    }
    let encoded = encode_project(&doc).unwrap();
    let output: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    let tags = |value: &serde_json::Value| {
        value["document"]["transforms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| (t["id"].clone(), t["kind"].clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(tags(&wire), tags(&output));
    for key in ["deformers", "deformation_links", "organization_links"] {
        assert!(output["document"].get(key).is_none());
    }
    assert!(decode_project(&encoded).unwrap().same_content(&doc));
}

#[test]
fn bound_transform_type_and_grid_changes_are_atomic_failures() {
    let mut doc = fixture_doc(&"0".repeat(64), &"0".repeat(64));
    scene_binding(&mut doc);
    doc.mark_saved();
    let before = doc.clone();
    let mut transform = doc.get_transform(&sid(2)).unwrap().clone();
    transform.data = TransformData::Warp(WarpTransform {
        bezier: None,
        rows: 1,
        columns: 1,
        quad: true,
        points: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
        ],
    });
    assert_eq!(
        doc.replace_transform(transform.clone()).status.code,
        "KEYFORMS_REQUIRED"
    );
    assert!(doc.same_content(&before));
    assert_eq!(doc.revision(), before.revision());
    assert!(!doc.modified());
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut DrawableFrame::default()).is_ok());
    assert!(doc.erase_object(&sid(99)).status.is_ok());
    assert!(doc.replace_transform(transform.clone()).status.is_ok());
    scene_binding(&mut doc);
    let before = doc.clone();
    transform.warp_mut().unwrap().rows = 2;
    transform
        .warp_mut()
        .unwrap()
        .points
        .extend([Vec2::new(0.0, 2.0), Vec2::new(1.0, 2.0)]);
    assert_eq!(
        doc.replace_transform(transform).status.code,
        "KEYFORMS_REQUIRED"
    );
    assert!(doc.same_content(&before));
    assert_eq!(doc.revision(), before.revision());
}

#[test]
fn parameter_range_changes_validate_mesh_and_scene_bindings() {
    for scene_only in [false, true] {
        let mut doc = fixture_doc(&"0".repeat(64), &"0".repeat(64));
        if scene_only {
            assert!(doc.erase_object(&id(7)).status.is_ok());
            scene_binding(&mut doc);
        }
        doc.mark_saved();
        let before = doc.clone();
        let mut parameter = doc.get_parameter(&id(6)).unwrap().clone();
        parameter.minimum = -0.5;
        parameter.maximum = 0.5;
        assert_eq!(
            doc.replace_parameter(parameter.clone()).status.code,
            "INVALID_KEYS"
        );
        assert!(doc.same_content(&before));
        assert_eq!(doc.revision(), before.revision());
        assert!(!doc.modified());
        parameter.minimum = -2.0;
        parameter.maximum = 2.0;
        let edit = doc.replace_parameter(parameter);
        assert!(edit.status.is_ok());
        assert!(edit.changes.mesh_ids.contains(&id(4)));
        assert!(decode_project(&encode_project(&doc).unwrap())
            .unwrap()
            .same_content(&doc));
    }
}

#[test]
fn save_as_refuses_existing_project_and_preserves_session() {
    let tmp = TestDirectory::new();
    let original = tmp.saved_session();
    let manifest = original.manifest();
    let before = fs::read(manifest).unwrap();
    let mut other = DocumentSession::new();
    assert!(other
        .document_mut()
        .initialize(id(999), original.document().source_canvas().unwrap())
        .is_ok());
    let result = other.save(manifest);
    assert_eq!(result.status.code, "DESTINATION_EXISTS");
    assert!(!result.published);
    assert_eq!(before, fs::read(manifest).unwrap());
    assert!(other.manifest().as_os_str().is_empty());
    assert!(other.document().modified());
}

#[test]
fn concurrent_writers_cannot_both_publish_the_same_baseline() {
    let tmp = TestDirectory::new();
    let session = tmp.saved_session();
    let manifest = session.manifest().to_path_buf();
    let hash = content_sha256(&fs::read(&manifest).unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let mut doc = session.document().clone();
            assert!(doc
                .rename_mesh(&id(4), format!("writer-{i}"))
                .status
                .is_ok());
            let barrier = barrier.clone();
            let manifest = manifest.clone();
            let hash = hash.clone();
            let root = session.root();
            std::thread::spawn(move || {
                barrier.wait();
                DocumentStore::new()
                    .save(&doc, &root, &manifest, Some(&hash))
                    .0
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.status.is_ok()).count(), 1);
    assert!(results.iter().all(|r| r.status.is_ok()
        || ["PROJECT_BUSY", "PROJECT_CONFLICT"].contains(&r.status.code.as_str())));
    let mut stale = session;
    assert_eq!(stale.save(&manifest).status.code, "PROJECT_CONFLICT");
}

#[test]
fn save_repairs_corrupt_asset_name_without_overwriting_old_bytes() {
    let tmp = TestDirectory::new();
    let (doc, input) = tmp.fixture();
    let destination = tmp.0.join("destination");
    let sha = &doc
        .get_asset(&id(2))
        .unwrap()
        .content
        .as_ref()
        .unwrap()
        .sha256;
    fs::create_dir_all(destination.join("assets")).unwrap();
    let corrupted = destination.join(format!("assets/{}.png", &sha[..16]));
    fs::write(&corrupted, b"partial old file").unwrap();
    let (result, snapshot) = DocumentStore::new().save(&doc, &input, &destination, None);
    assert!(result.status.is_ok(), "{result:?}");
    assert_eq!(fs::read(corrupted).unwrap(), b"partial old file");
    let snapshot = snapshot.unwrap();
    assert_eq!(
        snapshot
            .document
            .get_asset(&id(2))
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .source,
        format!("assets/{}.png", &sha[..24])
    );
    assert!(DocumentStore::new()
        .open(&destination)
        .0
        .resources_complete());
}

#[test]
fn precommit_failures_preserve_manifest_document_revision_and_saved_baseline() {
    for fault in [
        Failure::ShortWrite,
        Failure::FileSync,
        Failure::DirectorySync(1),
        Failure::DirectorySync(2),
        Failure::Rename(vec![1]),
    ] {
        let tmp = TestDirectory::new();
        let initial = tmp.saved_session();
        let path = initial.manifest().to_path_buf();
        let original_bytes = fs::read(&path).unwrap();
        let mut session = DocumentSession::with_filesystem(FailingFileSystem::new(fault));
        assert!(session.open(&path).status.is_ok());
        assert!(session
            .document_mut()
            .rename_mesh(&id(4), "Edited".into())
            .status
            .is_ok());
        let before = session.document().clone();
        let result = session.save(&path);
        assert_eq!(result.status.code, "PROJECT_IO", "{result:?}");
        assert!(!result.published);
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        assert!(session.document().same_content(&before));
        assert_eq!(session.document().revision(), before.revision());
        assert!(session.document().modified());
        assert!(!fs::read_dir(session.root()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".zipzi-stage-")));
    }
}

#[test]
fn postcommit_sync_failure_reports_publication_and_advances_baseline() {
    let tmp = TestDirectory::new();
    let initial = tmp.saved_session();
    let path = initial.manifest().to_path_buf();
    let mut session =
        DocumentSession::with_filesystem(FailingFileSystem::new(Failure::DirectorySync(3)));
    assert!(session.open(&path).status.is_ok());
    assert!(session
        .document_mut()
        .rename_mesh(&id(4), "Published".into())
        .status
        .is_ok());
    let result = session.save(&path);
    assert!(result.status.is_ok());
    assert!(result.published);
    assert!(!result.durable);
    assert!(!result.warnings.is_empty());
    assert!(!session.document().modified());
    assert_eq!(
        decode_project(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .get_mesh(&id(4))
            .unwrap()
            .name,
        "Published"
    );
    // This would conflict if a published result failed to update the session baseline.
    assert!(session.save(&path).status.is_ok());
}

#[test]
fn save_rechecks_baseline_after_staging() {
    let tmp = TestDirectory::new();
    let initial = tmp.saved_session();
    let path = initial.manifest().to_path_buf();
    let mut session = DocumentSession::with_filesystem(FailingFileSystem::new(
        Failure::ExternalChange(path.clone()),
    ));
    assert!(session.open(&path).status.is_ok());
    let before = session.document().clone();
    let result = session.save(&path);
    assert_eq!(result.status.code, "PROJECT_CONFLICT");
    assert_eq!(fs::read(&path).unwrap(), b"external change");
    assert!(session.document().same_content(&before));
    assert_eq!(session.document().revision(), before.revision());
}

#[test]
fn test_project_detachment_lifecycle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let src_fixture = root.join("fixtures/external_v50");

    let tmp = TestDirectory::new();
    let source_dir = tmp.0.join("external_source");
    fs::create_dir_all(&source_dir).unwrap();

    // 1. Copy fixture into source_dir
    let src_model3 = source_dir.join("model.model3.json");
    fs::copy(src_fixture.join("model.model3.json"), &src_model3).unwrap();
    fs::copy(
        src_fixture.join("model.moc3"),
        source_dir.join("model.moc3"),
    )
    .unwrap();
    fs::copy(
        src_fixture.join("texture_00.png"),
        source_dir.join("texture_00.png"),
    )
    .unwrap();

    // 2. Import into a session
    let mut session = DocumentSession::new();
    let (imp_res, report) = session.import_model3(&src_model3);
    assert!(imp_res.status.is_ok(), "{:?}", imp_res);
    assert!(report.is_some());
    let rep = report.unwrap();
    assert_eq!(rep.moc_version, 5);

    // 3. Save as project.zipzi
    let project_path = tmp.0.join("detached_project");
    let save_res = session.save(&project_path);
    assert!(save_res.status.is_ok(), "{:?}", save_res);

    // 4. Detachment: completely remove source_dir
    fs::remove_dir_all(&source_dir).unwrap();
    assert!(!source_dir.exists());

    // 5. Reopen the project from project_path without source files
    let mut reopened_session = DocumentSession::new();
    let open_res = reopened_session.open(&project_path);
    assert!(open_res.status.is_ok(), "{:?}", open_res);
    assert!(open_res.resources_complete());

    // Verify document structure intact
    let doc = reopened_session.document();
    assert!(!doc.mesh_order().is_empty());
    assert!(!doc.parameter_order().is_empty());
    assert!(!doc.asset_order().is_empty());

    // 6. Edit the reopened project
    let param_id = doc.parameter_order()[0].clone();
    let mut p = doc.get_parameter(&param_id).unwrap().clone();
    p.default_value = (p.minimum + p.maximum) / 2.0;
    let edit_res = reopened_session.document_mut().replace_parameter(p);
    assert!(edit_res.status.is_ok(), "{:?}", edit_res);

    // 7. Re-export MOC3 package from reopened session
    let export_dir = tmp.0.join("re_exported_package");
    assert_eq!(
        reopened_session.export_package(&export_dir).status.code,
        "MISSING_PACKAGE_ATTACHMENT"
    );
    assert!(reopened_session
        .document_mut()
        .set_missing_attachments(Vec::new())
        .status
        .is_ok());
    let exp_res = reopened_session.export_package(&export_dir);
    assert!(exp_res.status.is_ok(), "{:?}", exp_res);

    let exported_moc3 = export_dir.join("model.moc3");
    assert!(exported_moc3.exists());
    let moc3_bytes = fs::read(&exported_moc3).unwrap();
    let inspection = zipzi_moc3::inspect_moc3(&moc3_bytes).expect("Exported MOC3 inspect failed");
    assert_eq!(inspection.version, zipzi_moc3::Moc3Version::Version50);

    let exported_tex = export_dir.join("textures/0.png");
    assert!(exported_tex.exists());
    let tex_bytes = fs::read(&exported_tex).unwrap();
    assert!(!tex_bytes.is_empty());
}

#[test]
fn import_rejects_cycles_without_replacing_session() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let source = fs::read(root.join("fixtures/external_v50/model.moc3")).unwrap();
    let offsets = zipzi_moc3::inspect_moc3(&source).unwrap().section_offsets;
    let tmp = TestDirectory::new();
    let path = tmp.0.join("bad.moc3");
    let mut session = DocumentSession::new();
    let (result, _) = session.import_model3(&root.join("fixtures/external_v50/model.model3.json"));
    assert!(result.status.is_ok());
    let baseline = encode_project(session.document()).unwrap();
    let revision = session.document().revision();
    for (section, parent) in [(9, 0i32), (16, 0), (16, 1)] {
        let mut bytes = source.clone();
        let offset = offsets[section] as usize;
        bytes[offset..offset + 4].copy_from_slice(&parent.to_le_bytes());
        fs::write(&path, &bytes).unwrap();
        let (result, report) = session.import_bare_moc3(&path, &std::collections::HashMap::new());
        assert_eq!(result.status.code, "RELATIONSHIP_CYCLE");
        assert!(report.is_none());
        assert_eq!(encode_project(session.document()).unwrap(), baseline);
        assert_eq!(session.document().revision(), revision);
    }
}

#[test]
fn imported_canvas_and_drawing_groups_survive_save_reopen() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let tmp = TestDirectory::new();
    let path = tmp.0.join("original.moc3");
    fs::copy(
        root.join("third_party/purism-core/testdata/moc3/3d8e869a678a1dac.moc3"),
        &path,
    )
    .unwrap();
    let texture = tmp.0.join("original.png");
    create_test_png(&texture);
    let mut session = DocumentSession::new();
    let (result, _) = session.import_bare_moc3(&path, &HashMap::from([(0, texture.clone())]));
    assert!(result.status.is_ok());
    assert_eq!(session.document().source_canvas().unwrap().flag, 0);
    assert_eq!(session.document().draw_order_groups().unwrap().len(), 1);
    assert!(!session.document().part_order().is_empty());
    let mut before = DrawableFrame::default();
    assert!(evaluate_frame(session.document(), &HashMap::new(), &mut before).is_ok());
    let before_bytes = encode_moc3(session.document()).unwrap().bytes;
    let destination = tmp.0.join("saved");
    assert!(session.save(&destination).status.is_ok());
    fs::remove_file(&path).unwrap();
    fs::remove_file(&texture).unwrap();
    let mut reopened = DocumentSession::new();
    assert!(reopened.open(&destination).resources_complete());
    assert_eq!(
        reopened.document().draw_order_groups(),
        session.document().draw_order_groups()
    );
    assert_eq!(reopened.document().source_canvas().unwrap().flag, 0);
    assert!(!reopened.document().modified());
    let mut after = DrawableFrame::default();
    assert!(evaluate_frame(reopened.document(), &HashMap::new(), &mut after).is_ok());
    assert_eq!(before.drawables, after.drawables);
    assert_eq!(
        before_bytes,
        encode_moc3(reopened.document()).unwrap().bytes
    );

    // Drawing-group edits participate in saved-content tracking and validate atomically.
    let mut groups = reopened.document().draw_order_groups().unwrap().to_vec();
    groups[0].items.swap(0, 1);
    assert!(reopened
        .document_mut()
        .replace_draw_order_groups(groups.clone())
        .status
        .is_ok());
    assert!(reopened.document().modified());
    let baseline = encode_project(reopened.document()).unwrap();
    let duplicate = groups[0].items[0].clone();
    groups[0].items.push(duplicate);
    assert!(!reopened
        .document_mut()
        .replace_draw_order_groups(groups)
        .status
        .is_ok());
    assert_eq!(baseline, encode_project(reopened.document()).unwrap());
}

#[test]
fn test_project_failure_preserves_document() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let mut doc = fixture_doc(sha1, sha2);
    let snap = doc.clone();

    // Attempt invalid parameter edit
    let mut bad_p = doc
        .get_parameter(&doc.parameter_order()[0])
        .unwrap()
        .clone();
    bad_p.maximum = bad_p.minimum - 1.0; // invalid range
    let res = doc.replace_parameter(bad_p);
    assert!(!res.status.is_ok());
    assert_eq!(res.status.code, "INVALID_PARAMETER");

    // Document state remains completely unaltered
    assert!(doc.same_content(&snap));
}

#[test]
fn confirmed_replacement_preserves_destination_on_io_failure_and_can_retry() {
    let tmp = TestDirectory::new();
    let original = tmp.saved_session();
    let destination = original.manifest().to_path_buf();
    let old_bytes = fs::read(&destination).unwrap();
    let approved_hash = content_sha256(&old_bytes);
    let unrelated = original.root().join("notes.txt");
    fs::write(&unrelated, b"keep me").unwrap();

    let mut replacement =
        DocumentSession::with_filesystem(FailingFileSystem::new(Failure::Rename(vec![1])));
    let setup = original.document().source_canvas().unwrap();
    assert!(replacement
        .document_mut()
        .initialize(id(999), setup)
        .is_ok());
    let before = replacement.document().clone();
    let result = replacement.save_replacing(&destination, &approved_hash);
    assert_eq!(result.status.code, "PROJECT_IO");
    assert!(!result.published);
    assert!(replacement.manifest().as_os_str().is_empty());
    assert!(replacement.document().same_content(&before));
    assert!(replacement.document().modified());
    assert_eq!(fs::read(&destination).unwrap(), old_bytes);
    assert!(DocumentStore::new()
        .open(&destination)
        .0
        .resources_complete());

    let result = replacement.save_replacing(&destination, &approved_hash);
    assert!(result.status.is_ok(), "{result:?}");
    assert!(!replacement.document().modified());
    assert_eq!(fs::read(&unrelated).unwrap(), b"keep me");
    assert_eq!(
        decode_project(&fs::read_to_string(&destination).unwrap())
            .unwrap()
            .id(),
        id(999)
    );
    assert!(replacement.save(&destination).status.is_ok());
}
