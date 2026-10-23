//! Translate typed feedback at display time, so changing locale also updates errors.
use crate::t;
use editor_core::AnimationFailure;

pub fn failure_text(failure: &AnimationFailure) -> String {
    use AnimationFailure::*;
    use kasane_sdk::AnimationError as E;
    match failure {
        MotionRemoved => t!("workspace.motion_removed").into_owned(),
        ExpressionUnavailable => t!("workspace.expression_unavailable").into_owned(),
        TriggersRemoved => t!("workspace.triggers_removed").into_owned(),
        RendererStopped => t!("editor.preview_stopped").into_owned(),
        Other(detail) => t!("workspace.preview_failed", detail = detail).into_owned(),
        Runtime(error) => match error {
            E::MissingExpression(id) => t!("workspace.expression_missing", id = id).into_owned(),
            E::MissingMotion(id) => t!("workspace.motion_missing", id = id).into_owned(),
            E::MissingMotionEntry { group, index } => t!(
                "workspace.registration_missing",
                group = group,
                entry = index + 1
            )
            .into_owned(),
            E::UnresolvedMotionTarget {
                motion_id,
                category,
                runtime_id,
            } => t!(
                "workspace.motion_target_missing",
                motion = motion_id,
                category = category,
                target = runtime_id
            )
            .into_owned(),
            E::UnresolvedParameter {
                expression_id,
                runtime_id,
            } => t!(
                "workspace.expression_target_missing",
                expression = expression_id,
                target = runtime_id
            )
            .into_owned(),
            E::InvalidTime => t!("workspace.invalid_time").into_owned(),
            E::PastActivation => t!("workspace.past_activation").into_owned(),
            E::SeekLimit => t!("workspace.seek_limit").into_owned(),
            E::EventLimit => t!("workspace.event_limit").into_owned(),
            E::SeekCancelled => t!("workspace.seek_cancelled").into_owned(),
            E::Evaluation(detail) => {
                t!("workspace.evaluation_failed", detail = detail).into_owned()
            }
        },
    }
}
