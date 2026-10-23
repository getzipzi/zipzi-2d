pub use kasane_animation::MotionPreview;
pub use kasane_core::{Canvas, Document, Parameter, Vec2};
pub use kasane_project::{import_expression3, import_motion3, import_physics3, import_pose3};

pub const DOC: &str = "00000000-0000-4000-8000-000000000d01";
pub const PARAM: &str = "00000000-0000-4000-8000-000000000d02";
pub const MOTION: &str = "00000000-0000-4000-8000-000000000d03";

pub const THIRTY_PHYSICS3: &str =
    include_str!("../../../../tests/fixtures/animation_cpu/thirty.physics3.json");
pub const MISSING_PHYSICS3: &str =
    include_str!("../../../../tests/fixtures/animation_cpu/missing.physics3.json");
pub const LOOP_MOTION3: &str =
    include_str!("../../../../tests/fixtures/animation_cpu/loop.motion3.json");
pub const TYPED_MOTION3: &str =
    include_str!("../../../../tests/fixtures/animation_cpu/typed.motion3.json");
pub const MINIMAL_MOTION3: &str =
    include_str!("../../../../tests/fixtures/animation_cpu/minimal.motion3.json");
pub const MINIMAL_POSE3: &str =
    include_str!("../../../../tests/fixtures/animation_cpu/minimal.pose3.json");

pub fn document() -> Document {
    let mut document = Document::new();
    assert!(document
        .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
        .is_ok());
    assert!(document
        .create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamX".into(),
            name: "X".into(),
            minimum: 0.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })
        .status
        .is_ok());
    document
}

pub fn combined_cache_preview() -> MotionPreview {
    use kasane_core::Part;
    let mut doc = document();
    assert!(doc
        .create_parameter(Parameter {
            id: "00000000-0000-4000-8000-000000000e01".into(),
            runtime_id: "ParamY".into(),
            name: "Y".into(),
            minimum: -1.0,
            maximum: 1.0,
            ..Default::default()
        })
        .status
        .is_ok());
    for (id, runtime) in [
        ("00000000-0000-4000-8000-000000000e02", "Part0"),
        ("00000000-0000-4000-8000-000000000e03", "Part1"),
    ] {
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
    let source = serde_json::json!({"Version":3,"Meta":{"Duration":2,"Fps":30,"Loop":true,
        "AreBeziersRestricted":true,"CurveCount":2,"TotalSegmentCount":3,"TotalPointCount":5,
        "FadeInTime":0,"FadeOutTime":0,"UserDataCount":1,"TotalUserDataSize":4},
        "Curves":[{"Target":"Parameter","Id":"ParamX","Segments":[0,0,0,2,1]},
        {"Target":"PartOpacity","Id":"Part1","Segments":[0,0,2,0.8,1,2,2,0]}],
        "UserData":[{"Time":59.0/60.0,"Value":"tick"}]});
    doc = import_motion3(&doc, MOTION, "Cache", &source.to_string())
        .unwrap()
        .candidate;
    doc = import_pose3(&doc, "00000000-0000-4000-8000-000000000e04", MINIMAL_POSE3)
        .unwrap()
        .candidate;
    doc = import_physics3(
        &doc,
        "00000000-0000-4000-8000-000000000e05",
        THIRTY_PHYSICS3,
    )
    .unwrap()
    .candidate;
    let expression = "00000000-0000-4000-8000-000000000e06";
    doc = import_expression3(&doc, expression, "Add", r#"{"Type":"Live2D Expression","FadeInTime":0.7,"FadeOutTime":0.8,"Parameters":[{"Id":"ParamX","Value":0.2,"Blend":"Add"}]}"#).unwrap().candidate;
    let mut preview = MotionPreview::new(&doc);
    preview.schedule_motion(MOTION, 0.0).unwrap();
    preview.schedule_motion(MOTION, 2.4).unwrap();
    preview.schedule_expression(expression, 0.2).unwrap();
    preview.schedule_expression(expression, 1.5).unwrap();
    preview.schedule_parameter_input(PARAM, 1.1, 0.8).unwrap();
    preview
}
