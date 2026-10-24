//! Screen-space manipulators. Painting and picking share the same geometry.
use crate::{CanvasScene, ControlGeometry, ControlKind, ViewportPoint, ViewportTransform};

// Model-space dimensions; the viewport supplies fit-to-canvas scale and zoom.
pub(crate) const ROTATION_AXIS_LENGTH: f32 = 540.;
const ROTATION_RING_RADIUS: f32 = 280.;
const ROTATION_SCALE_DISTANCE: f32 = 400.;
// Fixed logical-pixel gap between the Warp control grid and its outer frame.
const WARP_FRAME_PADDING: f32 = 4.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeformerHandle {
    Move,
    Rotate,
    Scale(u8),
    RotationScale,
}

#[derive(Clone, Debug)]
pub struct DeformerGizmo {
    pub center: ViewportPoint,
    pub bounds: [f32; 4],
    pub outline: Vec<(ViewportPoint, ViewportPoint)>,
    pub handles: Vec<(DeformerHandle, ViewportPoint)>,
}

impl CanvasScene {
    pub fn deformer_gizmo(
        &self,
        c: &ControlGeometry,
        t: ViewportTransform,
    ) -> Option<DeformerGizmo> {
        if c.kind == ControlKind::Glue || !c.enabled || !c.visible {
            return None;
        }
        let points = self.control_points(c, t);
        let b =
            crate::picking::bounds(points.iter().map(|p| crate::ModelPoint { x: p.x, y: p.y }))?;
        let p = |x, y| ViewportPoint { x, y };
        if c.kind == ControlKind::Rotation {
            let [o, axis, ..] = points.as_slice() else {
                return None;
            };
            let axis_length = ROTATION_AXIS_LENGTH * t.scale;
            let direction = p((axis.x - o.x) / axis_length, (axis.y - o.y) / axis_length);
            let scale_distance = ROTATION_SCALE_DISTANCE * t.scale;
            let radius = ROTATION_RING_RADIUS * t.scale;
            let scale = p(
                o.x - direction.y * scale_distance,
                o.y + direction.x * scale_distance,
            );
            let mut outline = Vec::new();
            for i in 0..48 {
                let a = i as f32 * std::f32::consts::TAU / 48.;
                let b = (i + 1) as f32 * std::f32::consts::TAU / 48.;
                outline.push((
                    p(o.x + radius * a.cos(), o.y + radius * a.sin()),
                    p(o.x + radius * b.cos(), o.y + radius * b.sin()),
                ));
            }
            outline.push((*o, scale));
            return Some(DeformerGizmo {
                center: *o,
                bounds: b,
                outline,
                handles: vec![(DeformerHandle::RotationScale, scale)],
            });
        }
        let center = p((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
        let padding = WARP_FRAME_PADDING;
        let [x0, y0, x1, y1] = [
            b[0] - padding,
            b[1] - padding,
            b[2] + padding,
            b[3] + padding,
        ];
        let perimeter = [
            p(x0, y0),
            p(center.x, y0),
            p(x1, y0),
            p(x1, center.y),
            p(x1, y1),
            p(center.x, y1),
            p(x0, y1),
            p(x0, center.y),
        ];
        let outline: Vec<_> = (0..8)
            .map(|i| (perimeter[i], perimeter[(i + 1) % 8]))
            .collect();
        let handles: Vec<_> = perimeter
            .into_iter()
            .enumerate()
            .map(|(i, p)| (DeformerHandle::Scale(i as u8), p))
            .collect();
        Some(DeformerGizmo {
            center,
            bounds: b,
            outline,
            handles,
        })
    }
}

impl DeformerGizmo {
    pub fn scale_anchor(&self, index: u8) -> (ViewportPoint, ViewportPoint) {
        let [x0, y0, x1, y1] = self.bounds;
        let x = [0., 0.5, 1., 1., 1., 0.5, 0., 0.][index as usize];
        let y = [0., 0., 0., 0.5, 1., 1., 1., 0.5][index as usize];
        (
            ViewportPoint {
                x: x0 + (x1 - x0) * x,
                y: y0 + (y1 - y0) * y,
            },
            ViewportPoint {
                x: x1 - (x1 - x0) * x,
                y: y1 - (y1 - y0) * y,
            },
        )
    }
}

pub(crate) fn sample_grid(
    control: &ControlGeometry,
    (warp_rows, warp_columns, quad): (u32, u32, bool),
    transform: ViewportTransform,
) -> Vec<(ViewportPoint, ViewportPoint)> {
    if warp_rows == 0
        || warp_columns == 0
        || control.points.len() != (warp_rows as usize + 1) * (warp_columns as usize + 1)
    {
        return Vec::new();
    }
    let rows = (warp_rows.saturating_mul(4)).min(48) as usize;
    let columns = (warp_columns.saturating_mul(4)).min(48) as usize;
    let points: Vec<f32> = control.points.iter().flat_map(|p| [p.x, p.y]).collect();
    let input: Vec<f32> = (0..=rows)
        .flat_map(|r| {
            (0..=columns).flat_map(move |c| [c as f32 / columns as f32, r as f32 / rows as f32])
        })
        .collect();
    let mut output = vec![0.; input.len()];
    zipzi_core::deformers::warp_points(
        warp_rows as i32,
        warp_columns as i32,
        quad,
        &points,
        &input,
        &mut output,
        input.len() / 2,
    );
    let points: Vec<_> = output
        .chunks_exact(2)
        .map(|p| transform.to_viewport(crate::ModelPoint { x: p[0], y: p[1] }))
        .collect();
    let mut lines = Vec::new();
    for r in 0..=rows {
        for c in 0..=columns {
            let i = r * (columns + 1) + c;
            if c < columns {
                lines.push((points[i], points[i + 1]));
            }
            if r < rows {
                lines.push((points[i], points[i + columns + 1]));
            }
        }
    }
    lines
}

/// One atomic relative adjustment, expressed in canvas model coordinates.
#[derive(Clone, Copy, Debug)]
pub struct DeformerAdjustment {
    pub translation: crate::ModelVector,
    /// Positive angles turn counterclockwise on the canvas.
    pub angle: f32,
    pub scale: f32,
}

impl crate::Document {
    pub fn adjust_deformer(
        &mut self,
        object: &crate::ObjectRef,
        adjustment: DeformerAdjustment,
    ) -> anyhow::Result<()> {
        use crate::control_edit::{ControlChange, ControlEdit};
        use anyhow::ensure;
        ensure!(
            object.kind == crate::ObjectKind::Deformer,
            "Select a deformer first"
        );
        ensure!(
            [
                adjustment.translation.x,
                adjustment.translation.y,
                adjustment.angle,
                adjustment.scale
            ]
            .iter()
            .all(|v| v.is_finite())
                && adjustment.scale > 0.,
            "Enter finite values and a positive scale"
        );
        self.finish_edit()?;
        let snapshot = self.snapshot()?;
        let scene = self.geometry(&snapshot);
        let target = ControlEdit::prepare(self, &scene, object)?;
        let control = scene
            .controls
            .iter()
            .find(|c| &c.object == object && c.enabled)
            .ok_or_else(|| anyhow::anyhow!("Deformer is unavailable"))?;
        let b = crate::picking::bounds(control.points.iter().copied())
            .ok_or_else(|| anyhow::anyhow!("Deformer has no points"))?;
        let change = if target.rotation {
            ControlChange::Rotation {
                angle: adjustment.angle,
                scale: adjustment.scale,
                translation: adjustment.translation,
            }
        } else {
            ControlChange::Transform {
                pivot: crate::ModelPoint {
                    x: (b[0] + b[2]) / 2.,
                    y: (b[1] + b[3]) / 2.,
                },
                scale: [adjustment.scale; 2],
                radians: adjustment.angle.to_radians(),
                translation: adjustment.translation,
            }
        };
        let targets = [target];
        self.begin_control_edit(&targets)?;
        if let Err(error) = self
            .preview_control_edit(&targets, &[], change)
            .and_then(|_| self.finish_edit())
        {
            self.cancel_edit();
            return Err(error);
        }
        Ok(())
    }
}
