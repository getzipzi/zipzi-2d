use super::*;
use std::io::Cursor;

pub(super) fn fixture(root: &Path) -> Document {
    let mut document = Document::new();
    assert!(document
        .initialize(
            uuid::Uuid::from_u128(1).to_string(),
            zipzi_core::Canvas::new(640., 480., zipzi_core::Vec2::new(320., 240.), 100.)
        )
        .is_ok());
    fs::create_dir_all(root.join("assets")).unwrap();
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 8, 8);
        encoder.set_color(png::ColorType::Rgba);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[123; 256])
            .unwrap();
    }
    fs::write(root.join("assets/old.png"), &png).unwrap();
    for index in 2..5 {
        assert!(document
            .add_asset(zipzi_core::ImageAsset {
                id: uuid::Uuid::from_u128(index).to_string(),
                name: format!("image {index}"),
                content: (index != 4).then(|| zipzi_core::ImageContent {
                    source: "assets/old.png".into(),
                    width: 8,
                    height: 8,
                    sha256: content_sha256(&png),
                }),
                unresolved_source: None,
            })
            .status
            .is_ok());
    }
    assert!(document
        .set_package_attachments(vec![zipzi_contract::document::PackageAttachment {
            id: uuid::Uuid::from_u128(10).to_string(),
            path: "userdata/example.bin".into(),
            bytes: Some(Arc::new(vec![0, 255, 7])),
        }])
        .status
        .is_ok());
    document
}

fn write(document: &Document, root: &Path) -> (Vec<u8>, WrittenArchive) {
    let mut output = Cursor::new(Vec::new());
    let result = write_archive(
        document,
        root,
        &mut output,
        PrototypeLimits::default(),
        || true,
    )
    .unwrap();
    (output.into_inner(), result)
}

#[test]
fn archive_roundtrip_preserves_all_assets_attachments_and_lease() {
    let source = tempfile::tempdir().unwrap();
    let document = fixture(source.path());
    let original = encode_project(&document).unwrap();
    let (bytes, written) = write(&document, source.path());
    assert_eq!(written.unique_assets, 1);
    assert_eq!(document.asset_order().len(), 3); // No Mesh uses these assets.
    assert_eq!(encode_project(&document).unwrap(), original);
    let path = source.path().join("角色 with spaces.ZIPZI");
    fs::write(&path, bytes).unwrap();
    let opened = open_archive(&path, source.path(), PrototypeLimits::default(), || true).unwrap();
    assert!(written.document.same_content(&opened.document));
    assert_eq!(
        opened.document.package_attachments()[0]
            .bytes
            .as_ref()
            .unwrap()
            .as_slice(),
        &[0, 255, 7]
    );
    assert!(opened
        .document
        .get_asset(&uuid::Uuid::from_u128(4).to_string())
        .unwrap()
        .content
        .is_none());
    let working = opened.asset_root().to_owned();
    let held = opened.clone();
    drop(opened);
    fs::remove_file(path).unwrap();
    fs::remove_dir_all(source.path().join("assets")).unwrap();
    assert!(working.exists());
    for id in held.document.asset_order() {
        if let Some(content) = &held.document.get_asset(id).unwrap().content {
            let png = fs::read(working.join(&content.source)).unwrap();
            assert_eq!(content_sha256(&png), content.sha256);
        }
    }
    drop(held);
    assert!(!working.exists());
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
        ProjectFingerprint::read(&mut &bytes[..], bytes.len() as u64, || true).unwrap();
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
        ProjectFingerprint::read(&mut &changed[..], changed.len() as u64, || true).unwrap()
    );
    let path = root.path().join("changed.zipzi");
    fs::write(&path, changed).unwrap();
    assert!(
        !open_archive(&path, root.path(), PrototypeLimits::default(), || true)
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
        ProjectFingerprint::read(&mut &bytes[..], 16, || true),
        Err(ArchiveError::Limit(_))
    ));
    let mut checks = 0;
    assert!(matches!(
        ProjectFingerprint::read(&mut &bytes[..], bytes.len() as u64, || {
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
    let baseline = encode_project(&document).unwrap();
    fs::write(root.path().join("assets/old.png"), b"broken").unwrap();
    assert!(write_archive(
        &document,
        root.path(),
        &mut Cursor::new(Vec::new()),
        PrototypeLimits::default(),
        || true
    )
    .is_err());
    assert_eq!(encode_project(&document).unwrap(), baseline);
    assert!(write_archive(
        &document,
        root.path(),
        &mut Cursor::new(Vec::new()),
        PrototypeLimits::default(),
        || false
    )
    .is_err());
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
        PrototypeLimits {
            archive_bytes: 4,
            ..Default::default()
        },
        PrototypeLimits {
            entries: 2,
            ..Default::default()
        },
        PrototypeLimits {
            document_bytes: 4,
            ..Default::default()
        },
        PrototypeLimits {
            expanded_bytes: 4,
            ..Default::default()
        },
    ] {
        assert!(open_archive(&path, root.path(), limits, || true).is_err());
        assert_eq!(count(), before);
    }
    fs::write(&path, &bytes[..bytes.len() - 12]).unwrap();
    assert!(open_archive(&path, root.path(), PrototypeLimits::default(), || true).is_err());
    assert_eq!(count(), before);
}

#[test]
fn native_archive_store_publishes_with_fingerprint_conflict_and_lease() {
    let root = tempfile::tempdir().unwrap();
    let document = fixture(root.path());
    let source = write(&document, root.path()).0;
    let input = root.path().join("input.zipzi");
    fs::write(&input, source).unwrap();
    let opened = open_archive(&input, root.path(), PrototypeLimits::default(), || true).unwrap();
    assert_eq!(
        opened.document.package_attachments()[0].path,
        "userdata/example.bin"
    );
    let target = root.path().join("角色.zipzi");
    let store = ProjectArchiveStore::new(root.path());
    let receipt = store
        .save(
            &opened.document,
            &opened.resource_lease(),
            &target,
            None,
            || true,
        )
        .unwrap();
    assert!(receipt.published && receipt.durable);
    assert_eq!(
        receipt.fingerprint,
        ProjectFingerprint::read(&mut File::open(&target).unwrap(), 1 << 30, || true).unwrap()
    );
    let reopened = store.open(&target, || true).unwrap();
    assert!(opened.document.same_content(&reopened.document));
    let error = store
        .save(
            &opened.document,
            &opened.resource_lease(),
            &target,
            None,
            || true,
        )
        .unwrap_err();
    assert!(error.to_string().contains("Destination exists"));
    fs::write(&target, b"external replacement").unwrap();
    let error = store
        .save(
            &opened.document,
            &opened.resource_lease(),
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
        assert!(open_archive(&path, root.path(), PrototypeLimits::default(), || true).is_err());
    }
}

pub(super) fn native_bytes(document: &Document, root: &Path) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    write_archive_inner(
        document,
        root,
        &mut output,
        PrototypeLimits::default(),
        || true,
        ArchiveDocumentFormat::NativeV22,
    )
    .unwrap();
    output.into_inner()
}
fn admit(bytes: &[u8], limits: PrototypeLimits) -> Result<()> {
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
            admit(&bad, PrototypeLimits::default()).is_err(),
            "accepted mutation at {offset}"
        );
    }
    assert!(matches!(
        admit(
            &bytes,
            PrototypeLimits {
                directory_bytes: 16,
                ..Default::default()
            }
        ),
        Err(ArchiveError::Limit("central directory bytes"))
    ));
    let mut huge = bytes.clone();
    huge[e + 8..e + 12].fill(255);
    assert!(admit(&huge, PrototypeLimits::default()).is_err()); // missing ZIP64 end
    let mut prefix = vec![0];
    prefix.extend_from_slice(&bytes);
    assert!(admit(&prefix, PrototypeLimits::default()).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(admit(&trailing, PrototypeLimits::default()).is_err());
    for n in [0, 21, c, bytes.len() - 1] {
        assert!(admit(&bytes[..n], PrototypeLimits::default()).is_err());
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
        admit(&stream, PrototypeLimits::default()).unwrap();
        let path = root.path().join("stream.zipzi");
        fs::write(&path, &stream).unwrap();
        assert!(ProjectArchiveStore::new(root.path())
            .open(&path, || true)
            .is_ok());
        let descriptor = {
            let mut zip = ZipArchive::new(Cursor::new(&stream)).unwrap();
            let entry = zip.by_index(0).unwrap();
            (entry.data_start().unwrap() + entry.compressed_size()) as usize
        };
        stream[descriptor] ^= 1;
        assert!(admit(&stream, PrototypeLimits::default()).is_err());
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
    admit(&z, PrototypeLimits::default()).unwrap();
    let n = z.len();
    z[n - 22 - 20 + 16] = 2;
    assert!(admit(&z, PrototypeLimits::default()).is_err());
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
        assert!(admit(&bytes, PrototypeLimits::default()).is_err());
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
        let error = ProjectArchiveStore::new(root.path())
            .open(&path, || true)
            .err()
            .unwrap();
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
    assert!(ProjectArchiveStore::new(root.path())
        .open(&path, || true)
        .err()
        .unwrap()
        .to_string()
        .contains("Duplicate JSON"));
    // The production reader never accepts the experimental format.
    fs::write(&path, write(&document, root.path()).0).unwrap();
    assert!(ProjectArchiveStore::new(root.path())
        .open(&path, || true)
        .is_err());
}

#[test]
fn bad_or_missing_images_open_with_diagnostics_but_cannot_be_saved() {
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
        let store = ProjectArchiveStore::new(root.path());
        let opened = store.open(&path, || true).unwrap();
        assert_eq!(opened.unique_assets, 0);
        assert_eq!(opened.diagnostics.len(), 3);
        assert!(store
            .save(
                &opened.document,
                &opened.resource_lease(),
                &root.path().join("out.zipzi"),
                None,
                || true
            )
            .is_err());
    }
}
