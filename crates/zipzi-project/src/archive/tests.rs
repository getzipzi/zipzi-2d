use super::*;
use std::io::Cursor;

fn fixture(root: &Path) -> Document {
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
    assert!(open_archive(&path, root.path(), PrototypeLimits::default(), || true).is_err());
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
