use super::common::*;
use std::sync::Arc;
use zipzi_animation::MotionPreview;
use zipzi_core::{Document, ImageAsset, Mesh, Parameter, Part, Vec2};
use zipzi_project::{import_motion3, import_pose3};

#[test]
fn pose_virtual_controls_follow_official_motion_and_pose_frames() {
    let mut doc = document();
    assert!(doc
        .create_parameter(Parameter {
            id: "00000000-0000-4000-8000-000000000d05".into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let part0 = "00000000-0000-4000-8000-000000000d06";
    let part1 = "00000000-0000-4000-8000-000000000d07";
    for (id, runtime) in [(part0, "Part0"), (part1, "Part1")] {
        assert!(doc
            .create_part(Part {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let asset = "00000000-0000-4000-8000-000000000d09";
    assert!(doc
        .add_asset(ImageAsset {
            unresolved_source: None,
            id: asset.into(),
            name: String::new(),
            content: Some(zipzi_core::ImageContent {
                source: "memory://test".into(),
                width: 8,
                height: 8,
                ..Default::default()
            }),
        })
        .status
        .is_ok());
    for (index, part_id) in [part0, part1].iter().enumerate() {
        assert!(doc
            .create_mesh(Mesh {
                id: format!("00000000-0000-4000-8000-000000000d1{index}"),
                texture_asset_id: asset.into(),
                part_id: (*part_id).into(),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0)
                ],
                uvs: vec![Vec2::new(0.0, 0.0); 3],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let imported = import_motion3(&doc, MOTION, "Swap", MINIMAL_MOTION3).unwrap();
    assert!(imported.diagnostics.is_empty());
    let imported = import_pose3(
        &imported.candidate,
        "00000000-0000-4000-8000-000000000d08",
        MINIMAL_POSE3,
    )
    .unwrap();
    assert!(imported.diagnostics.is_empty());
    let mut preview = MotionPreview::new(&imported.candidate);
    assert_eq!(preview.snapshot().part_opacities[part0], 1.0);
    assert_eq!(preview.snapshot().part_opacities[part1], 0.0);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    for (time, part0_expected, part1_expected, model_expected, param_expected) in [
        (0.25, 0.7, 0.5, 0.3, 0.0),
        (0.5, 0.0, 1.0, 0.4, 0.25),
        (0.75, 0.0, 1.0, 0.5, 0.5),
    ] {
        let state = preview.advance(0.25).unwrap();
        assert!((state.time - time).abs() < 0.00001);
        assert!((state.part_opacities[part0] - part0_expected).abs() < 0.00001);
        assert!((state.part_opacities[part1] - part1_expected).abs() < 0.00001);
        assert!((state.model_opacity - model_expected).abs() < 0.00001);
        assert!((state.parameters[PARAM] - param_expected).abs() < 0.00001);
        let frame = preview.evaluate_drawables().unwrap();
        assert!((frame.drawables[0].opacity - part0_expected).abs() < 0.00001);
        assert!((frame.drawables[1].opacity - part1_expected).abs() < 0.00001);
    }
}

#[test]
fn pose_uses_real_parameter_when_part_runtime_id_matches() {
    let mut doc = document();
    let part0 = "00000000-0000-4000-8000-000000000d06";
    let part1 = "00000000-0000-4000-8000-000000000d07";
    let control = "00000000-0000-4000-8000-000000000d05";
    assert!(doc
        .create_parameter(Parameter {
            id: control.into(),
            runtime_id: "Part0".into(),
            name: "Control".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    for (id, runtime) in [(part0, "Part0"), (part1, "Part1")] {
        assert!(doc
            .create_part(Part {
                id: id.into(),
                runtime_id: runtime.into(),
                name: runtime.into(),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let imported = import_motion3(&doc, MOTION, "Swap", MINIMAL_MOTION3).unwrap();
    let imported = import_pose3(
        &imported.candidate,
        "00000000-0000-4000-8000-000000000d08",
        MINIMAL_POSE3,
    )
    .unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    assert_eq!(preview.snapshot().parameters[control], 1.0);
    assert!(!preview.snapshot().part_opacity_channels.contains_key(part0));
    preview.schedule_motion(MOTION, 0.0).unwrap();
    let state = preview.advance(0.25).unwrap();
    assert_eq!(state.parameters[control], 0.0);
    assert_eq!(state.part_opacity_channels[part1], 1.0);
    assert!((state.part_opacities[part0] - 0.7).abs() < 0.00001);
    assert!((state.part_opacities[part1] - 0.5).abs() < 0.00001);
}

#[test]
fn non_looping_motion_uses_natural_end_for_clip_and_track_fades() {
    for track_fade in [None, Some(0.5)] {
        let mut source: serde_json::Value = serde_json::from_str(LOOP_MOTION3).unwrap();
        source["Meta"]["Loop"] = false.into();
        source["Meta"]["FadeInTime"] = 0.0.into();
        source["Meta"]["FadeOutTime"] = 1.0.into();
        if let Some(fade) = track_fade {
            source["Curves"][0]["FadeOutTime"] = fade.into();
        }
        let imported = import_motion3(&document(), MOTION, "Fade", &source.to_string()).unwrap();
        let mut preview = MotionPreview::new(&imported.candidate);
        preview.schedule_motion(MOTION, 0.0).unwrap();
        preview.advance(0.0).unwrap();
        let weight = if track_fade.is_some() { 1.0 } else { 0.5 };
        assert!((preview.advance(0.5).unwrap().parameters[PARAM] - 0.5 * weight).abs() < 0.00001);
    }
}

#[test]
fn part_opacity_tracks_clamp_real_parameters() {
    let mut doc = document();
    let part = "00000000-0000-4000-8000-000000000d06";
    assert!(doc
        .create_part(Part {
            id: part.into(),
            runtime_id: "ParamX".into(),
            name: "Part".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut source: serde_json::Value = serde_json::from_str(LOOP_MOTION3).unwrap();
    source["Curves"][0]["Target"] = "PartOpacity".into();
    source["Curves"][0]["Segments"] = serde_json::json!([0, 2, 0, 1, 2]);
    let imported = import_motion3(&doc, MOTION, "Clamp", &source.to_string()).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    assert_eq!(preview.advance(0.0).unwrap().parameters[PARAM], 1.0);
}

#[test]
fn loop_events_survive_multiple_cycles_and_failed_advance_is_atomic() {
    let mut source: serde_json::Value = serde_json::from_str(LOOP_MOTION3).unwrap();
    source["UserData"] = serde_json::json!([{"Time":0.5,"Value":"tick"}]);
    source["Meta"]["UserDataCount"] = 1.into();
    source["Meta"]["TotalUserDataSize"] = 4.into();
    let imported = import_motion3(&document(), MOTION, "Events", &source.to_string()).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    preview.advance(0.0).unwrap();
    assert_eq!(preview.advance(0.75).unwrap().fired_events.len(), 1);
    assert_eq!(preview.advance(3.0).unwrap().fired_events.len(), 2);
    assert!(preview.advance(0.0).unwrap().fired_events.is_empty());
    let before = preview.snapshot().clone();
    assert_eq!(
        preview.advance(f32::MAX).unwrap_err(),
        zipzi_animation::AnimationError::EventLimit
    );
    assert_eq!(preview.snapshot(), &before);
}

#[test]
fn editing_one_track_preserves_shared_snapshots_and_other_track_storage() {
    let imported = import_motion3(&document(), MOTION, "Shared", LOOP_MOTION3).unwrap();
    let mut original = imported.candidate;
    let mut clip = original.get_motion(MOTION).unwrap().clone();
    let mut second = clip.tracks[0].clone();
    second.id = "00000000-0000-4000-8000-000000000d99".into();
    second.initial.id = uuid::Uuid::new_v4().to_string();
    for segment in Arc::make_mut(&mut second.segments) {
        segment.end_mut().id = uuid::Uuid::new_v4().to_string();
    }
    clip.tracks.push(second);
    assert!(original.replace_motion(clip).status.is_ok());
    let saved = original.checkpoint();
    let mut candidate = original.fork_candidate();
    assert!(std::ptr::eq(
        original.get_motion(MOTION).unwrap(),
        candidate.get_motion(MOTION).unwrap()
    ));
    let mut clip = candidate.get_motion(MOTION).unwrap().clone();
    let before = original.get_motion(MOTION).unwrap();
    let segment = &mut Arc::make_mut(&mut clip.tracks[0].segments)[0];
    let key_id = segment.end().id.clone();
    *segment = zipzi_core::document::MotionSegment::Linear {
        end: zipzi_core::document::MotionKey {
            id: key_id,
            time: 1.0,
            value: 0.25,
        },
    };
    assert!(!Arc::ptr_eq(
        &clip.tracks[0].segments,
        &before.tracks[0].segments
    ));
    assert!(Arc::ptr_eq(
        &clip.tracks[1].segments,
        &before.tracks[1].segments
    ));
    assert!(candidate.replace_motion(clip).status.is_ok());
    assert_eq!(
        original.get_motion(MOTION).unwrap().tracks[0].segments[0]
            .end()
            .value,
        1.0
    );
    assert_eq!(
        candidate.get_motion(MOTION).unwrap().tracks[0].segments[0]
            .end()
            .value,
        0.25
    );
    assert!(saved.estimated_bytes() > 0);
}

fn registered_document(track_fades: bool) -> Document {
    use zipzi_core::document::{MotionGroup, MotionRegistration};
    let mut curve =
        serde_json::json!({"Target":"Parameter", "Id":"ParamX", "Segments":[0,1,0,4,1]});
    if track_fades {
        curve["FadeInTime"] = 0.into();
        curve["FadeOutTime"] = 0.into();
    }
    let source = serde_json::json!({"Version":3,"Meta":{"Duration":4,"Fps":30,"Loop":true,
        "AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":1,"TotalPointCount":2,
        "FadeInTime":2,"FadeOutTime":2,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[curve]});
    let mut doc = import_motion3(&document(), MOTION, "Registered", &source.to_string())
        .unwrap()
        .candidate;
    assert!(doc
        .set_motion_groups(vec![MotionGroup {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Idle".into(),
            entries: [
                (None, None),
                (Some(0.0), Some(0.0)),
                (Some(1.0), Some(0.25))
            ]
            .into_iter()
            .map(|(fade_in, fade_out)| MotionRegistration {
                id: uuid::Uuid::new_v4().to_string(),
                clip_id: MOTION.into(),
                fade_in,
                fade_out,
                sound: None,
                extensions: Default::default(),
            })
            .collect()
        }])
        .status
        .is_ok());
    doc
}

#[test]
fn registration_fades_are_per_activation_and_track_fades_take_precedence() {
    for (entry, expected) in [(0, 0.14644662), (1, 1.0), (2, 0.5)] {
        let doc = registered_document(false);
        let mut preview = MotionPreview::new(&doc);
        preview.schedule_motion_entry("Idle", entry, 0.0).unwrap();
        preview.advance(0.0).unwrap();
        assert!((preview.advance(0.5).unwrap().parameters[PARAM] - expected).abs() < 1e-6);
        assert_eq!(doc.get_motion(MOTION).unwrap().fade_in, Some(2.0));
    }
    let mut preview = MotionPreview::new(&registered_document(true));
    preview.schedule_motion_entry("Idle", 2, 0.0).unwrap();
    assert_eq!(preview.advance(0.0).unwrap().parameters[PARAM], 1.0);

    // The same clip's outgoing registration must keep its own short fade-out.
    let mut preview = MotionPreview::new(&registered_document(false));
    preview.schedule_motion_entry("Idle", 2, 0.0).unwrap();
    preview.schedule_motion_entry("Idle", 0, 1.0).unwrap();
    preview.advance(0.0).unwrap();
    preview.advance(1.0).unwrap();
    assert_eq!(preview.snapshot().active_motions.len(), 2);
    assert_eq!(preview.advance(0.3).unwrap().active_motions.len(), 1);
    let mut preview = MotionPreview::new(&registered_document(false));
    preview.schedule_motion_entry("Idle", 0, 0.0).unwrap();
    preview.schedule_motion_entry("Idle", 1, 1.0).unwrap();
    preview.advance(0.0).unwrap();
    preview.advance(1.0).unwrap();
    assert_eq!(preview.advance(0.3).unwrap().active_motions.len(), 2);
}
