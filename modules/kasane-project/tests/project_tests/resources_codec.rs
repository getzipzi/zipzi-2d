use kasane_core::{RotationTransform, TransformData};
use std::collections::HashMap;
use std::fs;

use super::helpers::*;
use kasane_core::evaluation::{evaluate_frame, DrawableFrame};
use kasane_core::types::{Canvas, ImageAsset, RotationPose, Transform, Vec2};
use kasane_core::Document;
use kasane_moc3::encode_moc3;
use kasane_project::{content_sha256, decode_png, decode_project, encode_project, DocumentStore};

#[test]
fn cached_asset_reads_still_validate_disk_content_and_metadata() {
    use kasane_project::store::read_project_asset_if_changed;
    let root = std::env::temp_dir().join(format!("kasane-cached-resource-{}", std::process::id()));
    let path = root.join("assets/texture.png");
    let (_, hash) = create_test_png(&path);
    let mut asset = ImageAsset {
        unresolved_source: None,
        id: id(2),
        name: String::new(),
        content: Some(kasane_core::ImageContent {
            source: "assets/texture.png".into(),
            width: 8,
            height: 8,
            sha256: hash.clone(),
        }),
    };
    let decoded = read_project_asset_if_changed(&root, &asset, None)
        .unwrap()
        .unwrap();
    assert_eq!(decoded.rgba.len(), 8 * 8 * 4);
    let cached = Some((hash.as_str(), 8, 8));
    assert!(read_project_asset_if_changed(&root, &asset, cached)
        .unwrap()
        .is_none());
    asset.content.as_mut().unwrap().width = 16;
    assert_eq!(
        read_project_asset_if_changed(&root, &asset, cached)
            .unwrap_err()
            .code,
        "RESOURCE_DIMENSIONS"
    );
    asset.content.as_mut().unwrap().width = 8;
    asset.content.as_mut().unwrap().sha256 = "0".repeat(64);
    assert_eq!(
        read_project_asset_if_changed(&root, &asset, cached)
            .unwrap_err()
            .code,
        "RESOURCE_HASH"
    );
    asset.content.as_mut().unwrap().sha256 = hash.clone();
    fs::write(&path, b"corrupt image").unwrap();
    assert_eq!(
        read_project_asset_if_changed(&root, &asset, cached)
            .unwrap_err()
            .code,
        "INVALID_PNG"
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        read_project_asset_if_changed(&root, &asset, cached)
            .unwrap_err()
            .code,
        "PROJECT_IO"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn store_png_verification_cache_detects_external_edits() {
    let root = std::env::temp_dir().join(format!(
        "kasane-store-resource-cache-{}",
        std::process::id()
    ));
    let path = root.join("assets/texture.png");
    let (original, hash) = create_test_png(&path);
    let mut document = Document::new();
    assert!(document
        .initialize(id(1), Canvas::new(100.0, 100.0, Vec2::default(), 1.0))
        .is_ok());
    assert!(document
        .add_asset(ImageAsset {
            unresolved_source: None,
            id: id(2),
            name: String::new(),
            content: Some(kasane_core::ImageContent {
                source: "assets/texture.png".into(),
                width: 8,
                height: 8,
                sha256: hash,
            }),
        })
        .status
        .is_ok());
    let store = DocumentStore::new();
    assert!(store.diagnose(&document, &root).is_empty());
    assert!(store.diagnose(&document, &root).is_empty());

    fs::write(&path, b"corrupt image").unwrap();
    assert_eq!(store.diagnose(&document, &root)[0].code, "INVALID_PNG");
    fs::write(&path, &original).unwrap();
    let mut wrong = document.get_asset(&id(2)).unwrap().clone();
    wrong.content.as_mut().unwrap().width = 9;
    assert!(document.replace_asset(wrong).status.is_ok());
    assert_eq!(
        store.diagnose(&document, &root)[0].code,
        "RESOURCE_DIMENSIONS"
    );
    fs::remove_dir_all(root).unwrap();
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
    let temp_dir = std::env::temp_dir().join(format!("kasane-test-png-{}", std::process::id()));
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
fn test_project_encode_decode_roundtrip() {
    let sha1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha2 = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let before = fixture_doc(sha1, sha2);

    let encoded = encode_project(&before).expect("encode_project failed");
    assert_eq!(encoded.bytes().filter(|&byte| byte == b'\n').count(), 1);
    let root: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(root["format"], "kasane-directory-project");
    assert_eq!(root["format_version"], 20);

    let decoded = decode_project(&encoded).expect("decode_project failed");
    assert!(before.same_content(&decoded));
    assert_eq!(decoded.revision(), 1, "decode publishes one batch revision");

    // Both documents evaluate to identical frames
    let mut p = HashMap::new();
    p.insert(id(6), 0.5);

    let mut frame_before = DrawableFrame::default();
    let mut frame_after = DrawableFrame::default();
    assert!(evaluate_frame(&before, &p, &mut frame_before).is_ok());
    assert!(evaluate_frame(&decoded, &p, &mut frame_after).is_ok());

    assert_eq!(frame_before.drawables.len(), frame_after.drawables.len());
    for i in 0..frame_before.drawables.len() {
        let d1 = &frame_before.drawables[i];
        let d2 = &frame_after.drawables[i];
        assert_eq!(d1.positions, d2.positions);
        assert_eq!(d1.uvs, d2.uvs);
        assert_eq!(d1.indices, d2.indices);
        assert_eq!(d1.draw_order, d2.draw_order);
    }

    // Both encode to identical MOC3 bytes
    let moc1 = encode_moc3(&before).unwrap();
    let moc2 = encode_moc3(&decoded).unwrap();
    assert_eq!(moc1.bytes, moc2.bytes);
}

#[cfg(feature = "binary-prototype")]
#[test]
fn cbor_prototype_preserves_project_content_and_rejects_bad_header() {
    use kasane_project::{decode_project_cbor, encode_project_cbor};

    let mut original = fixture_doc(&"0".repeat(64), &"f".repeat(64));
    let id = original.mesh_order()[0].clone();
    assert!(original
        .replace_editor_state(kasane_core::document::EditorState {
            objects: [(
                id,
                kasane_core::document::ObjectEditorState {
                    locked: true,
                    ..Default::default()
                }
            )]
            .into()
        })
        .status
        .is_ok());
    let binary = encode_project_cbor(&original).unwrap();
    let decoded = decode_project_cbor(&binary).unwrap();
    assert!(original.same_content(&decoded));

    let mut invalid = binary.clone();
    invalid[0] = 0;
    assert_eq!(
        decode_project_cbor(&invalid).unwrap_err().code,
        "INVALID_PROJECT"
    );
    assert_eq!(
        decode_project_cbor(&binary[..binary.len() - 1])
            .unwrap_err()
            .code,
        "INVALID_PROJECT"
    );
}

#[test]
fn project_roundtrip_preserves_precise_rotation_origin() {
    let mut document = Document::new();
    assert!(document
        .initialize(id(1), Canvas::new(640.0, 480.0, Vec2::new(0.0, 0.0), 100.0))
        .is_ok());
    let origin = 0.9545455574989319_f64;
    let transform = Transform {
        id: id(2),
        runtime_id: "rotation".into(),
        data: TransformData::Rotation(RotationTransform {
            base_angle: 0.0,
            pose: RotationPose {
                origin: kasane_core::types::PreciseVec2::new(0.06838598102331161, origin),
                ..Default::default()
            },
        }),
        ..Default::default()
    };
    assert!(document.create_transform(transform).status.is_ok());

    let reopened = decode_project(&encode_project(&document).unwrap()).unwrap();
    assert_eq!(
        reopened
            .get_transform(&id(2))
            .unwrap()
            .rotation()
            .unwrap()
            .pose
            .origin
            .y
            .to_bits(),
        origin.to_bits()
    );
    assert!(document.same_content(&reopened));

    #[cfg(feature = "binary-prototype")]
    {
        let binary = kasane_project::encode_project_cbor(&document).unwrap();
        let binary_reopened = kasane_project::decode_project_cbor(&binary).unwrap();
        assert_eq!(
            binary_reopened
                .get_transform(&id(2))
                .unwrap()
                .rotation()
                .unwrap()
                .pose
                .origin
                .y
                .to_bits(),
            origin.to_bits()
        );
        assert!(document.same_content(&binary_reopened));
    }
}

#[test]
fn test_reject_duplicate_keys_and_malformed() {
    // Duplicate JSON key
    let json_dup = r#"{
      "format": "kasane-directory-project",
      "format": "duplicate-key",
      "format_version": 1,
      "document": {}
    }"#;
    let err = decode_project(json_dup).unwrap_err();
    assert_eq!(err.code, "INVALID_PROJECT");

    // Legacy format
    let legacy_json = r#"{
      "format": "kasane-project",
      "format_version": 5,
      "document": {}
    }"#;
    let err = decode_project(legacy_json).unwrap_err();
    assert_eq!(err.code, "LEGACY_PROJECT");

    // Unsupported version
    let bad_version = r#"{
      "format": "kasane-directory-project",
      "format_version": 99,
      "document": {
        "id": "00000001-1111-4111-8111-111111111111",
        "model_space": {"origin": {"x": 0.0, "y": 0.0}, "y_axis_down": true},
        "assets": [],
        "meshes": [],
        "parts": [],
        "transforms": [],
        "parameters": [],
        "bindings": [],
        "scene_bindings": []
      }
    }"#;
    let err = decode_project(bad_version).unwrap_err();
    assert_eq!(err.code, "UNSUPPORTED_VERSION");

    // Invalid asset path (directory traversal)
    let doc = fixture_doc(&"0".repeat(64), &"1".repeat(64));
    let mut traversal: serde_json::Value =
        serde_json::from_str(&encode_project(&doc).unwrap()).unwrap();
    traversal["document"]["assets"][0]["source"] = "../escape.png".into();
    let err = decode_project(&traversal.to_string()).unwrap_err();
    assert_eq!(err.code, "INVALID_PROJECT");
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
