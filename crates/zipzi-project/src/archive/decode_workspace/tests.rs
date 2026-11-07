use super::*;
use crate::codec::{decode_state, encode_state};
use serde_json::{json, Value};
use zipzi_core::state::{DocumentState, OwnedDocument};
use zipzi_core::{
    BindingAxis, BindingAxisEditing, Mesh, MeshBinding, MeshKeyform, Parameter, Vec2,
};

mod allocation;

fn id(n: usize) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn geometry(objects: usize, vertices: usize, forms: usize) -> Vec<u8> {
    let mut input = OwnedDocument {
        id: id(1),
        ..Default::default()
    };
    input.parameters.push(Parameter {
        id: id(2),
        runtime_id: "p".into(),
        minimum: 0.,
        maximum: forms.max(1) as f32,
        default_value: 0.,
        ..Default::default()
    });
    for n in 0..objects {
        let mesh = Mesh {
            id: id(3 + n * 2),
            runtime_id: format!("mesh{n}"),
            vertex_ids: (0..vertices as u32).collect(),
            base_positions: (0..vertices).map(|i| Vec2::new(i as f32, 0.)).collect(),
            uvs: vec![Vec2::default(); vertices],
            ..Default::default()
        };
        if forms > 0 {
            input.bindings.push(MeshBinding {
                id: id(4 + n * 2),
                mesh_id: mesh.id.clone(),
                axes: vec![BindingAxis::new(
                    id(2),
                    (0..forms).map(|i| i as f32).collect(),
                )],
                keyforms: (0..forms)
                    .map(|i| MeshKeyform {
                        keys: vec![i as f32],
                        positions: mesh.base_positions.clone(),
                        ..Default::default()
                    })
                    .collect(),
            });
        }
        input.meshes.push(mesh);
    }
    encode_state(&DocumentState::from_owned(input).unwrap(), vec![]).unwrap()
}

fn with_metadata(value: Value) -> Vec<u8> {
    let mut root: Value = serde_json::from_slice(&geometry(0, 0, 0)).unwrap();
    root["document"]["animation_assets"]["model3_settings"]["extensions"]["calibration"] = value;
    serde_json::to_vec(&root).unwrap()
}

fn motions(clips: usize, tracks: usize, keys: usize) -> Vec<u8> {
    use zipzi_core::document::{
        MotionClip, MotionEvent, MotionKey, MotionSegment, MotionTrack, MotionTrackTarget,
    };
    let bytes = geometry(0, 0, 0);
    let mut input = decode_state(std::str::from_utf8(&bytes).unwrap())
        .unwrap()
        .model;
    let mut next = 100;
    let mut identity = || {
        next += 1;
        id(next)
    };
    for clip in 0..clips {
        input.motions.push(MotionClip {
            id: identity(),
            name: format!("motion{clip}"),
            duration: keys as f32,
            fps: 30.,
            looping: false,
            restricted_beziers: false,
            fade_in: None,
            fade_out: None,
            track_order: vec![],
            tracks: (0..tracks)
                .map(|_| MotionTrack {
                    id: identity(),
                    target: MotionTrackTarget::Parameter {
                        parameter_id: id(2),
                    },
                    initial: MotionKey {
                        id: identity(),
                        time: 0.,
                        value: 0.,
                    },
                    segments: std::sync::Arc::new(
                        (1..=keys)
                            .map(|k| MotionSegment::Linear {
                                end: MotionKey {
                                    id: identity(),
                                    time: k as f32,
                                    value: (k % 2) as f32,
                                },
                            })
                            .collect(),
                    ),
                    fade_in: None,
                    fade_out: None,
                    extensions: Default::default(),
                })
                .collect::<Vec<_>>()
                .into(),
            events: vec![MotionEvent {
                id: identity(),
                time: 0.5,
                value: "event".into(),
                extensions: Default::default(),
            }]
            .into(),
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        });
    }
    encode_state(&DocumentState::from_owned(input).unwrap(), vec![]).unwrap()
}

fn calibrate(name: &str, input: &[u8], expected_error: Option<&str>) {
    let estimate = document_decode_workspace(input, &mut || true).unwrap();
    let (result, decoded_usage) = allocation::measure(0, || {
        // The input buffer is part of the estimate. Fixture generation is not.
        let bytes = input.to_vec();
        decode_state(std::str::from_utf8(&bytes).unwrap())
    });
    println!(
        "{name}: input={}, decode_peak={}, estimate={estimate}",
        input.len(),
        decoded_usage.peak
    );
    assert!(
        decoded_usage.peak <= estimate,
        "{name}: {decoded_usage:?} exceeds {estimate}"
    );
    if let Some(code) = expected_error {
        assert_eq!(result.err().expect("invalid fixture accepted").code, code);
    } else {
        let decoded = result.unwrap();
        let freeze_estimate = document_freeze_workspace(&decoded.model);
        assert!(decoded_usage.live >= 0);
        let (frozen, freeze_usage) = allocation::measure(decoded_usage.live as usize, || {
            DocumentState::from_owned(decoded.model)
        });
        frozen.unwrap();
        println!(
            "{name}: freeze_peak={}, estimate={freeze_estimate}",
            freeze_usage.peak
        );
        assert!(
            freeze_usage.peak <= freeze_estimate,
            "{name}: freeze {freeze_usage:?} exceeds {freeze_estimate}"
        );
    }
}

#[test]
fn decode_estimate_covers_numeric_strings_and_metadata_at_multiple_sizes() {
    calibrate("empty", &geometry(0, 0, 0), None);
    // Include both sides of Vec growth boundaries, not only exact capacities.
    for count in [128, 129, 8192, 8193] {
        calibrate(
            &format!("numeric_{count}"),
            &with_metadata(json!(vec![0; count])),
            None,
        );
        // Quotes, backslashes, punctuation and multibyte UTF-8 must remain inside
        // strings in the scanner; serde also needs scratch for escaped values.
        calibrate(
            &format!("escaped_strings_{count}"),
            &with_metadata(json!("中文\\\"{},[]:".repeat(count))),
            None,
        );
        let members: serde_json::Map<String, Value> = (0..count)
            .map(|i| (format!("key{i}"), json!({"x": i, "tag": "label"})))
            .collect();
        calibrate(
            &format!("metadata_objects_{count}"),
            &with_metadata(Value::Object(members)),
            None,
        );
    }
    let mut padded = vec![b' '; 1 << 20];
    padded.extend(geometry(0, 0, 0));
    calibrate("whitespace", &padded, None);
}

#[test]
fn decode_estimate_covers_geometry_and_keyforms_at_multiple_sizes() {
    for (objects, vertices, forms) in [
        (1, 128, 0),
        (1, 8192, 0),
        (1, 8193, 0),
        (8, 16, 4),
        (64, 128, 8),
    ] {
        calibrate(
            &format!("geometry_{objects}_{vertices}_{forms}"),
            &geometry(objects, vertices, forms),
            None,
        );
    }
}

#[test]
fn decode_estimate_covers_motion_tracks_and_keys_at_multiple_sizes() {
    for (clips, tracks, keys) in [(1, 1, 4), (8, 4, 64)] {
        calibrate(
            &format!("motions_{clips}_{tracks}_{keys}"),
            &motions(clips, tracks, keys),
            None,
        );
    }
}

#[test]
fn decode_estimate_covers_rejected_documents() {
    let mut root: Value = serde_json::from_slice(&geometry(0, 0, 0)).unwrap();
    root["unknown"] = json!(vec![0; 32768]);
    calibrate(
        "unknown_dense_field",
        &serde_json::to_vec(&root).unwrap(),
        Some("UNSUPPORTED_VERSION"),
    );
    calibrate(
        "duplicate_member",
        br#"{"format":"zipzi-project","format":"zipzi-project"}"#,
        Some("INVALID_PROJECT"),
    );
    let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
    calibrate("excessive_depth", deep.as_bytes(), Some("INVALID_PROJECT"));
}

#[test]
fn scanner_ignores_string_punctuation_and_checks_cancellation_between_chunks() {
    for string in [
        "{}[],:".to_owned(),
        format!("{}\\\"{{}}[],:", "x".repeat((64 << 10) - 2)),
    ] {
        let literal = serde_json::to_vec(&string).unwrap();
        assert_eq!(
            document_decode_workspace(&literal, &mut || true).unwrap(),
            literal.len() + (literal.len() - 1) * STRING_BYTE_ALLOWANCE
        );
    }
    let bytes = vec![b' '; (64 << 10) + 1];
    let mut checks = 0;
    let error = document_decode_workspace(&bytes, &mut || {
        checks += 1;
        checks < 2
    })
    .unwrap_err();
    assert!(matches!(error, super::super::ArchiveError::Cancelled));
    assert_eq!(checks, 2);
}
