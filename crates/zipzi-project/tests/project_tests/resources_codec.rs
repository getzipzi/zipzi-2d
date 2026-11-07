use super::helpers::*;
use std::{collections::HashMap, fs};
use zipzi_core::{
    evaluation::{evaluate_frame, DrawableFrame},
    state::*,
    *,
};
use zipzi_project::{content_sha256, decode_png, ResourceStore, ResourceView};

#[test]
fn captured_resource_reads_validate_record_and_keep_owned_bytes_after_source_changes() {
    let tmp = TestDirectory::new();
    let path = tmp.0.join("texture.png");
    let (original, _) = create_test_png(&path);
    let store = ResourceStore::default();
    let image = store.import_image(&path).unwrap();
    let view = ResourceView::default().with_images([image.clone()]);
    let mut asset = AssetRecord {
        id: id(2),
        name: String::new(),
        content: AssetContent::Image {
            resource: image.key(),
            width: 8,
            height: 8,
        },
    };
    let captured = view.read_asset(&asset).unwrap();
    assert_eq!(captured.rgba.len(), 8 * 8 * 4);
    asset.content = AssetContent::Image {
        resource: image.key(),
        width: 16,
        height: 8,
    };
    assert_eq!(
        view.read_asset(&asset).unwrap_err().code,
        "RESOURCE_DIMENSIONS"
    );
    asset.content = AssetContent::Image {
        resource: Default::default(),
        width: 8,
        height: 8,
    };
    assert_eq!(
        view.read_asset(&asset).unwrap_err().code,
        "MISSING_RESOURCE"
    );
    asset.content = AssetContent::Image {
        resource: image.key(),
        width: 8,
        height: 8,
    };
    fs::write(&path, b"corrupt image").unwrap();
    assert!(store.import_image(&path).is_err());
    assert_eq!(view.read_asset(&asset).unwrap().rgba, captured.rgba);
    fs::write(&path, original).unwrap();
    let restored = store.import_image(&path).unwrap();
    assert_ne!(restored.key(), image.key());
    assert_eq!(restored.texture().rgba, image.texture().rgba);
    fs::remove_file(&path).unwrap();
    assert!(store.import_image(&path).is_err());
    assert_eq!(view.read_asset(&asset).unwrap().rgba, captured.rgba);
}

#[test]
fn native_roundtrip_preserves_frames_and_moc3_bytes() {
    let before = fixture_doc();
    let encoded = fixture_bytes(&before);
    let wire = project_json(&encoded);
    assert_eq!(wire["format"], "zipzi-project");
    assert_eq!(wire["format_version"], 22);
    let decoded = load_native(&encoded).unwrap();
    assert_eq!(project_json(&resaved_native(&encoded)), wire);
    let p = HashMap::from([(id(6), 0.5)]);
    let mut a = DrawableFrame::default();
    let mut b = DrawableFrame::default();
    assert!(evaluate_frame(&before, &p, &mut a).is_ok());
    assert!(evaluate_frame(&decoded, &p, &mut b).is_ok());
    assert_eq!(a.drawables.len(), b.drawables.len());
    for (a, b) in a.drawables.iter().zip(&b.drawables) {
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.uvs, b.uvs);
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.draw_order, b.draw_order);
    }
    assert_eq!(
        zipzi_moc3::encode_moc3(&before).unwrap().bytes,
        zipzi_moc3::encode_moc3(&decoded).unwrap().bytes
    );
}

#[test]
fn native_codec_rejects_bad_format_duplicate_keys_versions_and_traversal() {
    let bytes = fixture_bytes(&fixture_doc());
    let wire = project_json(&bytes);
    for (field, value, code) in [
        (
            "format",
            serde_json::json!("unknown-project"),
            "INVALID_PROJECT",
        ),
        (
            "format",
            serde_json::json!("zipzi-directory-project"),
            "INVALID_PROJECT",
        ),
        (
            "format_version",
            serde_json::json!(99),
            "UNSUPPORTED_VERSION",
        ),
    ] {
        let mut modified = wire.clone();
        modified[field] = value;
        assert_eq!(
            load_native(&replace_project_json(&bytes, &modified))
                .unwrap_err()
                .code,
            code
        );
    }
    let text = serde_json::to_string(&wire).unwrap();
    let duplicate = text.replacen("{", "{\"format\":\"duplicate\",", 1);
    assert_eq!(
        load_native(&crate::support::replace_project_text(&bytes, &duplicate))
            .unwrap_err()
            .code,
        "INVALID_PROJECT"
    );
    let mut traversal = wire;
    traversal["document"]["assets"][0]["source"] = "../escape.png".into();
    assert_eq!(
        load_native(&replace_project_json(&bytes, &traversal))
            .unwrap_err()
            .code,
        "INVALID_PROJECT"
    );
}

#[test]
fn project_roundtrip_preserves_precise_rotation_origin() {
    let origin = 0.9545455574989319_f64;
    let document = prepare(
        &DocumentState::new(id(1), ModelSetup::default()).unwrap(),
        |b| {
            b.create_transform(Transform {
                id: id(2),
                runtime_id: "rotation".into(),
                data: TransformData::Rotation(RotationTransform {
                    base_angle: 0.,
                    pose: RotationPose {
                        origin: PreciseVec2::new(0.06838598102331161, origin),
                        ..Default::default()
                    },
                }),
                ..Default::default()
            })
        },
    );
    let bytes = fixture_bytes(&document);
    let reopened = load_native(&bytes).unwrap();
    assert_eq!(
        reopened
            .transform(&id(2))
            .unwrap()
            .rotation()
            .unwrap()
            .pose
            .origin
            .y
            .to_bits(),
        origin.to_bits()
    );
    assert_eq!(
        project_json(&bytes),
        project_json(&fixture_bytes(&reopened))
    );
}

#[test]
fn test_sha256_known_vectors() {
    for (input, expected) in [
        (
            "",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ] {
        assert_eq!(content_sha256(input.as_bytes()), expected);
    }
}

#[test]
fn test_png_decode_and_sha256() {
    let temp_dir = std::env::temp_dir().join(format!("zipzi-test-png-{}", std::process::id()));
    let png_path = temp_dir.join("test.png");
    let (bytes, sha) = create_test_png(&png_path);

    assert_eq!(sha.len(), 64);
    assert_eq!(content_sha256(&bytes), sha);

    let data = decode_png(&bytes).expect("decode_png failed");
    assert_eq!(data.width, 8);
    assert_eq!(data.height, 8);
    assert_eq!(data.sha256, sha);
    assert_eq!(data.rgba.len(), 8 * 8 * 4);

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn png_normalization_preserves_palette_low_depth_16bit_and_transparency() {
    let cases = [
        (
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            vec![0],
            Some(vec![0]),
            [255, 0, 0, 0],
        ),
        (
            png::ColorType::Grayscale,
            png::BitDepth::One,
            vec![0x80],
            None,
            [255, 255, 255, 255],
        ),
        (
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            vec![0xff, 0xff, 0, 0, 0x80, 0x80, 0x80, 0x80],
            None,
            [255, 0, 128, 128],
        ),
        (
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            vec![255, 0, 0],
            Some(vec![0, 255, 0, 0, 0, 0]),
            [255, 0, 0, 0],
        ),
        (
            png::ColorType::Grayscale,
            png::BitDepth::Eight,
            vec![123],
            Some(vec![0, 123]),
            [123, 123, 123, 0],
        ),
    ];
    for (color, depth, bytes, transparency, expected) in cases {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, 1, 1);
            encoder.set_color(color);
            encoder.set_depth(depth);
            if color == png::ColorType::Indexed {
                encoder.set_palette(vec![255, 0, 0]);
            }
            if let Some(trns) = transparency {
                encoder.set_trns(trns);
            }
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&bytes)
                .unwrap();
        }
        assert_eq!(
            decode_png(&encoded).unwrap().rgba,
            expected,
            "{color:?} {depth:?}"
        );
    }
}
