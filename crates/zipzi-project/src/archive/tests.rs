use super::*;
use std::io::Cursor;

#[test]
fn native_preview_is_optional_not_authoring_and_not_reused_after_save() {
    let source = tempfile::tempdir().unwrap();
    let mut store = io(source.path());
    let resources = crate::ResourceView::default();
    let mut png = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png, 2, 2);
        e.set_color(png::ColorType::Rgba);
        e.write_header()
            .unwrap()
            .write_image_data(&[200; 16])
            .unwrap();
    }
    let preview = ProjectPreview::from_png(png).unwrap();
    // Direct archive test with no resource-bearing document.
    let document = zipzi_core::state::DocumentState::new(
        uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(100., 100., zipzi_core::Vec2::new(50., 50.), 10.).into(),
    )
    .unwrap();
    store.set_save_preview(Some(preview.clone()));
    let path = source.path().join("preview.zipzi");
    let saved = store
        .save(&document, &resources, &path, None, || true)
        .unwrap();
    let actual = zipzi_project_preview::read_preview(File::open(&path).unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(preview.png(), actual.png());
    // Advisory damage must not make an otherwise valid document uneditable.
    let bytes = fs::read(&path).unwrap();
    for variant in 0..3 {
        let damaged = repack(&bytes, |name, data| {
            if name == "preview.png" && variant == 0 {
                return Some(b"bad pixels".to_vec());
            }
            if name == "preview.png" && variant == 1 {
                return None;
            }
            if name == "format.json" && variant == 2 {
                let mut format: serde_json::Value = serde_json::from_slice(&data).unwrap();
                format["preview"] = serde_json::json!({"width": "broken"});
                return Some(serde_json::to_vec(&format).unwrap());
            }
            Some(data)
        });
        let damaged_path = source.path().join(format!("damaged-{variant}.zipzi"));
        fs::write(&damaged_path, damaged).unwrap();
        assert!(store.load(&damaged_path, || true).is_ok());
        assert!(zipzi_project_preview::read_preview(File::open(damaged_path).unwrap()).is_err());
    }
    assert!(store.load(&path, || true).unwrap().document.id() == document.id());
    store.set_save_preview(None);
    store
        .save(
            &document,
            &resources,
            &path,
            Some(&saved.fingerprint),
            || true,
        )
        .unwrap();
    assert!(
        zipzi_project_preview::read_preview(File::open(path).unwrap())
            .unwrap()
            .is_none()
    );
}

#[test]
fn optional_preview_is_omitted_when_entry_size_or_disk_admission_is_full() {
    let root = tempfile::tempdir().unwrap();
    let mut store = io(root.path());
    let resources = crate::ResourceView::default();
    let document = zipzi_core::state::DocumentState::new(
        uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(100., 100., zipzi_core::Vec2::new(50., 50.), 10.).into(),
    )
    .unwrap();
    let mut png = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png, 2, 2);
        e.set_color(png::ColorType::Rgba);
        e.write_header()
            .unwrap()
            .write_image_data(&[255; 16])
            .unwrap();
    }
    store.set_save_preview(Some(ProjectPreview::from_png(png).unwrap()));
    let prepared =
        writer::prepare_state(&document, &resources, store.archive.limits, || true).unwrap();
    let base = prepared.extent;
    for variant in 0..4 {
        store.archive.limits = ArchiveIoLimits::default();
        store.archive.disk_budget = ProjectDiskBudget::default();
        match variant {
            0 => store.archive.limits.entries = 2,
            1 => store.archive.limits.format_bytes = prepared.preview_format().len() as u64,
            2 => store.archive.limits.archive_bytes = base,
            _ => store.archive.disk_budget = ProjectDiskBudget::new(base),
        }
        let path = root.path().join(format!("limited-{variant}.zipzi"));
        store
            .save(&document, &resources, &path, None, || true)
            .unwrap();
        assert!(
            zipzi_project_preview::read_preview(File::open(path).unwrap())
                .unwrap()
                .is_none()
        );
    }
}

pub(super) fn fixture(root: &Path) -> crate::ImportedProject {
    fixture_with_image_size(root, 8)
}

fn fixture_with_image_size(root: &Path, size: u32) -> crate::ImportedProject {
    let mut b = zipzi_core::state::DocumentState::new(
        uuid::Uuid::from_u128(1).to_string(),
        zipzi_core::Canvas::new(640., 480., zipzi_core::Vec2::new(320., 240.), 100.).into(),
    )
    .unwrap()
    .builder();
    let store = crate::ResourceStore::default();
    let image = store
        .prepare_rgba(
            size,
            size,
            vec![123; size as usize * size as usize * 4].into(),
        )
        .unwrap();
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(
        root.join("assets/old.png"),
        image.read_asset().unwrap().bytes,
    )
    .unwrap();
    for index in 2..5 {
        b.create_asset(zipzi_core::state::AssetRecord {
            id: uuid::Uuid::from_u128(index).to_string(),
            name: format!("image {index}"),
            content: if index == 4 {
                zipzi_core::state::AssetContent::Empty
            } else {
                zipzi_core::state::AssetContent::Image {
                    resource: image.key(),
                    width: size,
                    height: size,
                }
            },
        })
        .unwrap();
    }
    b.set_package_attachments(vec![zipzi_contract::document::PackageAttachment {
        id: uuid::Uuid::from_u128(10).to_string(),
        path: "userdata/example.bin".into(),
        bytes: Some(Arc::new(vec![0, 255, 7])),
    }])
    .unwrap();
    crate::ImportedProject {
        document: b.freeze().unwrap().state,
        resources: crate::ResourceView::default().with_images([image]),
        protected_paths: vec![],
    }
}
pub(super) fn io(root: &Path) -> ProjectIo {
    ProjectIo::new(root, crate::ResourceStore::default())
}
fn write(input: &crate::ImportedProject, _root: &Path) -> (Vec<u8>, WrittenArchive) {
    let mut output = Cursor::new(Vec::new());
    let result = write_state(input, &mut output, ArchiveIoLimits::default(), || true).unwrap();
    (output.into_inner(), result)
}
fn write_state(
    input: &crate::ImportedProject,
    output: &mut (impl Write + Seek),
    limits: ArchiveIoLimits,
    mut keep_going: impl FnMut() -> bool,
) -> Result<WrittenArchive> {
    let prepared =
        writer::prepare_state(&input.document, &input.resources, limits, &mut keep_going)?;
    writer::write(prepared, output, keep_going, None)
}
fn open_archive(
    path: &Path,
    root: &Path,
    limits: ArchiveIoLimits,
    keep_going: impl FnMut() -> bool,
) -> Result<LoadedProject> {
    io(root).with_limits(limits).load(path, keep_going)
}
pub(super) fn equivalent(
    a: &zipzi_core::state::DocumentState,
    ar: &crate::ResourceView,
    b: &zipzi_core::state::DocumentState,
    br: &crate::ResourceView,
) -> bool {
    fn encoded(s: &zipzi_core::state::DocumentState, r: &crate::ResourceView) -> Vec<u8> {
        let p = writer::prepare_state(s, r, Default::default(), || true).unwrap();
        let mut output = Cursor::new(Vec::new());
        writer::write(p, &mut output, || true, None).unwrap();
        output.into_inner()
    }
    encoded(a, ar) == encoded(b, br)
}
#[test]
fn archive_roundtrip_preserves_all_assets_attachments_and_lease() {
    let source = tempfile::tempdir().unwrap();
    let document = fixture(source.path());
    let original = document.document.content_id();
    let (bytes, written) = write(&document, source.path());
    assert_eq!(written.unique_assets, 1);
    assert_eq!(document.document.asset_order().len(), 3);
    assert_eq!(document.document.content_id(), original);
    let path = source.path().join("角色 with spaces.ZIPZI");
    fs::write(&path, bytes).unwrap();
    let opened = io(source.path()).load(&path, || true).unwrap();
    assert!(equivalent(
        &document.document,
        &document.resources,
        &opened.document,
        &opened.resources
    ));
    assert_eq!(
        opened.document.package_attachments()[0]
            .bytes
            .as_ref()
            .unwrap()
            .as_slice(),
        &[0, 255, 7]
    );
    assert!(matches!(
        opened
            .document
            .asset(&uuid::Uuid::from_u128(4).to_string())
            .unwrap()
            .content,
        zipzi_core::state::AssetContent::Empty
    ));
    let held = opened.clone();
    drop(opened);
    fs::remove_file(path).unwrap();
    fs::remove_dir_all(source.path().join("assets")).unwrap();
    for (_, _, _) in held.document.image_resources() {
        assert!(!held.resources.is_empty());
    }
    for id in held.document.asset_order() {
        let a = held.document.asset(id).unwrap();
        if matches!(a.content, zipzi_core::state::AssetContent::Image { .. }) {
            assert_eq!(held.resources.read_asset(a).unwrap().rgba, vec![123; 256]);
        }
    }
}

#[test]
fn archive_writer_uses_deterministic_zip64_stored_png_and_deflate_json() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let (first, _) = write(&document, root.path());
    assert_eq!(first, write(&document, root.path()).0);
    let mut zip = ZipArchive::new(Cursor::new(&first)).unwrap();
    assert_eq!(zip.len(), 3);
    for index in 0..3 {
        let entry = zip.by_index(index).unwrap();
        assert_eq!(
            entry.compression(),
            if index < 2 {
                CompressionMethod::Deflated
            } else {
                CompressionMethod::Stored
            }
        );
        assert_eq!(entry.last_modified(), Some(zip::DateTime::default()));
        // Local header always reserves ZIP64 size fields, even for a small PNG.
        let header = entry.header_start() as usize;
        assert_eq!(&first[header + 22..header + 26], &[255; 4]);
    }
}

#[test]
fn fingerprints_cover_resource_bytes_and_finalized_headers() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let (bytes, _) = write(&document, root.path());
    let fingerprint =
        crate::archive::read_fingerprint(&mut &bytes[..], bytes.len() as u64, || true).unwrap();
    assert_eq!(fingerprint.sha256, content_sha256(&bytes));
    let mut changed = bytes.clone();
    let data_start = {
        let mut zip = ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let offset = zip.by_index(2).unwrap().data_start().unwrap() as usize;
        offset
    };
    changed[data_start + 40] ^= 1;
    assert_ne!(
        fingerprint,
        crate::archive::read_fingerprint(&mut &changed[..], changed.len() as u64, || true).unwrap()
    );
    let path = root.path().join("changed.zipzi");
    fs::write(&path, changed).unwrap();
    assert!(
        !open_archive(&path, root.path(), ArchiveIoLimits::default(), || true)
            .unwrap()
            .diagnostics
            .is_empty()
    );
}

#[test]
fn copying_is_bounded_handles_short_io_and_checks_cancellation() {
    struct ShortWriter(Vec<u8>);
    impl Write for ShortWriter {
        fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
            let count = input.len().min(7);
            self.0.extend_from_slice(&input[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let bytes = vec![42; (1 << 20) + 12];
    let mut output = ShortWriter(Vec::new());
    let result = stream::copy_exact(
        &mut &bytes[..],
        &mut output,
        bytes.len() as u64,
        &mut || true,
    )
    .unwrap();
    assert_eq!(result.sha256, content_sha256(&bytes));
    assert_eq!(output.0, bytes);
    assert!(stream::copy_exact(
        &mut &bytes[..],
        &mut std::io::sink(),
        bytes.len() as u64 + 1,
        &mut || true
    )
    .is_err());
    assert!(matches!(
        crate::archive::read_fingerprint(&mut &bytes[..], 16, || true),
        Err(ArchiveError::Limit(_))
    ));
    let mut checks = 0;
    assert!(matches!(
        crate::archive::read_fingerprint(&mut &bytes[..], bytes.len() as u64, || {
            checks += 1;
            checks < 2
        }),
        Err(ArchiveError::Cancelled)
    ));
}

#[test]
fn write_failure_preserves_document_and_does_not_silence_bad_resources() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let baseline = document.document.content_id();
    fs::write(root.path().join("assets/old.png"), b"broken").unwrap();
    assert!(write_state(
        &document,
        &mut Cursor::new(Vec::new()),
        Default::default(),
        || true
    )
    .is_ok());
    let absent = crate::ImportedProject {
        resources: Default::default(),
        ..document.clone()
    };
    assert!(write_state(
        &absent,
        &mut Cursor::new(Vec::new()),
        Default::default(),
        || true
    )
    .is_err());
    assert!(write_state(
        &document,
        &mut Cursor::new(Vec::new()),
        Default::default(),
        || false
    )
    .is_err());
    assert_eq!(document.document.content_id(), baseline);
}

#[test]
fn archive_limits_and_failed_open_remove_private_candidates() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let (bytes, _) = write(&document, root.path());
    let path = root.path().join("input.zipzi");
    fs::write(&path, &bytes).unwrap();
    let count = || fs::read_dir(root.path()).unwrap().count();
    let before = count();
    for limits in [
        ArchiveIoLimits {
            archive_bytes: 4,
            ..Default::default()
        },
        ArchiveIoLimits {
            entries: 2,
            ..Default::default()
        },
        ArchiveIoLimits {
            document_bytes: 4,
            ..Default::default()
        },
        ArchiveIoLimits {
            expanded_bytes: 4,
            ..Default::default()
        },
    ] {
        assert!(open_archive(&path, root.path(), limits, || true).is_err());
        assert_eq!(count(), before);
    }
    fs::write(&path, &bytes[..bytes.len() - 12]).unwrap();
    assert!(open_archive(&path, root.path(), ArchiveIoLimits::default(), || true).is_err());
    assert_eq!(count(), before);
}

#[test]
fn native_archive_store_publishes_with_fingerprint_conflict_and_lease() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let source = write(&document, root.path()).0;
    let input = root.path().join("input.zipzi");
    fs::write(&input, source).unwrap();
    let opened = open_archive(&input, root.path(), ArchiveIoLimits::default(), || true).unwrap();
    assert_eq!(
        opened.document.package_attachments()[0].path,
        "userdata/example.bin"
    );
    let target = root.path().join("角色.zipzi");
    let store = io(root.path());
    let receipt = store
        .save(&opened.document, &opened.resources, &target, None, || true)
        .unwrap();
    assert!(receipt.published && receipt.durable);
    assert_eq!(
        receipt.fingerprint,
        crate::archive::read_fingerprint(&mut File::open(&target).unwrap(), 1 << 30, || true)
            .unwrap()
    );
    let reopened = store.load(&target, || true).unwrap();
    assert!(equivalent(
        &opened.document,
        &opened.resources,
        &reopened.document,
        &reopened.resources
    ));
    let error = store
        .save(&opened.document, &opened.resources, &target, None, || true)
        .unwrap_err();
    assert!(error.to_string().contains("Destination exists"));
    fs::write(&target, b"external replacement").unwrap();
    let error = store
        .save(
            &opened.document,
            &opened.resources,
            &target,
            Some(&receipt.fingerprint),
            || true,
        )
        .unwrap_err();
    assert!(error.to_string().contains("PROJECT_CONFLICT"));
}

#[test]
fn unknown_and_escaping_entries_are_rejected_without_extracting_them() {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "../outside",
        "assets/../outside",
        "ASSETS/",
        "C:/outside",
        "assets/a.png",
        "preview.png",
    ] {
        let mut output = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(&mut output);
        zip.start_file(name, options(CompressionMethod::Stored))
            .unwrap();
        zip.write_all(b"bad").unwrap();
        zip.finish().unwrap();
        let path = root.path().join("unknown.zipzi");
        fs::write(&path, output.into_inner()).unwrap();
        assert!(open_archive(&path, root.path(), ArchiveIoLimits::default(), || true).is_err());
    }
}

pub(super) fn native_bytes(input: &crate::ImportedProject, root: &Path) -> Vec<u8> {
    write(input, root).0
}

fn admit(bytes: &[u8], limits: ArchiveIoLimits) -> Result<()> {
    structure::validate(
        &mut Cursor::new(bytes),
        bytes.len() as u64,
        limits,
        &mut || true,
    )
    .map(|_| ())
}
fn central(bytes: &[u8]) -> usize {
    bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap()
}

#[test]
fn preflight_rejects_local_central_mismatches_boundaries_and_metadata_bombs() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let bytes = native_bytes(&document, root.path());
    let c = central(&bytes);
    let e = bytes.len() - 22;
    for (offset, value) in [
        (6, 1),         // local encryption flags
        (8, 99),        // unsupported local compression
        (14, 1),        // local CRC
        (18, 1),        // compressed local size
        (22, 1),        // expanded local size
        (30, b'x'),     // local filename
        (c + 8, 1),     // central encryption
        (c + 10, 99),   // unsupported central compression
        (c + 34, 1),    // split disk
        (c + 41, 0xa1), // Unix symlink mode
        (c + 42, 1),    // local offset skips prefix/overlaps another header
        (e + 4, 1),     // split archive
        (e + 8, 255),   // count mismatch
        (e + 12, 255),  // wrong directory length
        (e + 16, 255),  // wrong directory offset
    ] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert!(
            admit(&bad, ArchiveIoLimits::default()).is_err(),
            "accepted mutation at {offset}"
        );
    }
    assert!(matches!(
        admit(
            &bytes,
            ArchiveIoLimits {
                directory_bytes: 16,
                ..Default::default()
            }
        ),
        Err(ArchiveError::Limit("central directory bytes"))
    ));
    let mut huge = bytes.clone();
    huge[e + 8..e + 12].fill(255);
    assert!(admit(&huge, ArchiveIoLimits::default()).is_err()); // missing ZIP64 end
    let mut prefix = vec![0];
    prefix.extend_from_slice(&bytes);
    assert!(admit(&prefix, ArchiveIoLimits::default()).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(admit(&trailing, ArchiveIoLimits::default()).is_err());
    for n in [0, 21, c, bytes.len() - 1] {
        assert!(admit(&bytes[..n], ArchiveIoLimits::default()).is_err());
    }
}

#[test]
fn preflight_handles_streaming_descriptors_and_zip64_end_records() {
    let root = tempfile::tempdir().unwrap();
    let doc = fixture(root.path());
    let original = native_bytes(&doc, root.path());
    for large in [false, true] {
        let mut input = ZipArchive::new(Cursor::new(&original)).unwrap();
        let mut stream = Vec::new();
        let mut writer = ZipWriter::new_stream(&mut stream);
        for i in 0..input.len() {
            let mut entry = input.by_index(i).unwrap();
            writer
                .start_file(
                    entry.name().unwrap(),
                    SimpleFileOptions::default()
                        .large_file(large)
                        .compression_method(entry.compression()),
                )
                .unwrap();
            std::io::copy(&mut entry, &mut writer).unwrap();
        }
        writer.finish().unwrap();
        admit(&stream, ArchiveIoLimits::default()).unwrap();
        let path = root.path().join("stream.zipzi");
        fs::write(&path, &stream).unwrap();
        assert!(io(root.path()).load(&path, || true).is_ok());
        let descriptor = {
            let mut zip = ZipArchive::new(Cursor::new(&stream)).unwrap();
            let entry = zip.by_index(0).unwrap();
            (entry.data_start().unwrap() + entry.compressed_size()) as usize
        };
        stream[descriptor] ^= 1;
        assert!(admit(&stream, ArchiveIoLimits::default()).is_err());
    }
    // Small valid ZIP with explicit ZIP64 EOCD/locator and classic sentinels.
    let e = original.len() - 22;
    let count = 3u64;
    let c = central(&original) as u64;
    let mut z = original[..e].to_vec();
    z.extend_from_slice(b"PK\x06\x06");
    z.extend_from_slice(&44u64.to_le_bytes());
    z.extend_from_slice(&45u16.to_le_bytes());
    z.extend_from_slice(&45u16.to_le_bytes());
    z.extend_from_slice(&0u64.to_le_bytes());
    z.extend_from_slice(&count.to_le_bytes());
    z.extend_from_slice(&count.to_le_bytes());
    z.extend_from_slice(&(e as u64 - c).to_le_bytes());
    z.extend_from_slice(&c.to_le_bytes());
    z.extend_from_slice(b"PK\x06\x07");
    z.extend_from_slice(&0u32.to_le_bytes());
    z.extend_from_slice(&(e as u64).to_le_bytes());
    z.extend_from_slice(&1u32.to_le_bytes());
    z.extend_from_slice(&original[e..]);
    let n = z.len();
    z[n - 14..n - 2].fill(255);
    admit(&z, ArchiveIoLimits::default()).unwrap();
    let n = z.len();
    z[n - 22 - 20 + 16] = 2;
    assert!(admit(&z, ArchiveIoLimits::default()).is_err());
}

#[test]
fn duplicate_paths_and_case_aliases_never_reach_extraction() {
    for duplicate in [true, false] {
        let mut output = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(&mut output);
        for name in ["format.json", "format.jsoN", "project.json"] {
            writer
                .start_file(name, options(CompressionMethod::Stored))
                .unwrap();
            writer.write_all(b"{}").unwrap();
        }
        writer.finish().unwrap();
        let mut bytes = output.into_inner();
        if duplicate {
            for chunk in bytes
                .windows(11)
                .enumerate()
                .filter_map(|(i, b)| (b == b"format.jsoN").then_some(i))
                .collect::<Vec<_>>()
            {
                bytes[chunk + 10] = b'n';
            }
        }
        assert!(admit(&bytes, ArchiveIoLimits::default()).is_err());
    }
}

fn repack(bytes: &[u8], change: impl Fn(&str, Vec<u8>) -> Option<Vec<u8>>) -> Vec<u8> {
    let mut input = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut output);
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        let name = entry.name().unwrap();
        if let Some(data) = change(&name, data) {
            writer
                .start_file(name, options(entry.compression()))
                .unwrap();
            writer.write_all(&data).unwrap();
        }
    }
    writer.finish().unwrap();
    output.into_inner()
}

fn replace_archive_document(bytes: &[u8], document: &[u8]) -> Vec<u8> {
    repack(bytes, |name, data| {
        Some(match name {
            "project.json" => document.to_vec(),
            "format.json" => {
                let mut format: Format = serde_json::from_slice(&data).unwrap();
                format.document_sha256 = content_sha256(document);
                serde_json::to_vec(&format).unwrap()
            }
            _ => data,
        })
    })
}

#[test]
fn native_document_admission_rejects_duplicate_members_and_incompatible_headers() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let bytes = native_bytes(&document, root.path());
    let mut zip = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut original = String::new();
    zip.by_name("project.json")
        .unwrap()
        .read_to_string(&mut original)
        .unwrap();
    let nested = original.replacen(
        "\"document\":{",
        "\"document\":{\"unused\":{\"x\":0,\"x\":1},",
        1,
    );
    assert_ne!(nested, original);
    for (text, code, message) in [
        (
            original.replacen('{', "{\"format\":\"zipzi-project\",", 1),
            "INVALID_PROJECT",
            "Duplicate JSON",
        ),
        (nested, "INVALID_PROJECT", "Duplicate JSON"),
        (
            r#"{"format":"zipzi-project","format_version":99,"document":null}"#.into(),
            "UNSUPPORTED_VERSION",
            "Unsupported project document version",
        ),
        (
            r#"{"format":"zipzi-directory-project","format_version":21,"document":null}"#.into(),
            "INVALID_PROJECT",
            "Unknown project document identifier",
        ),
        (
            r#"{"format":"zipzi-project","format_version":22,"document":{"unused":"\u0000"}}"#
                .into(),
            "INVALID_PROJECT",
            "Embedded NUL",
        ),
    ] {
        let path = root.path().join("bad-header.zipzi");
        fs::write(&path, replace_archive_document(&bytes, text.as_bytes())).unwrap();
        let error = io(root.path()).load(&path, || true).err().unwrap();
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(Status::from(error).code, code);
    }
}

#[test]
fn shared_png_validation_checks_each_reference_before_and_after_a_valid_one() {
    let root = tempfile::tempdir().unwrap();
    let mut document = fixture(root.path());
    let mut repeated = document
        .document
        .asset(&uuid::Uuid::from_u128(2).to_string())
        .unwrap()
        .clone();
    repeated.id = uuid::Uuid::from_u128(5).to_string();
    let mut b = document.document.builder();
    b.create_asset(repeated).unwrap();
    document.document = b.freeze().unwrap().state;
    let bytes = native_bytes(&document, root.path());
    let mut zip = ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut json = String::new();
    zip.by_name("project.json")
        .unwrap()
        .read_to_string(&mut json)
        .unwrap();
    for (bad_id, width, code) in [
        (2, 7, "RESOURCE_DIMENSIONS"),
        (3, 7, "RESOURCE_DIMENSIONS"),
        (2, u32::MAX, "DECODE_WORKSPACE_LIMIT"),
    ] {
        let bad_id = uuid::Uuid::from_u128(bad_id).to_string();
        let mut wire: serde_json::Value = serde_json::from_str(&json).unwrap();
        for asset in wire["document"]["assets"].as_array_mut().unwrap() {
            if asset["id"] == bad_id {
                asset["content"]["width"] = width.into();
            }
        }
        let path = root.path().join("shared.zipzi");
        fs::write(
            &path,
            replace_archive_document(&bytes, &serde_json::to_vec(&wire).unwrap()),
        )
        .unwrap();
        let mut store = io(root.path());
        let budget = crate::ImageMemoryBudget::default();
        let memory = Arc::new(budget.reserve_transfer(0).unwrap());
        store.set_open_memory(Some(memory));
        let opened = store.load(&path, || true).unwrap();
        assert_eq!(opened.stats.unique_assets, 1);
        assert_eq!(opened.diagnostics.len(), 2); // Also the missing-content asset.
        assert!(opened
            .diagnostics
            .iter()
            .any(|d| d.asset_id == bad_id && d.code == code));
        assert!(!opened
            .diagnostics
            .iter()
            .any(|d| d.asset_id == uuid::Uuid::from_u128(5).to_string()));
        let good = opened
            .document
            .asset(&uuid::Uuid::from_u128(5).to_string())
            .unwrap();
        let texture = opened.resources.read_asset(good).unwrap();
        assert_eq!((texture.width, texture.height), (8, 8));
    }
}

#[test]
fn png_workspace_and_buffers_share_one_host_reservation_bound() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture_with_image_size(root.path(), 512);
    let path = root.path().join("large-image.zipzi");
    fs::write(&path, native_bytes(&document, root.path())).unwrap();

    // The PNG peak is about 6 MiB plus 2 MiB for the ZIP index. Charging
    // the same pixel buffers twice adds another 2 MiB.
    for (limit, should_open) in [(7 << 20, false), (9 << 20, true)] {
        let budget = crate::ImageMemoryBudget::with_system_headroom(move || limit);
        let resources = crate::ResourceStore::new(budget.clone());
        let mut store = ProjectIo::new(root.path().join("private"), resources);
        store.set_open_memory(Some(Arc::new(budget.reserve_transfer(0).unwrap())));
        let loaded = store.load(&path, || true);
        drop(store);
        if should_open {
            let loaded = loaded.unwrap();
            assert!(equivalent(
                &document.document,
                &document.resources,
                &loaded.document,
                &loaded.resources
            ));
        } else {
            let error = loaded.err().unwrap();
            assert!(matches!(error, ArchiveError::Document { ref code, .. }
                if code == "BUDGET_EXCEEDED" || code == "RESOURCE_BUDGET_EXCEEDED"));
        }
        assert_eq!(budget.usage().total_bytes(), 0);
    }
}

#[test]
fn expanded_json_is_admitted_before_decoding_and_failed_candidates_are_reclaimed() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let original = native_bytes(&document, root.path());
    let mut input = ZipArchive::new(Cursor::new(&original)).unwrap();
    let mut padded = vec![b' '; 1 << 20];
    input
        .by_name("project.json")
        .unwrap()
        .read_to_end(&mut padded)
        .unwrap();
    let bytes = replace_archive_document(&original, &padded);
    assert!(bytes.len() < 10 << 10);
    let path = root.path().join("compressed.zipzi");
    fs::write(&path, &bytes).unwrap();
    let budget = crate::ImageMemoryBudget::default();
    budget.bind_host_budget(0, 3 << 20);
    let lease = Arc::new(budget.reserve_transfer(4096).unwrap());
    let cache = root.path().join("private");
    let mut store = io(&cache);
    store.set_open_memory(Some(lease.clone()));
    let error = store.load(&path, || true).err().unwrap();
    assert!(matches!(error, ArchiveError::Document { ref code, .. } if code == "BUDGET_EXCEEDED"));
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 0);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    budget.bind_host_budget(0, 8 << 20);
    let opened = store.load(&path, || true).unwrap();
    assert!(equivalent(
        &document.document,
        &document.resources,
        &opened.document,
        &opened.resources
    ));
    assert!(budget.usage().transfer_bytes < 3 << 20);
    drop(opened);

    // A smaller but structurally dense document needs room for decoded values.
    let mut dense = br#"{"extra":["#.to_vec();
    dense.extend(b"0,".repeat(1 << 17));
    dense.extend_from_slice(b"0]}");
    let bytes = replace_archive_document(&original, &dense);
    fs::write(&path, &bytes).unwrap();
    let error = store.load(&path, || true).err().unwrap();
    assert!(matches!(error, ArchiveError::Document { ref code, .. } if code == "BUDGET_EXCEEDED"));
    assert_eq!(fs::read_dir(&cache).unwrap().count(), 0);

    drop(store);
    drop(lease);
    assert_eq!(budget.usage().transfer_bytes, 0);
}

#[test]
fn versions_features_and_duplicate_json_keys_have_distinct_failures() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let bytes = native_bytes(&document, root.path());
    let path = root.path().join("input.data"); // SDK identifies content, not suffix.
    for (key, value, code) in [
        (
            "container_version",
            serde_json::json!(2),
            "UNSUPPORTED_CONTAINER_VERSION",
        ),
        (
            "required_features",
            serde_json::json!(["future-feature"]),
            "UNSUPPORTED_PROJECT_FEATURE",
        ),
    ] {
        let bad = repack(&bytes, |name, data| {
            if name != "format.json" {
                return Some(data);
            }
            let mut json: serde_json::Value = serde_json::from_slice(&data).unwrap();
            json[key] = value.clone();
            Some(serde_json::to_vec(&json).unwrap())
        });
        fs::write(&path, bad).unwrap();
        let error = io(root.path()).load(&path, || true).err().unwrap();
        assert!(matches!(error, ArchiveError::Document { code: actual, .. } if actual == code));
    }
    let bad = repack(&bytes, |name, data| {
        if name != "format.json" {
            return Some(data);
        }
        let text = String::from_utf8(data).unwrap();
        Some(
            text.replacen('{', "{\"format\":\"zipzi-project-package\",", 1)
                .into_bytes(),
        )
    });
    fs::write(&path, bad).unwrap();
    assert!(io(root.path())
        .load(&path, || true)
        .err()
        .unwrap()
        .to_string()
        .contains("Duplicate JSON"));
    // The production reader never accepts the experimental format.
    let experimental = repack(&bytes, |name, data| {
        if name == "format.json" {
            let mut f: serde_json::Value = serde_json::from_slice(&data).unwrap();
            f["required_features"] = serde_json::json!(["prototype-directory-document-v21"]);
            Some(serde_json::to_vec(&f).unwrap())
        } else {
            Some(data)
        }
    });
    fs::write(&path, experimental).unwrap();
    assert!(io(root.path()).load(&path, || true).is_err());
}

#[test]
fn bad_or_missing_images_remain_explicitly_unresolved_when_saved() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let bytes = native_bytes(&document, root.path());
    for missing in [true, false] {
        let bad = repack(&bytes, |name, mut data| {
            if name.starts_with("assets/") {
                if missing {
                    return None;
                }
                data[40] ^= 1;
            }
            Some(data)
        });
        let path = root.path().join("bad.zipzi");
        fs::write(&path, bad).unwrap();
        let store = io(root.path());
        let opened = store.load(&path, || true).unwrap();
        assert_eq!(opened.stats.unique_assets, 0);
        assert_eq!(opened.diagnostics.len(), 3);
        assert!(store
            .save(
                &opened.document,
                &opened.resources,
                &root.path().join(format!("out-{missing}.zipzi")),
                None,
                || true
            )
            .is_ok());
    }
}
