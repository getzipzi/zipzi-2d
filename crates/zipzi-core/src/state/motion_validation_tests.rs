use super::*;
use crate::document::{PackageAttachment, PhysicsEdit, PoseEdit};
use crate::{ModelSetup, Parameter};
use serde_json::json;

fn id(n: u32) -> String {
    format!("30000000-0000-4000-8000-{n:012x}")
}

fn clip() -> MotionClip {
    serde_json::from_value(json!({
        "id": id(100), "name": "Motion", "duration": 1., "fps": 30.,
        "looping": false, "restricted_beziers": false,
        "fade_in": null, "fade_out": null, "track_order": [],
        "tracks": [{"id": id(101), "target": {"kind": "parameter", "parameter_id": id(2)},
            "initial": {"id": id(102), "time": 0., "value": 0.},
            "segments": [], "fade_in": null, "fade_out": null, "extensions": {}}],
        "events": [{"id": id(103), "time": 0., "value": "cue", "extensions": {}}],
        "extensions": {}, "meta_extensions": {}, "opaque_source_ids": null
    }))
    .unwrap()
}

fn fixture() -> DocumentBuilder {
    let mut d = DocumentState::new(id(1), ModelSetup::default())
        .unwrap()
        .builder();
    assert!(d
        .create_parameter(Parameter {
            id: id(2),
            ..Default::default()
        })
        .is_ok());
    for intent in [
        PhysicsEdit::CreateGroup {
            asset_id: id(10),
            id: id(11),
            name: "Rig".into(),
            before: None,
        },
        PhysicsEdit::CreateInput {
            group_id: id(11),
            id: id(12),
            before: None,
        },
    ] {
        d.edit_physics(intent).unwrap();
    }
    let node = d
        .physics()
        .as_ref()
        .and_then(|a| a.data.settings.iter().find(|g| g.id == id(11)))
        .unwrap()
        .vertices
        .last()
        .unwrap()
        .id
        .clone();
    d.edit_physics(PhysicsEdit::CreateOutput {
        group_id: id(11),
        id: id(13),
        node_id: node,
        before: None,
    })
    .unwrap();
    for intent in [
        PoseEdit::CreateGroup {
            pose_id: id(20),
            id: id(21),
            name: "Pose".into(),
            before: None,
        },
        PoseEdit::CreateEntry {
            group_id: id(21),
            id: id(22),
            before: None,
        },
        PoseEdit::CreateLink {
            entry_id: id(22),
            id: id(23),
            before: None,
        },
    ] {
        d.edit_pose(intent).unwrap();
    }
    Arc::make_mut(&mut d.content.package_attachments).push(PackageAttachment {
        id: id(30),
        path: "sound.wav".into(),
        bytes: None,
    });
    let registration = |n| super::MotionRegistration {
        id: id(n),
        clip_id: id(100),
        fade_in: None,
        fade_out: None,
        sound: None,
        extensions: Default::default(),
    };
    Arc::make_mut(&mut d.content.motion_groups).push(super::MotionGroup {
        id: id(31),
        name: "Group".into(),
        entries: vec![registration(32)],
    });
    Arc::make_mut(&mut d.content.motion_roots).push(registration(33));
    d
}

#[test]
fn bulk_motion_validation_retains_collisions_with_nested_document_identities() {
    let d = fixture();
    let valid = clip();
    assert!(d.validate_motion(&valid).is_ok());
    let model = d.content.model();
    let reserved = model.base_identity_set();
    for identity in reserved {
        assert!(
            d.content.model().contains_id(identity),
            "unexpected identity {identity}"
        );
        for kind in 0..3 {
            let mut candidate = valid.clone();
            match kind {
                0 => candidate.tracks[0].id = identity.into(),
                1 => candidate.tracks[0].initial.id = identity.into(),
                _ => candidate.events[0].id = identity.into(),
            }
            assert_eq!(
                d.validate_motion(&candidate).code,
                "DUPLICATE_ID",
                "{identity} kind {kind}"
            );
        }
    }
    for n in [1, 2, 10, 11, 12, 13, 20, 21, 22, 23, 30, 31, 32, 33] {
        assert!(d
            .content
            .model()
            .base_identity_set()
            .contains(id(n).as_str()));
    }
}

#[test]
fn bulk_motion_validation_excludes_only_the_current_clips_nested_keys() {
    let mut d = fixture();
    let original = clip();
    d.create_motion(original.clone()).unwrap();
    assert!(d.validate_motion(&original).is_ok());
    let mut other = original.clone();
    other.id = id(200);
    other.name = "Other".into();
    assert_eq!(d.validate_motion(&other).code, "DUPLICATE_ID");
    other.tracks[0].id = id(201);
    other.tracks[0].initial.id = id(202);
    other.events[0].id = id(203);
    assert!(d.validate_motion(&other).is_ok());
    other.tracks[0].initial.time = f32::NAN;
    assert_eq!(d.validate_motion(&other).code, "INVALID_MOTION_POINT");
}

#[test]
fn motion_curve_validation_checks_frame_counts_and_preset_provenance() {
    use crate::document::{
        MotionCurve, MotionEasingPreset, MotionFrameStep, MotionKey, MotionSegment,
    };
    let d = fixture();
    let mut clip = clip();
    let start = clip.tracks[0].initial.point();
    let end = MotionKey {
        id: id(110),
        time: 1.,
        value: 1.,
    };
    let [control1, control2] = MotionEasingPreset::EaseIn
        .mapped_controls(start, end.point())
        .unwrap();
    clip.tracks[0].segments = Arc::new(vec![MotionSegment::new(
        end,
        MotionCurve::Bezier {
            control1,
            control2,
            preset: Some(MotionEasingPreset::EaseIn),
        },
    )]);
    assert!(d.validate_motion(&clip).is_ok());
    let mut bad = clip.clone();
    Arc::make_mut(&mut bad.tracks[0].segments)[0].frame_step =
        MotionFrameStep::EveryFrames { frames: 0 };
    assert_eq!(d.validate_motion(&bad).code, "INVALID_MOTION_FRAME_STEP");
    let mut bad = clip.clone();
    let MotionCurve::Bezier { control1, .. } =
        &mut Arc::make_mut(&mut bad.tracks[0].segments)[0].curve
    else {
        panic!()
    };
    control1.value += 0.01;
    assert_eq!(d.validate_motion(&bad).code, "INVALID_MOTION_EASING_PRESET");
    clip.restricted_beziers = true;
    assert_eq!(d.validate_motion(&clip).code, "RESTRICTED_BEZIER_PRESET");
    Arc::make_mut(&mut clip.tracks[0].segments)[0].curve = MotionCurve::AutoSmooth {};
    assert!(d.validate_motion(&clip).is_ok());
}
