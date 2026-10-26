//! User-facing explanations for the manual mesh operation boundary.
use crate::t;

pub(crate) fn diagnostic(message: &str) -> String {
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
    } else if reason.contains("fewer than three boundary") {
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
