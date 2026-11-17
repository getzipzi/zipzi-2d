//! Shared presentation vocabulary for all curve-authoring entrypoints.
use crate::t;
use zipzi_contract::{
    kernel_commands::MotionInterpolation,
    motion_curve::{MotionEasingPreset, MotionFrameStep},
};
use zipzi_editor_core::{TimelineCurveDisabled, TimelineCurveSettings, TimelineMixed};

pub(crate) const INTERPOLATIONS: [(MotionInterpolation, &str); 5] = [
    (MotionInterpolation::Linear, "linear"),
    (MotionInterpolation::AutoSmooth, "auto_smooth"),
    (MotionInterpolation::Bezier, "bezier"),
    (MotionInterpolation::Stepped, "stepped"),
    (MotionInterpolation::InverseStepped, "inverse"),
];
pub(crate) fn kind_label(kind: MotionInterpolation) -> String {
    t!(match kind {
        MotionInterpolation::Linear => "timeline.linear",
        MotionInterpolation::AutoSmooth => "timeline.auto_smooth",
        MotionInterpolation::Bezier => "timeline.bezier",
        MotionInterpolation::Stepped => "timeline.stepped",
        MotionInterpolation::InverseStepped => "timeline.inverse",
    })
    .into_owned()
}
pub(crate) fn kind_id(kind: MotionInterpolation) -> &'static str {
    INTERPOLATIONS.iter().find(|(k, _)| *k == kind).unwrap().1
}
pub(crate) fn preset_id(preset: MotionEasingPreset) -> String {
    serde_json::to_value(preset)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
pub(crate) fn preset_label(preset: MotionEasingPreset) -> String {
    let definition = preset.definition();
    format!(
        "{}{}",
        definition.name,
        if definition.approximate { " ≈" } else { "" }
    )
}
pub(crate) fn step_label(step: MotionFrameStep) -> String {
    match step {
        MotionFrameStep::Off {} => t!("timeline.step_off").into_owned(),
        MotionFrameStep::EveryFrames { frames } => {
            t!("timeline.every_frames", count = frames).into_owned()
        }
    }
}
pub(crate) fn settings_hint(settings: &TimelineCurveSettings) -> String {
    if let Some(reason) = settings.disabled {
        return t!(match reason {
            TimelineCurveDisabled::NoSelection => "timeline.select_curve_keys",
            TimelineCurveDisabled::EventsSelected => "timeline.curve_events_selected",
            TimelineCurveDisabled::MissingKey => "timeline.curve_missing_key",
            TimelineCurveDisabled::NoOutgoing => "timeline.no_outgoing",
            TimelineCurveDisabled::ReadOnly => "timeline.curve_readonly",
            TimelineCurveDisabled::Locked => "timeline.curve_locked",
        })
        .into_owned();
    }
    let mut hint = t!(
        "timeline.interval_count",
        count = settings.keys.len(),
        trailing = settings.trailing_keys
    )
    .into_owned();
    if settings.constant()
        && matches!(
            settings.frame_step,
            TimelineMixed::Uniform(MotionFrameStep::EveryFrames { .. })
        )
    {
        hint.push_str(&format!(" · {}", t!("timeline.constant_step_hint")));
    }
    hint
}
