mod support;

use serde_json::{json, Value};
use std::fs;
use support::*;
use zipzi_project::{export_motion3, ProjectIo, ResourceStore};

// Written and exported by the v0.5.0-alpha.1 release with an opaque motion3 field.
const V22: &[u8] = include_bytes!("../../../fixtures/animation_curves/v22-motion.zipzi");
const MOTION3: &str = include_str!("../../../fixtures/animation_curves/v22-motion.motion3.json");
const MOTION: &str = "00000000-0000-4000-8000-000000000c03";
const TRACK: &str = "/document/animation_assets/motions/0/tracks/0";

#[test]
fn v22_motion_keeps_key_identity_playback_and_export_through_v23_save() {
    let original = project_json(V22);
    let document = load_native(V22).unwrap();
    let saved = native_bytes(&document);
    let upgraded = project_json(&saved);
    assert_eq!(original["format_version"], 22);
    assert_eq!(upgraded["format_version"], 23);
    let before = original.pointer(TRACK).unwrap();
    let after = upgraded.pointer(TRACK).unwrap();
    assert_eq!(before["id"], after["id"]);
    assert_eq!(before["initial"], after["initial"]);
    let old_segments = before["segments"].as_array().unwrap();
    let new_segments = after["segments"].as_array().unwrap();
    assert_eq!(new_segments.len(), old_segments.len());
    for (old, new) in old_segments.iter().zip(new_segments) {
        assert_eq!(new["end"], old["end"]);
        assert_eq!(new["curve"]["kind"], old["kind"]);
        assert_eq!(new["frame_step"], json!({"kind":"off"}));
        if old["kind"] == "bezier" {
            assert_eq!(new["curve"]["control1"], old["control1"]);
            assert_eq!(new["curve"]["control2"], old["control2"]);
            assert_eq!(new["curve"]["preset"], Value::Null);
        }
    }
    let reopened = load_native(&saved).unwrap();
    assert_eq!(document.motion(MOTION), reopened.motion(MOTION));
    let clip = reopened.motion(MOTION).unwrap();
    let digest = clip.opaque_source_content_hash.as_deref().unwrap();
    assert_eq!(digest.len(), 64);
    assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));

    let curve = zipzi_animation::CompiledCurve::from_track(&clip.tracks[0], clip.fps);
    // Subframes and exact segment boundaries sampled by the same v22 release.
    for (t, expected) in [
        (0., 0.),
        (0.125, 0.5),
        (0.25, 1.0000004),
        (0.375, 1.05),
        (0.5, 0.5),
        (0.625, 0.5),
        (0.749, 0.5),
        (0.75, 0.9),
        (0.875, 0.9),
        (1., 0.9),
    ] {
        let actual = zipzi_animation::sample_motion_curve(&curve, t, clip.restricted_beziers);
        assert!(
            (actual - expected).abs() < 1e-6,
            "time={t}, actual={actual}"
        );
    }
    let exported: Value =
        serde_json::from_str(&export_motion3(&reopened, MOTION).unwrap()).unwrap();
    assert_eq!(exported, serde_json::from_str::<Value>(MOTION3).unwrap());
    for path in [
        "/document/animation_assets/motion_roots",
        "/document/animation_assets/motions/0/events",
    ] {
        assert_eq!(
            upgraded.pointer(path).unwrap(),
            original.pointer(path).unwrap()
        );
    }
    assert_eq!(project_json(&native_bytes(&reopened)), upgraded);
}

#[test]
fn historical_v22_projects_upgrade_in_place_with_identical_resources() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/observe_inspection/projects");
    let temp = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(temp.path(), ResourceStore::default());
    for name in [
        "masked-offscreen.zipzi",
        "pixel-binding.zipzi",
        "rotated-warp.zipzi",
    ] {
        let original = fs::read(fixtures.join(name)).unwrap();
        let path = temp.path().join(name);
        fs::write(&path, &original).unwrap();
        let old = io.load(&path, || true).unwrap();
        assert!(old.diagnostics.is_empty());
        assert_eq!(fs::read(&path).unwrap(), original);
        io.save(
            &old.document,
            &old.resources,
            &path,
            Some(&old.metadata.fingerprint),
            || true,
        )
        .unwrap();
        let mut expected = project_json(&original);
        assert_eq!(expected["format_version"], 22);
        expected["format_version"] = 23.into();
        let saved = fs::read(&path).unwrap();
        let actual = project_json(&saved);
        assert_eq!(actual, expected, "{name}");
        let reopened = io.load(&path, || true).unwrap();
        assert!(reopened.diagnostics.is_empty());
        assert!(reopened.stats.unique_assets > 0);
        assert_eq!(reopened.stats.unique_assets, old.stats.unique_assets);
        assert_eq!(reopened.stats.asset_bytes, old.stats.asset_bytes);
    }
}

#[test]
fn each_native_version_enforces_its_own_segment_schema() {
    let original = project_json(V22);
    let current = project_json(&native_bytes(&load_native(V22).unwrap()));
    let segment_path = format!("{TRACK}/segments/0");
    let old_segment = original.pointer(&segment_path).unwrap();
    let new_segment = current.pointer(&segment_path).unwrap();
    let end = &old_segment["end"];
    let mut missing_id = old_segment.clone();
    missing_id["end"].as_object_mut().unwrap().remove("id");
    for (base, replacement) in [
        (&original, missing_id),
        (&original, json!({"kind":"auto_smooth","end":end})),
        (&original, new_segment.clone()),
        (
            &original,
            json!({"kind":"linear","frame_step":{"kind":"off"},"end":end}),
        ),
        (&current, old_segment.clone()),
    ] {
        let mut invalid = base.clone();
        *invalid.pointer_mut(&segment_path).unwrap() = replacement;
        assert_eq!(
            load_native(&replace_project_json(V22, &invalid))
                .unwrap_err()
                .code,
            "INVALID_PROJECT"
        );
    }
    for version in [21, 24] {
        let mut unsupported = original.clone();
        unsupported["format_version"] = version.into();
        assert_eq!(
            load_native(&replace_project_json(V22, &unsupported))
                .unwrap_err()
                .code,
            "UNSUPPORTED_VERSION"
        );
    }
}

#[test]
fn v22_opaque_provenance_survives_edits_and_restoration() {
    let mut old = project_json(V22);
    old.pointer_mut(TRACK).unwrap()["initial"]["value"] = 0.125.into();
    let imported = load_native(&replace_project_json(V22, &old)).unwrap();
    let reopened = roundtrip(&imported);
    assert_eq!(
        export_motion3(&reopened, MOTION).unwrap_err().code,
        "OPAQUE_CONTENT_CHANGED"
    );
    let mut clip = reopened.motion(MOTION).unwrap().clone();
    clip.tracks[0].initial.value = 0.;
    let restored = prepare(&reopened, |builder| builder.replace_motion(clip.clone()));
    assert!(export_motion3(&restored, MOTION).is_ok());
    std::sync::Arc::make_mut(&mut clip.tracks[0].segments)[0].frame_step =
        zipzi_contract::motion_curve::MotionFrameStep::EveryFrames { frames: 2 };
    let stepped = prepare(&restored, |builder| builder.replace_motion(clip));
    assert_eq!(
        export_motion3(&stepped, MOTION).unwrap_err().code,
        "OPAQUE_CONTENT_CHANGED"
    );

    let mut old = project_json(V22);
    old["document"]["parameters"][0]["runtime_id"] = "Renamed".into();
    let renamed = load_native(&replace_project_json(V22, &old)).unwrap();
    assert_eq!(
        export_motion3(&roundtrip(&renamed), MOTION)
            .unwrap_err()
            .code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}
