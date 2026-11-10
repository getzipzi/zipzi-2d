use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use zipzi_core::evaluation::{Drawable, DrawableFrame, RenderCommand};
use zipzi_core::geometry::validate_render_mesh;
use zipzi_core::types::{Status, Vec2};

/// Shared preflight for editable Document frames and external runtime frames.
pub fn validate_frame(frame: &DrawableFrame) -> Status {
    validate_frame_with(frame, |_, drawable| {
        validate_render_mesh(&drawable.positions, &drawable.uvs, &drawable.indices)
    })
}

#[derive(Clone, Debug)]
struct Topology {
    uvs: Weak<[Vec2]>,
    indices: Weak<[u32]>,
    vertices: usize,
}

/// Tracks immutable topology without holding the caller's frame buffers alive.
/// Weak handles prevent address reuse and are invalidated by `Arc::make_mut`.
#[derive(Clone, Debug, Default)]
pub(crate) struct FrameValidator {
    topology: Vec<Option<Topology>>,
}

impl FrameValidator {
    pub(crate) fn validate(&mut self, frame: &DrawableFrame) -> Status {
        self.topology.resize_with(frame.drawables.len(), || None);
        validate_frame_with(frame, |slot, drawable| {
            let cached = &mut self.topology[slot];
            if cached.as_ref().is_some_and(|previous| {
                previous.vertices == drawable.positions.len()
                    && previous
                        .uvs
                        .upgrade()
                        .is_some_and(|uvs| Arc::ptr_eq(&uvs, &drawable.uvs))
                    && previous
                        .indices
                        .upgrade()
                        .is_some_and(|indices| Arc::ptr_eq(&indices, &drawable.indices))
            }) {
                // Converted coordinates are still checked below on every frame.
                return Status::ok();
            }
            let status =
                validate_render_mesh(&drawable.positions, &drawable.uvs, &drawable.indices);
            if status.is_ok() {
                *cached = Some(Topology {
                    uvs: Arc::downgrade(&drawable.uvs),
                    indices: Arc::downgrade(&drawable.indices),
                    vertices: drawable.positions.len(),
                });
            }
            status
        })
    }
}

fn validate_frame_with(
    frame: &DrawableFrame,
    mut geometry: impl FnMut(usize, &Drawable) -> Status,
) -> Status {
    // Resolve mask and render-plan references once. Scanning every drawable for
    // every command makes validation quadratic during dense parameter updates.
    let mut ids = HashMap::with_capacity(frame.drawables.len());
    for (slot, drawable) in frame.drawables.iter().enumerate() {
        if drawable.id.is_empty()
            || ids
                .insert(drawable.id.as_str(), drawable.renderable)
                .is_some()
        {
            return Status::error("INVALID_ID", "Drawable IDs must be non-empty and unique.");
        }
        let status = geometry(slot, drawable);
        if !status.is_ok() {
            return status;
        }
        if !drawable.opacity.is_finite()
            || drawable
                .multiply_color
                .iter()
                .chain(&drawable.screen_color)
                .any(|v| !v.is_finite())
            || drawable
                .positions
                .iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Status::error(
                "NON_FINITE",
                "Appearance and converted positions must be finite.",
            );
        }
        if let Some(mode) = drawable.raw_blend_mode {
            if !valid_blend_mode(mode) {
                return Status::error("UNSUPPORTED_BLEND_MODE", format!("{}: {mode}", drawable.id));
            }
        }
    }
    for drawable in &frame.drawables {
        if drawable
            .masks
            .iter()
            .any(|id| !ids.contains_key(id.as_str()))
        {
            return Status::error("MISSING_MASK", &drawable.id);
        }
    }
    for drawable in frame.drawables.iter().filter(|d| d.renderable) {
        if drawable
            .masks
            .iter()
            .any(|id| ids.get(id.as_str()) == Some(&false))
        {
            return Status::error("UNAVAILABLE_MASK", &drawable.id);
        }
    }
    let mut offscreen_ids = HashMap::with_capacity(frame.offscreens.len());
    for offscreen in &frame.offscreens {
        if offscreen.id.is_empty()
            || ids.contains_key(offscreen.id.as_str())
            || offscreen_ids
                .insert(offscreen.id.as_str(), offscreen)
                .is_some()
        {
            return Status::error(
                "INVALID_ID",
                "Offscreen IDs must be non-empty and unique across the frame.",
            );
        }
        if !offscreen.opacity.is_finite()
            || offscreen
                .multiply_color
                .iter()
                .chain(&offscreen.screen_color)
                .any(|v| !v.is_finite())
        {
            return Status::error("NON_FINITE", format!("{}.appearance", offscreen.id));
        }
        if offscreen.renderable
            && offscreen
                .masks
                .iter()
                .any(|id| ids.get(id.as_str()) == Some(&false))
        {
            return Status::error("UNAVAILABLE_MASK", &offscreen.id);
        }
        if !valid_blend_mode(offscreen.blend_mode) {
            return Status::error(
                "UNSUPPORTED_BLEND_MODE",
                format!("{}: {}", offscreen.id, offscreen.blend_mode),
            );
        }
        if offscreen
            .masks
            .iter()
            .any(|id| !ids.contains_key(id.as_str()))
        {
            return Status::error("MISSING_MASK", &offscreen.id);
        }
        if offscreen
            .parent_offscreen_id
            .as_ref()
            .is_some_and(|id| id == &offscreen.id)
        {
            return Status::error("INVALID_RENDER_PLAN", "An offscreen cannot parent itself.");
        }
    }
    for offscreen in &frame.offscreens {
        if offscreen
            .parent_offscreen_id
            .as_ref()
            .is_some_and(|id| !offscreen_ids.contains_key(id.as_str()))
        {
            return Status::error(
                "INVALID_RENDER_PLAN",
                format!("{}: missing parent", offscreen.id),
            );
        }
    }
    if !frame.offscreens.is_empty() || !frame.render_plan.is_empty() {
        let mut stack: Vec<&str> = Vec::new();
        let mut begun = HashSet::new();
        let mut drawn = HashSet::new();
        for command in &frame.render_plan {
            match command {
                RenderCommand::BeginOffscreen { offscreen_id } => {
                    let offscreen = offscreen_ids.get(offscreen_id.as_str());
                    if !offscreen.is_some_and(|o| o.renderable)
                        || !begun.insert(offscreen_id.as_str())
                    {
                        return Status::error("INVALID_RENDER_PLAN", offscreen_id);
                    }
                    let expected_parent = offscreen.and_then(|o| o.parent_offscreen_id.as_deref());
                    if stack.last().copied() != expected_parent {
                        return Status::error(
                            "INVALID_RENDER_PLAN",
                            format!("{}: parent mismatch", offscreen_id),
                        );
                    }
                    stack.push(offscreen_id);
                }
                RenderCommand::DrawMesh { mesh_id } => {
                    if ids.get(mesh_id.as_str()) != Some(&true) || !drawn.insert(mesh_id.as_str()) {
                        return Status::error("INVALID_RENDER_PLAN", mesh_id);
                    }
                }
                RenderCommand::EndOffscreen { offscreen_id } => {
                    if stack.pop() != Some(offscreen_id.as_str()) {
                        return Status::error("INVALID_RENDER_PLAN", offscreen_id);
                    }
                }
            }
        }
        if !stack.is_empty()
            || begun.len() != frame.offscreens.iter().filter(|o| o.renderable).count()
            || drawn.len() != frame.drawables.iter().filter(|d| d.renderable).count()
        {
            return Status::error(
                "INVALID_RENDER_PLAN",
                "Render plan must contain every available drawable and balanced offscreen exactly once.",
            );
        }
    }
    Status::ok()
}

fn valid_blend_mode(mode: u32) -> bool {
    let color = mode & 0xff;
    let alpha = (mode >> 8) & 0xff;
    mode >> 16 == 0 && color <= 17 && alpha <= 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use zipzi_core::evaluation::Drawable;
    use zipzi_core::types::Vec2;

    fn frame() -> DrawableFrame {
        DrawableFrame {
            drawables: vec![Drawable {
                id: "mesh".into(),
                positions: vec![
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0),
                ],
                uvs: std::sync::Arc::from([
                    Vec2::new(0.0, 0.0),
                    Vec2::new(1.0, 0.0),
                    Vec2::new(0.0, 1.0),
                ]),
                indices: std::sync::Arc::from([0, 1, 2]),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn cached_topology_still_checks_dynamic_data_and_vertex_count() {
        let mut validator = FrameValidator::default();
        let mut value = frame();
        assert!(validator.validate(&value).is_ok());
        value.drawables[0].positions[0].x = f32::NAN;
        assert_eq!(validator.validate(&value).code, "NON_FINITE");
        value.drawables[0].positions[0].x = f32::MAX;
        value.drawables[0].positions[0].x = f32::INFINITY;
        assert_eq!(validator.validate(&value).code, "NON_FINITE");
        value.drawables[0].positions[0].x = 0.0;
        value.drawables[0].positions.pop();
        assert_eq!(validator.validate(&value).code, "INVALID_LENGTH");
        value.drawables[0].positions.push(Vec2::new(0.0, 1.0));
        assert!(validator.validate(&value).is_ok());
        value.drawables[0].positions[0].y = f32::NAN;
        assert_eq!(validator.validate(&value).code, "NON_FINITE");
    }

    #[test]
    fn cache_tracks_topology_without_pin_and_revalidates_mutation() {
        let mut validator = FrameValidator::default();
        let mut value = frame();
        assert!(validator.validate(&value).is_ok());
        assert_eq!(Arc::strong_count(&value.drawables[0].indices), 1);
        Arc::make_mut(&mut value.drawables[0].indices)[2] = 9;
        assert_eq!(validator.validate(&value).code, "INVALID_INDEX");
        value.drawables[0].indices = Arc::from([0, 1, 2]);
        assert!(validator.validate(&value).is_ok());
        Arc::make_mut(&mut value.drawables[0].uvs)[0].x = f32::NAN;
        assert_eq!(validator.validate(&value).code, "NON_FINITE");
        value.drawables[0].uvs = Arc::from([Vec2::new(0.0, 0.0); 3]);
        assert!(validator.validate(&value).is_ok());
        let indices = Arc::clone(&value.drawables[0].indices);
        value.drawables.clear();
        assert!(validator.validate(&value).is_ok());
        assert_eq!(Arc::strong_count(&indices), 1);
    }

    #[test]
    fn accepts_complete_frame_and_empty_model() {
        let mut value = frame();
        assert!(validate_frame(&value).is_ok());
        value.drawables.clear();
        assert!(validate_frame(&value).is_ok());
    }

    #[test]
    fn boundless_frames_keep_coordinate_and_scale_validation() {
        let mut value = frame();
        assert!(validate_frame(&value).is_ok());
        assert!(validate_frame(&value).is_ok());
        value.drawables[0].positions[0].x = f32::NAN;
        assert_eq!(validate_frame(&value).code, "NON_FINITE");
        value.drawables[0].positions[0].x = 0.0;
        value.drawables[0].positions[0].y = f32::NAN;
        assert_eq!(validate_frame(&value).code, "NON_FINITE");
    }

    #[test]
    fn rejects_invalid_canvas_and_appearance() {
        let mut value = frame();
        value.drawables[0].positions[0].y = f32::NAN;
        assert_eq!(validate_frame(&value).code, "NON_FINITE");
        let mut value = frame();
        value.drawables[0].opacity = f32::NAN;
        assert_eq!(validate_frame(&value).code, "NON_FINITE");
    }

    #[test]
    fn rejects_invalid_extended_blend_mode() {
        let mut value = frame();
        value.drawables[0].raw_blend_mode = Some(18);
        assert_eq!(validate_frame(&value).code, "UNSUPPORTED_BLEND_MODE");
    }

    #[test]
    fn rejects_out_of_range_indices_and_mismatched_uvs() {
        let mut value = frame();
        value.drawables[0].indices = std::sync::Arc::from([0, 1, 9]);
        assert_eq!(validate_frame(&value).code, "INVALID_INDEX");
        let mut value = frame();
        value.drawables[0].uvs = std::sync::Arc::from([Vec2::new(0.0, 0.0)]);
        assert_eq!(validate_frame(&value).code, "INVALID_LENGTH");
    }

    #[test]
    fn rejects_duplicate_ids_and_dangling_masks() {
        let mut value = frame();
        value.drawables.push(value.drawables[0].clone());
        assert_eq!(validate_frame(&value).code, "INVALID_ID");
        let mut value = frame();
        value.drawables[0].masks.push("missing".into());
        assert_eq!(validate_frame(&value).code, "MISSING_MASK");
    }

    #[test]
    fn rejects_non_finite_and_overflowing_positions() {
        let mut value = frame();
        value.drawables[0].positions[0].x = f32::INFINITY;
        assert_eq!(validate_frame(&value).code, "NON_FINITE");
        let mut value = frame();
        value.drawables[0].positions[0].x = f32::INFINITY;
        value.drawables[0].positions[1].x = f32::MAX;
        assert_eq!(validate_frame(&value).code, "NON_FINITE");
    }

    #[test]
    fn allows_zero_triangles_and_rejects_incomplete_triangles() {
        let mut value = frame();
        value.drawables[0].indices = std::sync::Arc::from([]);
        assert!(validate_frame(&value).is_ok());
        let mut value = frame();
        value.drawables[0].indices = std::sync::Arc::from([0, 1]);
        assert_eq!(validate_frame(&value).code, "INVALID_LENGTH");
    }

    #[test]
    fn accepts_core_flat_plan_but_rejects_duplicate_commands() {
        let mut value = frame();
        value.render_plan.push(RenderCommand::DrawMesh {
            mesh_id: "mesh".into(),
        });
        assert!(validate_frame(&value).is_ok());
        value.render_plan.push(RenderCommand::DrawMesh {
            mesh_id: "mesh".into(),
        });
        assert_eq!(validate_frame(&value).code, "INVALID_RENDER_PLAN");
    }

    #[test]
    fn indexed_references_preserve_availability_and_parent_validation() {
        use zipzi_core::evaluation::OffscreenFrame;
        let mut value = frame();
        let mut source = value.drawables[0].clone();
        source.id = "mask".into();
        source.renderable = false;
        value.drawables.push(source);
        value.drawables[0].masks.push("mask".into());
        assert_eq!(validate_frame(&value).code, "UNAVAILABLE_MASK");
        value.drawables[0].masks.clear();
        value.render_plan.push(RenderCommand::DrawMesh {
            mesh_id: "mask".into(),
        });
        assert_eq!(validate_frame(&value).code, "INVALID_RENDER_PLAN");
        value.render_plan.clear();
        value.offscreens.push(OffscreenFrame {
            id: "group".into(),
            renderable: true,
            enabled: true,
            opacity: 1.,
            ..Default::default()
        });
        value.render_plan = vec![
            RenderCommand::BeginOffscreen {
                offscreen_id: "group".into(),
            },
            RenderCommand::DrawMesh {
                mesh_id: "mesh".into(),
            },
            RenderCommand::EndOffscreen {
                offscreen_id: "group".into(),
            },
        ];
        assert!(validate_frame(&value).is_ok());
        value.offscreens[0].masks.push("mask".into());
        assert_eq!(validate_frame(&value).code, "UNAVAILABLE_MASK");
        value.offscreens[0].masks.clear();
        value.offscreens[0].parent_offscreen_id = Some("missing".into());
        assert_eq!(validate_frame(&value).code, "INVALID_RENDER_PLAN");
        value.offscreens.push(OffscreenFrame {
            id: "missing".into(),
            renderable: true,
            ..Default::default()
        });
        // Parent now exists, but the plan opens its child at the root.
        assert_eq!(validate_frame(&value).code, "INVALID_RENDER_PLAN");
    }
}
