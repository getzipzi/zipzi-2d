//! User-facing explanations for the manual mesh operation boundary.
use crate::t;

/// Feedback follows the latched gesture before looking at live modifiers.
pub(crate) fn connection_cursor(
    feedback: Option<zipzi_editor_core::ManualMeshFeedback>,
    modifiers: gpui_kit::Modifiers,
    gesture_active: bool,
) -> Option<gpui_kit::CursorStyle> {
    use gpui_kit::CursorStyle as C;
    use zipzi_editor_core::ManualMeshFeedback as F;
    Some(match feedback? {
        F::ConnectStart | F::ConnectVertex | F::ConnectNewVertex | F::FixEdge => C::PointingHand,
        F::Cancelled => C::Arrow,
        hit if modifiers.platform && !modifiers.shift && !gesture_active => match hit {
            F::FixedEdge | F::Empty => C::OperationNotAllowed,
            _ => C::PointingHand,
        },
        _ => return None,
    })
}

pub(crate) fn diagnostic(message: &str) -> String {
    if let Some(reason) = message.strip_prefix("Mesh Edit paused: ") {
        return t!("manual_mesh.paused", reason = diagnostic(reason)).into_owned();
    }
    let reason = message
        .strip_prefix("INVALID_MANUAL_MESH: ")
        .unwrap_or(message);
    if reason.contains("outside the source image") || reason.contains("outside source image") {
        t!("manual_mesh.outside_image").into_owned()
    } else if reason.contains("outside the boundary") {
        t!("manual_mesh.outside_boundary").into_owned()
    } else if reason.contains("crosses") || reason.contains("collapses triangle") {
        t!("manual_mesh.crossing").into_owned()
    } else if reason.contains("coincides")
        || reason.contains("overlapping UV")
        || reason.contains("touches multiple edges")
    {
        t!("manual_mesh.overlap").into_owned()
    } else if reason.contains("fewer than three boundary")
        || reason.contains("at least three noncollinear")
        || reason.contains("no mesh faces")
    {
        t!("manual_mesh.minimum_boundary").into_owned()
    } else if reason.contains("independent source image") {
        t!("manual_mesh.shared_image").into_owned()
    } else if reason.contains("binding")
        || reason.contains("BlendShape")
        || reason.contains("Glue")
        || reason.contains("parent deformer")
    {
        t!("manual_mesh.bound_mesh").into_owned()
    } else if reason.contains("affine") || reason.contains("not invertible") {
        t!("manual_mesh.non_affine").into_owned()
    } else if reason.contains("boundary") || reason.contains("holes or separate regions") {
        t!("manual_mesh.invalid_boundary").into_owned()
    } else {
        reason.to_owned()
    }
}
