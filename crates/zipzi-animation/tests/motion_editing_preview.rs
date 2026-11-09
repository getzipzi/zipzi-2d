use zipzi_animation::{canonical_motion_seek_time, MotionEditingPreview, MotionPreview};
use zipzi_core::{state::DocumentState, Canvas, Parameter, Part, Vec2};
use zipzi_project::{import_expression3, import_motion3, import_pose3};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn document() -> DocumentState {
    let document =
        DocumentState::new(id(1), Canvas::new(100., 100., Vec2::default(), 1.).into()).unwrap();
    let mut document = document.builder();
    for (n, runtime, default_value) in [(2, "ParamX", 0.), (3, "ParamY", 0.25)] {
        document
            .create_parameter(Parameter {
                id: id(n),
                runtime_id: runtime.into(),
                minimum: 0.,
                maximum: 1.,
                default_value,
                ..Default::default()
            })
            .unwrap();
    }
    for (n, runtime) in [(4, "Part0"), (5, "Part1")] {
        document
            .create_part(Part {
                id: id(n),
                runtime_id: runtime.into(),
                ..Default::default()
            })
            .unwrap();
    }
    let document = document.freeze().unwrap().state;
    let document = import_motion3(
        &document,
        &id(6),
        "Motion",
        include_str!("../../../fixtures/animation_cpu/minimal.motion3.json"),
    )
    .unwrap()
    .candidate;
    let document = import_pose3(
        &document,
        &id(7),
        include_str!("../../../fixtures/animation_cpu/minimal.pose3.json"),
    )
    .unwrap()
    .candidate;
    import_expression3(
        &document,
        &id(8),
        "Expression",
        include_str!("../../../fixtures/animation_cpu/minimal.exp3.json"),
    )
    .unwrap()
    .candidate
}

#[test]
fn raw_preview_is_stateless_has_default_baseline_and_resolves_pose_without_fades() {
    let document = document();
    let before = document.motion(&id(6)).unwrap().clone();
    let content = document.content_id();
    let mut raw = MotionEditingPreview::new(&document);
    let frame = raw.sample(Some(&id(6)), 0.5).unwrap().clone();
    assert_eq!(frame.parameters[&id(2)], 0.5);
    assert_eq!(frame.parameters[&id(3)], 0.25);
    assert_eq!(frame.part_opacities[&id(4)], 0.);
    assert_eq!(frame.part_opacities[&id(5)], 1.);
    assert_eq!(frame.model_opacity, 0.5);
    assert!(frame.fired_events.is_empty() && frame.active_expressions.is_empty());
    raw.sample(Some(&id(6)), 0.9).unwrap();
    assert_eq!(raw.sample(Some(&id(6)), 0.5).unwrap(), &frame);
    let defaults = raw.sample(None, 0.5).unwrap();
    assert_eq!(defaults.parameters[&id(2)], 0.);
    assert_eq!(defaults.parameters[&id(3)], 0.25);
    assert_eq!(defaults.part_opacities[&id(4)], 1.);
    assert_eq!(document.motion(&id(6)), Some(&before));
    assert_eq!(document.content_id(), content);
}

#[test]
fn raw_preview_ignores_result_expressions_and_fades_and_rejects_invalid_inputs_atomically() {
    let document = document();
    let mut clip = document.motion(&id(6)).unwrap().clone();
    clip.fade_in = Some(2.);
    let mut builder = document.builder();
    builder.replace_motion(clip).unwrap();
    let document = builder.freeze().unwrap().state;
    let mut result = MotionPreview::new(&document);
    result.schedule_motion(&id(6), 0.).unwrap();
    result.schedule_expression(&id(8), 0.).unwrap();
    let result = result.seek(0.5).unwrap();
    let mut raw = MotionEditingPreview::new(&document);
    let frame = raw.sample(Some(&id(6)), 0.5).unwrap().clone();
    assert_eq!(frame.parameters[&id(2)], 0.5);
    assert_ne!(frame.parameters[&id(2)], result.parameters[&id(2)]);
    for (clip, time) in [
        (Some("missing"), 0.5),
        (Some("missing"), f32::NAN),
        (None, -1.),
    ] {
        assert!(raw.sample(clip, time).is_err());
        assert_eq!(raw.snapshot(), &frame);
    }
    assert!(raw.evaluate_drawables().is_ok());
}

#[test]
fn sdk_seek_mapping_returns_requested_local_time_and_does_not_advance_loop_at_duration() {
    let document = document();
    let mut clip = document.motion(&id(6)).unwrap().clone();
    clip.looping = true;
    clip.fade_in = Some(0.);
    clip.fade_out = Some(0.);
    let mut builder = document.builder();
    builder.replace_motion(clip.clone()).unwrap();
    let document = builder.freeze().unwrap().state;
    let mut preview = MotionPreview::new(&document);
    preview.schedule_motion(&id(6), 0.).unwrap();
    for cycle in [0, 1, 5, 20] {
        for local in [0., 0.1, 0.5, 1.] {
            let time = canonical_motion_seek_time(&clip, cycle, local).unwrap();
            let frame = preview.seek(time).unwrap();
            let position = &frame.motion_positions[0];
            assert_eq!(position.loop_index, cycle);
            assert!((position.local_time - local).abs() < 0.00001);
            assert_eq!(position.period, 1. + 1. / 30.);
        }
    }
    assert!(canonical_motion_seek_time(&clip, 0, f32::NAN).is_err());
    assert!(canonical_motion_seek_time(&clip, 0, 2.).is_err());
}

#[test]
fn model_effects_mix_keyed_and_unkeyed_parameters_in_cubism_order() {
    let mut builder = document().builder();
    let settings = zipzi_project::model3::import_settings(
        &builder.preview().unwrap(),
        &serde_json::json!({"Groups":[
            {"Target":"Parameter","Name":"EyeBlink","Ids":["ParamX","ParamY","Missing"]},
            {"Target":"Parameter","Name":"LipSync","Ids":["ParamX","ParamY"]}
        ]}),
    )
    .unwrap();
    builder.set_model3_settings(settings).unwrap();
    let document = builder.freeze().unwrap().state;
    let source = serde_json::json!({
        "Version":3,
        "Meta":{"Duration":2,"Fps":30,"Loop":false,"AreBeziersRestricted":true,
            "CurveCount":3,"TotalSegmentCount":3,"TotalPointCount":6,
            "FadeInTime":1,"FadeOutTime":0,"UserDataCount":0,"TotalUserDataSize":0},
        "Curves":[
            {"Target":"Model","Id":"EyeBlink","Segments":[0,0.5,0,2,0.5]},
            {"Target":"Model","Id":"LipSync","Segments":[0,0.25,0,2,0.25]},
            {"Target":"Parameter","Id":"ParamX","FadeInTime":0.5,"Segments":[0,0.8,0,2,0.8]}
        ],
        "UserData":[]
    });
    let document = import_motion3(&document, &id(9), "Model effects", &source.to_string())
        .unwrap()
        .candidate;
    let mut playback = MotionPreview::new(&document);
    playback.schedule_motion(&id(9), 0.).unwrap();
    playback.advance(0.).unwrap();
    let frame = playback.advance(0.5).unwrap();
    // ParamX uses its track fade and multiplies before adding. Unkeyed ParamY
    // fades toward 0.5, then fades from that result toward 0.25.
    assert!((frame.parameters[&id(2)] - 0.65).abs() < 1e-6);
    assert!((frame.parameters[&id(3)] - 0.3125).abs() < 1e-6);
    assert!(frame.coverage.is_empty());

    let mut editing = MotionEditingPreview::new(&document);
    let frame = editing.sample(Some(&id(9)), 0.5).unwrap();
    assert!((frame.parameters[&id(2)] - 0.65).abs() < 1e-6);
    assert_eq!(frame.parameters[&id(3)], 0.25);
    assert!(frame.coverage.is_empty());
}
