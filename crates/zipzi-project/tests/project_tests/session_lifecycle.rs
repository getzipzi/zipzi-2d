use super::helpers::*;
use std::{collections::HashMap, fs, path::Path};
use zipzi_core::{
    evaluation::{evaluate_frame, DrawableFrame},
    state::*,
    *,
};
use zipzi_moc3::encode_moc3;
use zipzi_project::{ProjectImporter, ResourceView};

fn rename(doc: &DocumentState, name: &str) -> DocumentState {
    let mut mesh = doc.mesh(&id(4)).unwrap().clone();
    mesh.name = name.into();
    prepare(doc, |b| b.replace_mesh(mesh))
}
fn code<T: std::fmt::Debug>(r: zipzi_project::archive::Result<T>) -> String {
    Status::from(r.unwrap_err()).code
}

#[test]
fn native_save_deduplicates_resources_and_preserves_source_snapshot() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let initial = doc.content_id();
    let path = tmp.0.join("project.zipzi");
    let saved = tmp
        .io()
        .save(&doc, &resources, &path, None, || true)
        .unwrap();
    assert!(saved.published && saved.durable);
    assert_eq!(doc.content_id(), initial);
    let loaded = tmp.io().load(&path, || true).unwrap();
    assert!(loaded.diagnostics.is_empty());
    assert_eq!(loaded.stats.unique_assets, 1);
    assert_eq!(loaded.resources.len(), 1);
    assert_eq!(
        project_json(&fs::read(&path).unwrap()),
        project_json(&crate::support::native_bytes_with_resources(
            &loaded.document,
            &loaded.resources
        ))
    );
    let edited = rename(&loaded.document, "RenamedQuad");
    let updated = tmp
        .io()
        .save(
            &edited,
            &loaded.resources,
            &path,
            Some(&saved.fingerprint),
            || true,
        )
        .unwrap();
    assert_ne!(updated.fingerprint, saved.fingerprint);
    assert_eq!(
        tmp.io()
            .load(&path, || true)
            .unwrap()
            .document
            .mesh(&id(4))
            .unwrap()
            .name,
        "RenamedQuad"
    );
    assert_eq!(doc.mesh(&id(4)).unwrap().name, "Quad");
}

#[test]
fn published_format_preserves_cpp_transform_tags_and_omits_legacy_fields() {
    let mut doc = fixture_doc();
    doc = prepare(&doc, |builder| {
        builder.create_transform(Transform {
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
    });
    let text = fixture_bytes(&doc);
    let wire: serde_json::Value = project_json(&text);
    let tags = wire["document"]["transforms"].as_array().unwrap();
    assert_eq!(tags.iter().find(|t| t["id"] == sid(2)).unwrap()["kind"], 1);
    assert_eq!(tags.iter().find(|t| t["id"] == sid(3)).unwrap()["kind"], 0);
    let decoded = load_native(&text).unwrap();
    for t in wire["document"]["transforms"].as_array().unwrap() {
        let expected = if t["kind"] == 0 {
            TransformKind::Warp
        } else {
            TransformKind::Rotation
        };
        assert_eq!(
            decoded.transform(t["id"].as_str().unwrap()).unwrap().kind(),
            expected
        );
    }
    let encoded = fixture_bytes(&doc);
    let output: serde_json::Value = project_json(&encoded);
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
    assert_eq!(
        project_json(&resaved_native(&encoded)),
        project_json(&fixture_bytes(&doc))
    );
}

#[test]
fn bound_transform_type_and_grid_changes_fail_freeze_without_mutating_source() {
    let mut doc = fixture_doc();
    scene_binding(&mut doc);
    let before = doc.content_id();
    let mut transform = doc.transform(&sid(2)).unwrap().clone();
    transform.data = TransformData::Warp(WarpTransform {
        rows: 1,
        columns: 1,
        quad: true,
        points: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(0., 1.),
            Vec2::new(1., 1.),
        ],
        bezier: None,
    });
    let mut b = doc.builder();
    b.replace_transform(transform.clone()).unwrap();
    assert!(b.freeze().is_err());
    assert_eq!(doc.content_id(), before);
    assert!(evaluate_frame(&doc, &HashMap::new(), &mut DrawableFrame::default()).is_ok());
    doc = prepare(&doc, |b| {
        b.remove_scene_binding(&sid(99))?;
        b.replace_transform(transform.clone())
    });
    scene_binding(&mut doc);
    let before = doc.content_id();
    transform.warp_mut().unwrap().rows = 2;
    transform
        .warp_mut()
        .unwrap()
        .points
        .extend([Vec2::new(0., 2.), Vec2::new(1., 2.)]);
    let mut b = doc.builder();
    b.replace_transform(transform).unwrap();
    assert!(b.freeze().is_err());
    assert_eq!(doc.content_id(), before);
}

#[test]
fn parameter_range_changes_validate_mesh_and_scene_bindings() {
    for scene_only in [false, true] {
        let mut doc = fixture_doc();
        if scene_only {
            doc = prepare(&doc, |b| b.remove_binding(&id(7)));
            scene_binding(&mut doc);
        }
        let before = doc.content_id();
        let mut p = doc.parameter(&id(6)).unwrap().clone();
        p.minimum = -0.5;
        p.maximum = 0.5;
        let mut b = doc.builder();
        b.replace_parameter(p.clone()).unwrap();
        assert!(b.freeze().is_err());
        assert_eq!(doc.content_id(), before);
        p.minimum = -2.;
        p.maximum = 2.;
        doc = prepare(&doc, |b| b.replace_parameter(p));
        let encoded = fixture_bytes(&doc);
        assert_eq!(
            project_json(&encoded),
            project_json(&resaved_native(&encoded))
        );
    }
}

#[test]
fn save_as_existing_and_directory_targets_preserve_destination() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let path = tmp.0.join("existing.zipzi");
    tmp.io()
        .save(&doc, &resources, &path, None, || true)
        .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        code(
            tmp.io()
                .save(&rename(&doc, "Other"), &resources, &path, None, || true)
        ),
        "PROJECT_EXISTS"
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let directory = tmp.0.join("directory.zipzi");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("keep"), b"keep").unwrap();
    assert_eq!(
        code(tmp.io().save(&doc, &resources, &directory, None, || true)),
        "UNSUPPORTED_PROJECT_PATH"
    );
    assert_eq!(fs::read(directory.join("keep")).unwrap(), b"keep");
}

#[test]
fn concurrent_writers_cannot_both_publish_the_same_baseline() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let path = tmp.0.join("project.zipzi");
    let saved = tmp
        .io()
        .save(&doc, &resources, &path, None, || true)
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|i| {
            let candidate = rename(&doc, &format!("writer-{i}"));
            let barrier = barrier.clone();
            let path = path.clone();
            let fingerprint = saved.fingerprint.clone();
            let resources = resources.clone();
            let io = tmp.io();
            std::thread::spawn(move || {
                barrier.wait();
                io.save(&candidate, &resources, &path, Some(&fingerprint), || true)
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    for result in results {
        if let Err(e) = result {
            assert!(["PROJECT_BUSY", "PROJECT_CONFLICT"].contains(&Status::from(e).code.as_str()));
        }
    }
    assert_eq!(
        code(
            tmp.io()
                .save(&doc, &resources, &path, Some(&saved.fingerprint), || true)
        ),
        "PROJECT_CONFLICT"
    );
}

#[test]
fn prepublication_faults_preserve_bytes_and_allow_retry() {
    for fault in [
        Failure::ShortWrite,
        Failure::FileSync,
        Failure::Rename(vec![1]),
    ] {
        let tmp = TestDirectory::new();
        let (doc, resources) = tmp.fixture();
        let path = tmp.0.join("project.zipzi");
        let saved = tmp
            .io()
            .save(&doc, &resources, &path, None, || true)
            .unwrap();
        let bytes = fs::read(&path).unwrap();
        let edited = rename(&doc, "Edited");
        let identity = edited.content_id();
        let failing = tmp.io().with_filesystem(FailingFileSystem::new(fault));
        assert_eq!(
            code(
                failing.save(&edited, &resources, &path, Some(&saved.fingerprint), || {
                    true
                })
            ),
            "PROJECT_IO"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(edited.content_id(), identity);
        assert!(!fs::read_dir(&tmp.0).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".zipzi-stage-")));
        tmp.io()
            .save(&edited, &resources, &path, Some(&saved.fingerprint), || {
                true
            })
            .unwrap();
        assert_eq!(
            tmp.io()
                .load(&path, || true)
                .unwrap()
                .document
                .mesh(&id(4))
                .unwrap()
                .name,
            "Edited"
        );
    }
}

#[test]
fn postpublication_sync_warning_returns_new_baseline() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let path = tmp.0.join("project.zipzi");
    let saved = tmp
        .io()
        .save(&doc, &resources, &path, None, || true)
        .unwrap();
    let io = tmp
        .io()
        .with_filesystem(FailingFileSystem::new(Failure::DirectorySync(1)));
    let receipt = io
        .save(
            &rename(&doc, "Published"),
            &resources,
            &path,
            Some(&saved.fingerprint),
            || true,
        )
        .unwrap();
    assert!(receipt.published);
    assert!(!receipt.durable);
    assert!(!receipt.warnings.is_empty());
    assert_eq!(
        tmp.io()
            .load(&path, || true)
            .unwrap()
            .document
            .mesh(&id(4))
            .unwrap()
            .name,
        "Published"
    );
    io.save(&doc, &resources, &path, Some(&receipt.fingerprint), || true)
        .unwrap();
}

#[test]
fn save_rechecks_baseline_after_staging() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let path = tmp.0.join("project.zipzi");
    let saved = tmp
        .io()
        .save(&doc, &resources, &path, None, || true)
        .unwrap();
    let io = tmp
        .io()
        .with_filesystem(FailingFileSystem::new(Failure::ExternalChange(
            path.clone(),
        )));
    assert_eq!(
        code(io.save(&doc, &resources, &path, Some(&saved.fingerprint), || true)),
        "PROJECT_CONFLICT"
    );
    assert_eq!(fs::read(&path).unwrap(), b"external change");
}

#[test]
fn confirmed_replacement_uses_approved_fingerprint_and_retries_failed_publish() {
    let tmp = TestDirectory::new();
    let (doc, resources) = tmp.fixture();
    let path = tmp.0.join("project.zipzi");
    let saved = tmp
        .io()
        .save(&doc, &resources, &path, None, || true)
        .unwrap();
    let bytes = fs::read(&path).unwrap();
    let replacement = DocumentState::new(id(999), doc.setup()).unwrap();
    let io = tmp
        .io()
        .with_filesystem(FailingFileSystem::new(Failure::Rename(vec![1])));
    assert_eq!(
        code(io.save(
            &replacement,
            &ResourceView::default(),
            &path,
            Some(&saved.fingerprint),
            || true
        )),
        "PROJECT_IO"
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let receipt = io
        .save(
            &replacement,
            &ResourceView::default(),
            &path,
            Some(&saved.fingerprint),
            || true,
        )
        .unwrap();
    assert!(receipt.published);
    assert_eq!(
        tmp.io().load(&path, || true).unwrap().document.id(),
        id(999)
    );
}

#[test]
fn import_rejects_cycles_without_returning_partial_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read(root.join("fixtures/external_v50/model.moc3")).unwrap();
    let offsets = zipzi_moc3::inspect_moc3(&source).unwrap().section_offsets;
    let tmp = TestDirectory::new();
    let path = tmp.0.join("bad.moc3");
    for (section, parent) in [(9, 0i32), (16, 0), (16, 1)] {
        let mut bytes = source.clone();
        let offset = offsets[section] as usize;
        bytes[offset..offset + 4].copy_from_slice(&parent.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        let (result, imported, report) =
            ProjectImporter::default().import_bare_moc3(&path, &HashMap::new());
        assert_eq!(result.status.code, "RELATIONSHIP_CYCLE");
        assert!(imported.is_none());
        assert!(report.is_none());
    }
}

#[test]
fn imported_canvas_and_drawing_groups_survive_source_removal_and_native_roundtrip() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tmp = TestDirectory::new();
    let path = tmp.0.join("original.moc3");
    fs::copy(
        root.join("third_party/purism-core/testdata/moc3/3d8e869a678a1dac.moc3"),
        &path,
    )
    .unwrap();
    let texture = tmp.0.join("original.png");
    create_test_png(&texture);
    let (result, imported, _) =
        ProjectImporter::default().import_bare_moc3(&path, &HashMap::from([(0, texture.clone())]));
    assert!(result.status.is_ok(), "{result:?}");
    let imported = imported.unwrap();
    let doc = &imported.document;
    assert_eq!(doc.setup().source_canvas.unwrap().flag, 0);
    assert_eq!(doc.draw_order_groups().as_ref().unwrap().len(), 1);
    assert!(!doc.part_order().is_empty());
    let mut before = DrawableFrame::default();
    assert!(evaluate_frame(doc, &HashMap::new(), &mut before).is_ok());
    let moc = encode_moc3(doc).unwrap().bytes;
    let destination = tmp.0.join("saved.zipzi");
    tmp.io()
        .save(doc, &imported.resources, &destination, None, || true)
        .unwrap();
    fs::remove_file(&path).unwrap();
    fs::remove_file(&texture).unwrap();
    let reopened = tmp.io().load(&destination, || true).unwrap();
    assert!(reopened.diagnostics.is_empty());
    assert_eq!(
        reopened.document.draw_order_groups(),
        doc.draw_order_groups()
    );
    assert_eq!(reopened.document.setup().source_canvas.unwrap().flag, 0);
    let mut after = DrawableFrame::default();
    assert!(evaluate_frame(&reopened.document, &HashMap::new(), &mut after).is_ok());
    assert_eq!(before.drawables, after.drawables);
    assert_eq!(moc, encode_moc3(&reopened.document).unwrap().bytes);
    let mut groups = doc.draw_order_groups().clone().unwrap();
    groups[0].items.swap(0, 1);
    let edited = prepare(doc, |b| b.set_draw_order_groups(Some(groups.clone())));
    assert_ne!(edited.content_id(), doc.content_id());
    let duplicate = groups[0].items[0].clone();
    groups[0].items.push(duplicate);
    let mut b = edited.builder();
    b.set_draw_order_groups(Some(groups)).unwrap();
    assert!(b.freeze().is_err());
}
