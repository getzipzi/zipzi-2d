use super::*;

fn png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
    }
    bytes
}

fn asset(path: &Path, width: u32, height: u32, hash: &str) -> ImageAsset {
    ImageAsset {
        id: "asset".into(),
        name: "texture".into(),
        unresolved_source: None,
        content: Some(zipzi_core::ImageContent {
            source: path.to_string_lossy().into(),
            width,
            height,
            sha256: hash.into(),
        }),
    }
}

#[test]
fn memory_acquisition_shares_arc_and_never_creates_png_or_new_budget_charge() {
    let budget = crate::ImageMemoryBudget::new(1024);
    let pixels: Arc<[u8]> = Arc::from([123, 231, 17, 0, 1, 2, 3, 128]);
    let image = PreparedImageContent::from_rgba(2, 1, pixels.clone(), &budget).unwrap();
    let before = budget.usage();
    let verified = image.verified_texture().unwrap();
    assert!(Arc::ptr_eq(&pixels, &verified.texture.rgba));
    assert_eq!(&*verified.texture.rgba, &*pixels);
    assert!(image.memory.png.lock().unwrap().is_none());
    assert_eq!(budget.usage(), before);
    assert!(matches!(
        verified.source_content_identity,
        ObservationContentIdentity::Rgba8Sha256 { .. }
    ));
    assert_eq!(
        verified.source_content_identity,
        image.verified_texture().unwrap().source_content_identity
    );
}

#[test]
fn memory_identity_is_stable_after_another_owner_lazily_encodes_png() {
    let image = PreparedImageContent::from_rgba(
        1,
        1,
        Arc::from([7, 8, 9, 0]),
        &crate::ImageMemoryBudget::default(),
    )
    .unwrap();
    let before = image.verified_texture().unwrap().source_content_identity;
    image.read_asset().unwrap();
    assert!(image.memory.png.lock().unwrap().is_some());
    assert_eq!(
        before,
        image.verified_texture().unwrap().source_content_identity
    );
}

#[test]
fn memory_metadata_failures_do_not_encode_pixels_to_obtain_a_hash() {
    let image = PreparedImageContent::from_rgba(
        1,
        1,
        Arc::from([255; 4]),
        &crate::ImageMemoryBudget::default(),
    )
    .unwrap();
    let mut changed = image.clone();
    changed.content.width = 2;
    assert_eq!(
        changed.verified_texture().unwrap_err().code,
        "RESOURCE_DIMENSIONS"
    );
    changed.content.width = 1;
    changed.content.sha256 = "a".repeat(64);
    assert_eq!(
        changed.verified_texture().unwrap_err().code,
        "RESOURCE_HASH"
    );
    assert!(image.memory.png.lock().unwrap().is_none());
    let loaded = crate::prepare_png_content(
        Arc::from(png(1, 1, &[255; 4])),
        &crate::ImageMemoryBudget::default(),
    )
    .unwrap();
    assert!(loaded.verified_texture().is_ok());
}

#[test]
fn disk_acquisition_verifies_dimensions_hash_and_complete_png() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.png");
    let bytes = png(2, 1, &[9, 8, 7, 0, 6, 5, 4, 128]);
    fs::write(&path, &bytes).unwrap();
    let hash = content_sha256(&bytes);
    let source = asset(&path, 2, 1, &hash);
    let acquire = |a: &ImageAsset| {
        read_project_texture_bounded(directory.path(), a, PngDecodeLimits::default(), || true)
    };
    let verified = acquire(&source).unwrap();
    assert_eq!(
        verified.source_content_identity,
        ObservationContentIdentity::PngSha256 { hash: hash.clone() }
    );
    assert_eq!(&*verified.texture.rgba, [9, 8, 7, 0, 6, 5, 4, 128]);
    // Missing declared identity is allowed, but execution reports the actual one.
    assert_eq!(
        acquire(&asset(&path, 2, 1, ""))
            .unwrap()
            .source_content_identity,
        verified.source_content_identity
    );
    assert_eq!(
        acquire(&asset(&path, 1, 2, &hash)).unwrap_err().code,
        "RESOURCE_DIMENSIONS"
    );
    fs::write(&path, png(2, 1, &[255; 8])).unwrap();
    assert_eq!(acquire(&source).unwrap_err().code, "RESOURCE_HASH");
    let mut corrupt = bytes;
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&path, corrupt).unwrap();
    assert_eq!(
        acquire(&asset(&path, 2, 1, "")).unwrap_err().code,
        "INVALID_PNG"
    );
}

#[test]
fn disk_acquisition_rejects_encoded_size_and_workspace_before_reading_png() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.png");
    // Invalid bytes prove these checks occur before the PNG decoder is entered.
    fs::write(&path, [0; 100]).unwrap();
    let source = asset(&path, 2, 1, "");
    assert_eq!(
        read_project_texture_bounded(
            directory.path(),
            &source,
            PngDecodeLimits {
                max_encoded_bytes: 99,
                ..Default::default()
            },
            || true
        )
        .unwrap_err()
        .code,
        "ENCODED_READ_LIMIT"
    );
    assert_eq!(
        read_project_texture_bounded(
            directory.path(),
            &source,
            PngDecodeLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            || true
        )
        .unwrap_err()
        .code,
        "DECODE_WORKSPACE_LIMIT"
    );
}

#[test]
fn a_file_growing_after_length_check_cannot_expand_the_read_buffer() {
    use std::io::Write;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.png");
    fs::write(&path, png(1, 1, &[255; 4])).unwrap();
    let source = asset(&path, 1, 1, "");
    let mut checks = 0;
    let error = read_project_texture_bounded(
        directory.path(),
        &source,
        PngDecodeLimits::default(),
        || {
            checks += 1;
            if checks == 3 {
                fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .unwrap()
                    .write_all(&[0; 1024])
                    .unwrap();
            }
            true
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "RESOURCE_CHANGED");
}

#[test]
fn cancellation_is_checked_between_hash_chunks_and_before_disk_io() {
    let directory = tempfile::tempdir().unwrap();
    let source = asset(&directory.path().join("missing.png"), 1, 1, "");
    assert_eq!(
        read_project_texture_bounded(
            directory.path(),
            &source,
            PngDecodeLimits::default(),
            || false
        )
        .unwrap_err()
        .code,
        "CANCELLED"
    );
    let pixels: Arc<[u8]> = vec![255; 128 << 10].into();
    let image =
        PreparedImageContent::from_rgba(512, 64, pixels, &crate::ImageMemoryBudget::default())
            .unwrap();
    let mut checks = 0;
    assert_eq!(
        image
            .verified_texture_with_control(|| {
                checks += 1;
                checks < 4
            })
            .unwrap_err()
            .code,
        "CANCELLED"
    );
    assert!(image.memory.png.lock().unwrap().is_none());
    let data = vec![17; 200_000];
    assert_eq!(
        content_sha256_with_control(&data, || true).unwrap(),
        content_sha256(&data)
    );
}

#[test]
fn pixel_identity_pins_domain_dimensions_and_pixel_conventions() {
    let pixels = [123, 231, 9, 0, 5, 10, 15, 1];
    let hash = rgba8_sha256_with_control(2, 1, &pixels, || true).unwrap();
    assert_ne!(
        hash,
        rgba8_sha256_with_control(1, 2, &pixels, || true).unwrap()
    );
    assert_ne!(hash, content_sha256(&pixels));
    assert_eq!(
        hash,
        "cde2664d1e2277585c0257a690412e849799c38c1480abde6ecf353af2229204"
    );
}
