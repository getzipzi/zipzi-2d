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
                    ControlKind::Warp => !visibility.hide_unselected_warps,
                    ControlKind::Rotation => !visibility.hide_unselected_rotations,
                    ControlKind::Glue => {
                        self.topology.glue(&c.object.id).is_some_and(|g| {
                            selection
                                .iter()
                                .any(|r| r.id == g.mesh_a || r.id == g.mesh_b)
                        }) || tool == ToolId::Glue
                    }
                })
    }
    /// Evaluated outer lattice edges shared by painting and picking. No Bezier evaluation.
    pub fn warp_outline(
        &self,
        c: &ControlGeometry,
        t: ViewportTransform,
    ) -> Vec<(ViewportPoint, ViewportPoint)> {
        let Some((rows, columns, _)) = self
            .topology
            .controls
            .get(&c.object.id)
            .and_then(|c| c.warp_shape)
        else {
            return Vec::new();
        };
        let (rows, columns) = (rows as usize, columns as usize);
        let width = columns + 1;
        (0..columns)
            .flat_map(|col| [(col, col + 1), (rows * width + col, rows * width + col + 1)])
            .chain((0..rows).flat_map(|row| {
                [
                    (row * width, (row + 1) * width),
                    (row * width + columns, (row + 1) * width + columns),
                ]
            }))
            .filter_map(|(a, b)| {
                Some((
                    t.to_viewport(*c.points.get(a)?),
                    t.to_viewport(*c.points.get(b)?),
                ))
            })
            .collect()
    }
    /// Rotation tips share the pose-scaled bone used for painting and picking.
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
                let axis_length = self.rotation_screen_length(c, t);
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
