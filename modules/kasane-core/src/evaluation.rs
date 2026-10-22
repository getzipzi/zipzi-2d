mod evaluator;
mod glue;
mod meshes;
mod parameters;
mod parts;
mod prepared;
mod render;
mod selection;
mod transforms;
mod types;

pub use evaluator::{
    evaluate_frame, evaluate_frame_including_hidden, EvaluationTimings, FrameEvaluator,
};
pub(crate) use prepared::PreparedEvaluation;
pub use selection::evaluate_blend_binding;
pub use types::{
    parent_origin, parent_positions, Drawable, DrawableFrame, EvaluatedParameter, OffscreenFrame,
    PreviewValues, RenderCommand,
};

/// Native parameter-only edit snapshot, before parent inheritance and Glue.
pub(crate) fn authoring_rotation_pose(
    doc: &crate::Document,
    id: &str,
    values: &PreviewValues,
) -> crate::RotationPose {
    let t = doc.get_transform(id).unwrap();
    let mut pose = t.rotation().unwrap().pose;
    let values = normalized_authoring_values(doc, values);
    if let Some(b) = doc
        .binding_for_scene(id)
        .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
    {
        let mut selection = selection::Selection::default();
        let sel = selection::select(doc, &values, &b.axes, &mut selection);
        let first = b.track.sample(sel.indices[0]).rotation;
        pose = crate::RotationPose {
            origin: crate::PreciseVec2::default(),
            angle: 0.,
            scale: 0.,
            reflect_x: first.reflect_x,
            reflect_y: first.reflect_y,
        };
        // Evaluation accumulates the origins as float32 after parent_origin.
        let mut origin = crate::Vec2::default();
        for (&i, &w) in sel.indices.iter().zip(&sel.weights) {
            let p = b.track.sample(i).rotation;
            origin.x += p.origin.x as f32 * w;
            origin.y += p.origin.y as f32 * w;
            pose.angle += p.angle * w;
            pose.scale += p.scale * w;
        }
        pose.origin = crate::PreciseVec2::new(f64::from(origin.x), f64::from(origin.y));
    }
    let mut blended = false;
    for b in doc.iter_active_blend_bindings_for_target(id) {
        if let crate::DeltaKeyforms::Rotation(forms) = &b.keyforms {
            blended = true;
            let mut origin = crate::Vec2::new(pose.origin.x as f32, pose.origin.y as f32);
            for (i, w) in selection::evaluate_blend_binding(doc, &values, b) {
                if let Some(v) = forms[i].origin {
                    origin.x += v.x * w;
                    origin.y += v.y * w;
                }
                pose.angle += forms[i].angle.unwrap_or(0.) * w;
                pose.scale += forms[i].scale.unwrap_or(0.) * w;
            }
            pose.origin = crate::PreciseVec2::new(f64::from(origin.x), f64::from(origin.y));
        }
    }
    if blended {
        pose.angle = pose.angle.clamp(-3600., 3600.);
        pose.scale = pose.scale.clamp(0.0001, 100.);
    }
    pose
}

fn normalized_authoring_values(doc: &crate::Document, values: &PreviewValues) -> PreviewValues {
    values
        .iter()
        .map(|(id, &v)| {
            (
                id.clone(),
                parameters::normalize(doc.get_parameter(id).unwrap(), v).0,
            )
        })
        .collect()
}
/// Geometry inspection does not require texture content or renderability.
pub(crate) fn authoring_mesh_positions(
    doc: &crate::Document,
    id: &str,
    values: &PreviewValues,
) -> Result<Vec<crate::Vec2>, crate::Status> {
    let m = doc.get_mesh(id).unwrap();
    let values = normalized_authoring_values(doc, values);
    let mut positions = Vec::new();
    if let Some(b) = doc
        .binding_for_mesh(id)
        .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
    {
        let mut out = selection::Selection::default();
        let sel = selection::select(doc, &values, &b.axes, &mut out);
        selection::blend_positions(
            doc,
            &m.deformer_id,
            sel,
            |i| &b.keyforms[i].positions,
            &mut positions,
        )?;
    } else {
        positions.clone_from(&m.base_positions);
    }
    for b in doc.iter_active_blend_bindings_for_target(id) {
        if let crate::DeltaKeyforms::Mesh(forms) = &b.keyforms {
            for (i, w) in selection::evaluate_blend_binding(doc, &values, b) {
                for (p, d) in positions.iter_mut().zip(&forms[i].positions) {
                    p.x += d.x * w;
                    p.y += d.y * w;
                }
            }
        }
    }
    Ok(positions)
}
