use super::*;
use crate::{ResourceStore, ResourceView};
use zipzi_core::state::{AssetContent, DocumentState};

fn json(path: &Path) -> serde_json::Value {
    let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
    serde_json::from_reader(zip.by_name("project.json").unwrap()).unwrap()
}

#[test]
fn every_persistent_domain_roundtrips_without_an_old_document() {
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let state = DocumentState::from_owned(super::state_fixture::owned()).unwrap();
    let first = root.path().join("first.zipzi");
    let second = root.path().join("second.zipzi");
    let before = state.content_id();
    io.save(&state, &ResourceView::default(), &first, None, || true)
        .unwrap();
    let loaded = io.load(&first, || true).unwrap();
    io.save(&loaded.document, &loaded.resources, &second, None, || true)
        .unwrap();
    assert_eq!(json(&first), json(&second));
    assert_eq!(state.content_id(), before);
    assert_eq!(state.editor_state(), loaded.document.editor_state());
    assert_eq!(
        state.package_attachments(),
        loaded.document.package_attachments()
    );
    assert_eq!(state.physics(), loaded.document.physics());
    assert_eq!(state.pose(), loaded.document.pose());
    assert_eq!(state.motion_roots(), loaded.document.motion_roots());
}

#[test]
fn existing_native_archives_keep_format_and_resource_validation() {
    let root = tempfile::tempdir().unwrap();
    let original = super::tests::fixture(root.path());
    let input = root.path().join("input.zipzi");
    fs::write(&input, super::tests::native_bytes(&original, root.path())).unwrap();
    let disk = ProjectDiskBudget::default();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default())
        .with_disk_budget(disk.clone());
    let loaded = io.load(&input, || true).unwrap();
    assert_eq!(loaded.resources.len(), 1); // two asset records share one image version
    assert_eq!(disk.used_bytes(), 0); // extracted inputs are gone; captured resources own bytes
    let output = root.path().join("output.zipzi");
    io.save(&loaded.document, &loaded.resources, &output, None, || true)
        .unwrap();
    assert_eq!(json(&input), json(&output));
    // An old file-format consumer can read the new projection unchanged.
    let old = ProjectArchiveStore::new(root.path().join("old-cache"))
        .open(&output, || true)
        .unwrap();
    assert_eq!(old.document.asset_order(), original.asset_order());
    assert_eq!(
        old.document.package_attachments(),
        original.package_attachments()
    );
}

#[test]
fn missing_or_corrupt_png_is_unresolved_and_resaves_without_fake_pixels() {
    let root = tempfile::tempdir().unwrap();
    let original = super::tests::fixture(root.path());
    let bytes = super::tests::native_bytes(&original, root.path());
    let mut input_zip = ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let path = root.path().join("damaged.zipzi");
    let mut output = ZipWriter::new(File::create(&path).unwrap());
    for i in 0..input_zip.len() {
        let mut entry = input_zip.by_index(i).unwrap();
        let name = entry.name().unwrap().to_string();
        if name.starts_with("assets/") {
            continue;
        }
        output
            .start_file(name, options(CompressionMethod::Stored))
            .unwrap();
        std::io::copy(&mut entry, &mut output).unwrap();
    }
    output.finish().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let loaded = io.load(&path, || true).unwrap();
    assert!(loaded.resources.is_empty());
    assert!(loaded.diagnostics.iter().any(|d| d.code == "RESOURCE_IO"));
    for id in &loaded.document.asset_order()[..2] {
        assert!(matches!(
            loaded.document.asset(id).unwrap().content,
            AssetContent::Unresolved { .. }
        ));
    }
    let saved = root.path().join("unresolved.zipzi");
    io.save(&loaded.document, &loaded.resources, &saved, None, || true)
        .unwrap();
    let reopened = io.load(&saved, || true).unwrap();
    assert!(reopened.resources.is_empty());
    assert_eq!(
        loaded
            .document
            .asset(loaded.document.asset_order().first().unwrap()),
        reopened
            .document
            .asset(reopened.document.asset_order().first().unwrap())
    );
}

#[test]
fn resource_and_archive_limits_and_cancellation_leave_no_candidate_files() {
    let root = tempfile::tempdir().unwrap();
    let original = super::tests::fixture(root.path());
    let input = root.path().join("input.zipzi");
    fs::write(&input, super::tests::native_bytes(&original, root.path())).unwrap();
    let cache = root.path().join("cache");
    let io = ProjectIo::new(&cache, ResourceStore::new(crate::ImageMemoryBudget::new(1)));
    assert!(io.load(&input, || true).is_err());
    assert!(io.load(&input, || false).is_err());
    let state = DocumentState::from_owned(super::state_fixture::owned()).unwrap();
    let limited = io.with_limits(ArchiveIoLimits {
        document_bytes: 1,
        ..Default::default()
    });
    let output = root.path().join("failed.zipzi");
    assert!(limited
        .save(&state, &ResourceView::default(), &output, None, || true)
        .is_err());
    assert!(!output.exists());
}
