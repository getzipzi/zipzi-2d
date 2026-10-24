//! Control display eligibility and viewport-dependent control points.
use super::{CanvasScene, ControlGeometry, ControlKind};
use crate::{DeformerVisibility, ObjectRef, ToolId, ViewportPoint, ViewportTransform};

impl CanvasScene {
    pub fn control_visible(
        &self,
        c: &ControlGeometry,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
    ) -> bool {
        c.enabled
            && c.visible
            && (selection.contains(&c.object)
                || match c.kind {
                    ControlKind::Glue => tool == ToolId::Glue,
                    _ => {
                        visibility.all
                            || visibility.ancestors
                                && selection
                                    .iter()
                                    .any(|r| self.ancestors(r).contains(&c.object))
                    }
                })
    }
    /// Rotation axes scale with the viewport, independently of the authored pose scale.
    pub fn control_points(&self, c: &ControlGeometry, t: ViewportTransform) -> Vec<ViewportPoint> {
        let mut points: Vec<_> = c.points.iter().map(|p| t.to_viewport(*p)).collect();
        if c.kind == ControlKind::Rotation && points.len() >= 2 {
            // Do not subtract nearby viewport/world points to recover a tiny
            // authored axis: translation and camera offsets erase its low bits.
            let axis = c.rotation_axis.unwrap_or(crate::ModelVector {
                x: c.points[1].x - c.points[0].x,
                y: c.points[1].y - c.points[0].y,
            });
            let d = ViewportPoint {
                x: axis.x,
                y: -axis.y,
            };
            let length = d.x.hypot(d.y);
            if length > 1e-6 {
                let axis_length = crate::deformer::ROTATION_AXIS_LENGTH * t.scale;
                points[1] = ViewportPoint {
                    x: points[0].x + d.x / length * axis_length,
                    y: points[0].y + d.y / length * axis_length,
                };
            }
            points.truncate(2);
        }
        points
    }
}
