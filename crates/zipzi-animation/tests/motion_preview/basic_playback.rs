use super::common::*;
use zipzi_animation::{MotionOperationKind, MotionPreview};
use zipzi_core::Parameter;
use zipzi_project::{import_expression3, import_motion3, import_physics3};

#[test]
fn operation_identity_distinguishes_zero_time_actions_and_failed_seek() {
    let mut preview = MotionPreview::new(&document());
    let preview_id = preview.operation().preview_id.clone();
    assert_eq!(preview.operation().sequence, 0);
    assert!(matches!(
        preview.operation().kind,
        MotionOperationKind::Created
    ));
    preview.reset();
    assert_eq!(preview.operation().sequence, 1);
    assert!(matches!(
        preview.operation().kind,
        MotionOperationKind::Reset
    ));
    preview.advance(0.0).unwrap();
    assert_eq!(preview.operation().sequence, 2);
    assert!(matches!(
        preview.operation().kind,
        MotionOperationKind::Advance { dt: 0.0 }
    ));
    preview.seek(0.0).unwrap();
    assert_eq!(preview.operation().sequence, 3);
    assert!(matches!(
        preview.operation().kind,
        MotionOperationKind::Seek {
            requested_time: 0.0
        }
    ));
    let before_failure = preview.operation().clone();
    assert!(preview.seek(f32::NAN).is_err());
    assert_eq!(preview.operation(), &before_failure);
    preview.stabilize_physics();
    assert_eq!(preview.operation().sequence, 4);
    assert_eq!(preview.operation().preview_id, preview_id);
    assert!(matches!(
        preview.operation().kind,
        MotionOperationKind::StabilizePhysics
    ));
}

#[test]
fn physics_inputs_replay_with_seek_without_editing_document() {
    let mut doc = document();
    let y = "00000000-0000-4000-8000-000000000d0a";
    assert!(doc
        .create_parameter(Parameter {
            id: y.into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    let physics = import_physics3(
        &doc,
        "00000000-0000-4000-8000-000000000d0b",
        THIRTY_PHYSICS3,
    )
    .unwrap()
    .candidate;
    let revision = physics.revision();
    let mut preview = MotionPreview::new(&physics);
    let mut time = 0.0f32;
    for (dt, input) in [
        (1.0 / 60.0, 0.0),
        (1.0 / 60.0, 1.0),
        (1.0 / 60.0, 1.0),
        (1.0 / 60.0, -1.0),
        (1.0 / 30.0, -1.0),
        (0.1, 0.0),
    ] {
        time += dt;
        preview
            .schedule_parameter_input(PARAM, time, input)
            .unwrap();
    }
    let first = preview.seek(time).unwrap().parameters[y];
    let second = preview.seek(time).unwrap().parameters[y];
    assert_eq!(first, second);
    assert_eq!(physics.revision(), revision);
}

#[test]
fn expression_then_physics_replays_in_one_preview() {
    let mut doc = document();
    let y = "00000000-0000-4000-8000-000000000d0c";
    assert!(doc
        .create_parameter(Parameter {
            id: y.into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    let expression_id = "00000000-0000-4000-8000-000000000d0d";
    let source = r#"{"Type":"Live2D Expression","FadeInTime":0,"Parameters":[{"Id":"ParamX","Value":0.75,"Blend":"Add"}]}"#;
    let doc = import_expression3(&doc, expression_id, "Add", source)
        .unwrap()
        .candidate;
    let doc = import_physics3(
        &doc,
        "00000000-0000-4000-8000-000000000d0e",
        MISSING_PHYSICS3,
    )
    .unwrap()
    .candidate;
    let mut preview = MotionPreview::new(&doc);
    preview.schedule_expression(expression_id, 0.0).unwrap();
    assert_eq!(preview.advance(1.0 / 60.0).unwrap().parameters[PARAM], 0.75);
    preview.advance(1.0 / 60.0).unwrap();
    let third = preview.advance(1.0 / 60.0).unwrap().clone();
    assert_eq!(third.active_expressions, vec![expression_id.to_string()]);
    assert!(third.parameters[y].abs() > 0.01);
    let first_seek = preview.seek(0.05).unwrap().parameters[y];
    assert_eq!(preview.seek(0.05).unwrap().parameters[y], first_seek);
}

#[test]
fn loop_v2_matches_official_framework_frames() {
    let imported = import_motion3(&document(), MOTION, "Loop", LOOP_MOTION3).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    for (dt, expected) in [
        (0.25, 0.0),
        (0.25, 0.25),
        (0.5, 0.75),
        (0.25, 1.0),
        (0.25, 0.5),
        (0.25, 1.0),
        (0.5, 0.5),
    ] {
        let state = preview.advance(dt).unwrap();
        assert!(
            (state.parameters[PARAM] - expected).abs() < 0.00002,
            "time={} actual={} expected={expected}",
            state.time,
            state.parameters[PARAM]
        );
    }
    let first_seek = preview.seek(2.25).unwrap().parameters[PARAM];
    assert_eq!(preview.seek(2.25).unwrap().parameters[PARAM], first_seek);
    preview.clear_seek_cache();
    let before_cancel = preview.snapshot().clone();
    let mut updates = Vec::new();
    assert_eq!(
        preview
            .seek_with_progress(1.0, |done, total| {
                updates.push((done, total));
                done < 3
            })
            .unwrap_err(),
        zipzi_animation::AnimationError::SeekCancelled
    );
    assert_eq!(preview.snapshot(), &before_cancel);
    assert_eq!(updates[0], (0, 60));
    assert_eq!(
        preview.seek_with_progress(1.0, |_, _| true).unwrap().time,
        1.0
    );
    assert!(imported.candidate.get_motion(MOTION).is_some());
}

#[test]
fn event_cursor_and_virtual_target_coverage_are_explicit() {
    let imported = import_motion3(&document(), MOTION, "Typed", TYPED_MOTION3).unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    preview.advance(0.0).unwrap();
    assert!(preview.advance(0.5).unwrap().fired_events.is_empty());
    assert_eq!(preview.advance(0.25).unwrap().fired_events[0].value, "你好");
    assert!(preview.advance(0.25).unwrap().fired_events.is_empty());
    let imported = import_motion3(
        &document(),
        "00000000-0000-4000-8000-000000000d04",
        "Missing",
        MINIMAL_MOTION3,
    )
    .unwrap();
    let mut preview = MotionPreview::new(&imported.candidate);
    preview
        .schedule_motion("00000000-0000-4000-8000-000000000d04", 0.0)
        .unwrap();
    assert!(!preview.advance(0.5).unwrap().coverage.is_empty());
}
