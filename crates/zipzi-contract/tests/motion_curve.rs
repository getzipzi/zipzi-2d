use zipzi_contract::{
    api_shape::ApiShape,
    binary_wire::{Reader, WireRead, WireWrite},
    motion_curve::{MotionEasingPreset, MotionFrameStep},
};

const PRESETS: &str = include_str!("../../../fixtures/animation_curves/presets.json");

fn wire_roundtrip<T>(value: &T)
where
    T: WireRead + WireWrite + PartialEq + std::fmt::Debug,
{
    let mut bytes = Vec::new();
    value.encode(&mut bytes).unwrap();
    let mut cost = Reader::new(&bytes).unwrap();
    T::input_cost(&mut cost).unwrap();
    cost.finish().unwrap();
    let mut reader = Reader::new(&bytes).unwrap();
    assert_eq!(T::decode(&mut reader).unwrap(), *value);
    reader.finish().unwrap();
}

#[test]
fn preset_table_matches_the_versioned_fixture_and_roundtrips() {
    let fixture: serde_json::Value = serde_json::from_str(PRESETS).unwrap();
    assert_eq!(fixture["version"], 1);
    let expected = fixture["presets"].as_array().unwrap();
    assert_eq!(MotionEasingPreset::ALL.len(), expected.len());
    for (preset, row) in MotionEasingPreset::ALL.into_iter().zip(expected) {
        let definition = preset.definition();
        assert_eq!(serde_json::to_value(preset).unwrap(), row["id"]);
        assert_eq!(
            serde_json::from_value::<MotionEasingPreset>(row["id"].clone()).unwrap(),
            preset
        );
        assert_eq!(definition.name, row["name"].as_str().unwrap());
        assert_eq!(
            definition.approximate,
            row["approximate"].as_bool().unwrap()
        );
        let controls: [[f32; 2]; 2] = serde_json::from_value(row["controls"].clone()).unwrap();
        assert_eq!(definition.controls, controls, "{preset:?}");
        wire_roundtrip(&preset);
    }
    let shape = serde_json::to_value(MotionEasingPreset::api_shape()).unwrap();
    let names: Vec<_> = shape["variants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| &v["name"])
        .collect();
    assert_eq!(
        names,
        expected.iter().map(|row| &row["id"]).collect::<Vec<_>>()
    );
    assert!(serde_json::from_str::<MotionEasingPreset>("\"custom\"").is_err());
}

#[test]
fn frame_step_has_a_canonical_off_state_and_accepts_the_full_positive_range() {
    assert_eq!(MotionFrameStep::default(), MotionFrameStep::Off {});
    assert_eq!(MotionFrameStep::from_frames(0), None);
    assert_eq!(
        MotionFrameStep::from_frames(1),
        Some(MotionFrameStep::Off {})
    );
    assert_eq!(
        MotionFrameStep::EveryFrames { frames: 0 }.normalized(),
        None
    );
    assert_eq!(
        MotionFrameStep::EveryFrames { frames: 1 }.normalized(),
        Some(MotionFrameStep::Off {})
    );
    for frames in [2, 3, 7, u32::MAX] {
        let step = MotionFrameStep::from_frames(frames).unwrap();
        let json = serde_json::json!({"kind":"every_frames", "frames":frames});
        assert_eq!(serde_json::to_value(step).unwrap(), json);
        assert_eq!(
            serde_json::from_value::<MotionFrameStep>(json).unwrap(),
            step
        );
        assert_eq!(step.normalized(), Some(step));
        wire_roundtrip(&step);
    }
    assert_eq!(
        serde_json::to_value(MotionFrameStep::Off {}).unwrap(),
        serde_json::json!({"kind":"off"})
    );
    wire_roundtrip(&MotionFrameStep::Off {});
    for invalid in [
        r#"{"kind":"every_frames", "frames":-1}"#,
        r#"{"kind":"every_frames", "frames":2.5}"#,
        r#"{"kind":"every_frames", "frames":4294967296}"#,
        r#"{"kind":"every_frames", "frames":true}"#,
        r#"{"kind":"every_frames"}"#,
        r#"{"kind":"off", "frames":2}"#,
    ] {
        assert!(
            serde_json::from_str::<MotionFrameStep>(invalid).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn authored_intervals_roundtrip_modes_preset_sources_and_frame_steps() {
    use zipzi_contract::document::{MotionCurve, MotionKey, MotionPoint, MotionSegment};
    let end = MotionKey {
        id: "endpoint".into(),
        time: 1.,
        value: 1.,
    };
    let start = MotionPoint {
        time: 0.,
        value: 0.,
    };
    let [control1, control2] = MotionEasingPreset::CubicInOut
        .mapped_controls(start, end.point())
        .unwrap();
    for curve in [
        MotionCurve::Linear {},
        MotionCurve::AutoSmooth {},
        MotionCurve::Bezier {
            control1,
            control2,
            preset: Some(MotionEasingPreset::CubicInOut),
        },
        MotionCurve::Stepped {},
        MotionCurve::InverseStepped {},
    ] {
        let segment = MotionSegment {
            end: end.clone(),
            curve,
            frame_step: MotionFrameStep::EveryFrames { frames: u32::MAX },
        };
        let encoded = serde_json::to_value(&segment).unwrap();
        assert_eq!(encoded["frame_step"]["frames"], u32::MAX);
        assert_eq!(
            serde_json::from_value::<MotionSegment>(encoded).unwrap(),
            segment
        );
        wire_roundtrip(&segment);
    }
    let shape = serde_json::to_value(MotionSegment::api_shape()).unwrap();
    assert_eq!(
        shape["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["end", "curve", "frame_step"]
    );
    assert!(serde_json::from_str::<MotionCurve>(
        r#"{"kind":"auto_smooth","control1":{"time":0,"value":0}}"#
    )
    .is_err());
}

#[test]
fn retime_selection_command_roundtrips_fixed_end_keys_and_events() {
    use zipzi_contract::kernel_commands::{MotionCurveEdit, MotionRetimeFixedEnd};
    let payload = serde_json::json!({
        "kind": "retime_selection",
        "value": {
            "keys": [{"track_id": "track", "key_id": "key"}],
            "events": ["cue"],
            "fixed_end": "latest",
            "moving_time": 0.4123456789
        }
    });
    let edit: MotionCurveEdit = serde_json::from_value(payload.clone()).unwrap();
    assert!(matches!(&edit, MotionCurveEdit::RetimeSelection {
        fixed_end: MotionRetimeFixedEnd::Latest, keys, events, ..
    } if keys.len() == 1 && events == &["cue"]));
    assert_eq!(serde_json::to_value(edit).unwrap(), payload);
}
