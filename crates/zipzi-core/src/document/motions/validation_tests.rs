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

fn fixture() -> Document {
    let mut d = Document::new();
    assert!(d.initialize(id(1), ModelSetup::default()).is_ok());
    assert!(
        d.create_parameter(Parameter {
            id: id(2),
            ..Default::default()
        })
        .status
        .is_ok()
    );
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
        let plan = d.plan_physics_edit(intent).unwrap();
        assert!(d.apply_physics_plan(&plan).status.is_ok());
    }
    let node = d
        .get_physics_group(&id(11))
        .unwrap()
        .vertices
        .last()
        .unwrap()
        .id
        .clone();
    let plan = d
        .plan_physics_edit(PhysicsEdit::CreateOutput {
            group_id: id(11),
            id: id(13),
            node_id: node,
            before: None,
        })
        .unwrap();
    assert!(d.apply_physics_plan(&plan).status.is_ok());
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
        let plan = d.plan_pose_edit(intent).unwrap();
        assert!(d.apply_pose_plan(&plan).status.is_ok());
    }
    d.content.package_attachments.push(PackageAttachment {
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
    d.content.motion_groups.push(super::MotionGroup {
        id: id(31),
        name: "Group".into(),
        entries: vec![registration(32)],
    });
    d.content.motion_roots.push(registration(33));
    d
}

#[test]
fn bulk_motion_validation_retains_collisions_with_nested_document_identities() {
    let d = fixture();
    let valid = clip();
    assert!(d.validate_motion(&valid).is_ok());
    let reserved = d.base_identity_set();
    for identity in reserved {
        assert!(
            d.contains_base_identity(identity),
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
        assert!(d.base_identity_set().contains(id(n).as_str()));
    }
}

#[test]
fn bulk_motion_validation_excludes_only_the_current_clips_nested_keys() {
    let mut d = fixture();
    let original = clip();
    d.content
        .motions
        .insert(original.id.clone(), std::sync::Arc::new(original.clone()));
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
