//! Screen-space manipulators. Painting and picking share the same geometry.
use crate::{CanvasScene, ControlGeometry, ControlKind, ViewportPoint, ViewportTransform};

// Model-space display length, independent of the authored deformation.
pub(crate) const ROTATION_BONE_LENGTH: f32 = zipzi_core::document::DEFAULT_ROTATION_BONE_LENGTH;
// Fixed logical-pixel gap between editable geometry and its transform frame.
const FRAME_PADDING: f32 = 6.;
const WARP_ROTATE_OFFSET: f32 = 24.;

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
    /// Screen-space orientation; bounds describe the unrotated frame at this center.
    pub frame_angle: f32,
    pub bounds: [f32; 4],
    pub outline: Vec<(ViewportPoint, ViewportPoint)>,
    pub handles: Vec<(DeformerHandle, ViewportPoint)>,
}

#[derive(Clone)]
pub(crate) struct WarpSelectionGizmo {
    pub object: crate::ObjectRef,
    pub gizmo: DeformerGizmo,
}

#[derive(Clone, Default)]
pub(crate) struct WarpGizmos {
    pub objects: Vec<crate::ObjectRef>,
    pub local: bool,
    pub items: Vec<(crate::ObjectRef, DeformerGizmo)>,
    pub adjusting: Vec<crate::ObjectRef>,
}

impl CanvasScene {
    pub(crate) fn rotation_screen_length(&self, c: &ControlGeometry, t: ViewportTransform) -> f32 {
        let scale = self
            .source
            .viewport_frame
            .deformers
            .iter()
            .find(|d| d.id == c.object.id)
            .and_then(|d| d.rotation_basis)
            .map_or(1., |[x, _]| x.x.hypot(x.y));
        c.rotation_bone_length * (scale / c.rotation_bone_reference_scale) * t.scale
    }

    pub(crate) fn object_gizmo(
        &self,
        object: &crate::ObjectRef,
        t: ViewportTransform,
        selection: Option<&WarpGizmos>,
    ) -> Option<DeformerGizmo> {
        if let Some(g) = selection.and_then(|s| s.items.iter().find(|(o, _)| o == object)) {
            return Some(g.1.clone());
        }
        self.gizmo_for_selection(self.control(&object.id)?, t, selection)
    }
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
            let length = self.rotation_screen_length(c, t);
            // Recover direction before adding screen coordinates: a subpixel
            // bone at a large origin must not lose its authored angle.
            let axis_vector = c.rotation_axis.unwrap_or(crate::ModelVector {
                x: c.points[1].x - c.points[0].x,
                y: c.points[1].y - c.points[0].y,
            });
            let magnitude = axis_vector.x.hypot(axis_vector.y);
            if magnitude <= 1e-6 {
                return None;
            }
            let direction = p(axis_vector.x / magnitude, -axis_vector.y / magnitude);
            let scale = length / c.rotation_bone_length;
            let shoulder = (c.rotation_bone_length * 0.18).min(24.) * scale;
            let width = 8. * scale;
            let a = p(
                o.x + direction.x * shoulder - direction.y * width,
                o.y + direction.y * shoulder + direction.x * width,
            );
            let b = p(
                o.x + direction.x * shoulder + direction.y * width,
                o.y + direction.y * shoulder - direction.x * width,
            );
            let outline = vec![(*o, a), (a, *axis), (*axis, b), (b, *o)];
            return Some(DeformerGizmo {
                center: *o,
                frame_angle: direction.y.atan2(direction.x),
                bounds: crate::picking::bounds(
                    [*o, a, *axis, b].map(|p| crate::ModelPoint { x: p.x, y: p.y }),
                )
                .unwrap(),
                outline,
                handles: vec![(DeformerHandle::RotationScale, *axis)],
            });
        }
        Some(DeformerGizmo::warp(b))
    }

    pub(crate) fn warp_selected_points(
        &self,
        components: &[crate::ComponentRef],
    ) -> Vec<(crate::ComponentRef, crate::ModelPoint)> {
        components
            .iter()
            .filter_map(|component| {
                let control = self
                    .control(&component.object.id)
                    .filter(|c| c.kind == ControlKind::Warp)?;
                let point = if crate::warp_bezier::component(component.id).is_some() {
                    self.bezier(&component.object.id)?
                        .points()
                        .find(|(id, _)| *id == component.id)?
                        .1
                } else {
                    *control.points.get(component.id as usize)?
                };
                Some((component.clone(), point))
            })
            .collect()
    }

    pub(crate) fn warp_selection_gizmo(
        &self,
        components: &[crate::ComponentRef],
        t: ViewportTransform,
    ) -> Option<WarpSelectionGizmo> {
        let points = self.warp_selected_points(components);
        if points.len() < 2 {
            return None;
        }
        let object = points.first()?.0.object.clone();
        let bounds = crate::picking::bounds(points.iter().map(|(_, p)| {
            let p = t.to_viewport(*p);
            crate::ModelPoint { x: p.x, y: p.y }
        }))?;
        Some(WarpSelectionGizmo {
            object,
            gizmo: DeformerGizmo::warp(bounds),
        })
    }

    pub(crate) fn gizmo_for_selection(
        &self,
        c: &ControlGeometry,
        t: ViewportTransform,
        selection: Option<&WarpGizmos>,
    ) -> Option<DeformerGizmo> {
        if c.kind == ControlKind::Warp {
            if let Some(selection) = selection {
                return selection
                    .items
                    .iter()
                    .find(|(object, _)| *object == c.object)
                    .map(|(_, gizmo)| gizmo.clone());
            }
        }
        self.deformer_gizmo(c, t)
    }
}

impl DeformerGizmo {
    /// Bone polygons are convex. Include the same small edge tolerance used for picking.
    pub(crate) fn bone_hit(&self, point: ViewportPoint) -> bool {
        let mut positive = false;
        let mut negative = false;
        for &(a, b) in &self.outline {
            if crate::picking::segment_distance2(point, a, b) <= 25. {
                return true;
            }
            let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
            positive |= cross > 0.;
            negative |= cross < 0.;
        }
        (positive || negative) && !(positive && negative)
    }

    pub(crate) fn warp(b: [f32; 4]) -> Self {
        Self::warp_oriented(b, 0.)
    }

    pub(crate) fn object(b: [f32; 4]) -> Self {
        Self::object_oriented(b, 0.)
    }
    pub(crate) fn object_oriented(b: [f32; 4], frame_angle: f32) -> Self {
        Self::box_oriented(b, frame_angle, FRAME_PADDING)
    }
    pub(crate) fn warp_oriented(b: [f32; 4], frame_angle: f32) -> Self {
        Self::box_oriented(b, frame_angle, FRAME_PADDING)
    }
    fn box_oriented(b: [f32; 4], frame_angle: f32, padding: f32) -> Self {
        let p = |x, y| ViewportPoint { x, y };
        let center = p((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
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
        let mut outline: Vec<_> = (0..8)
            .map(|i| (perimeter[i], perimeter[(i + 1) % 8]))
            .collect();
        let mut handles: Vec<_> = perimeter
            .into_iter()
            .enumerate()
            .map(|(i, p)| (DeformerHandle::Scale(i as u8), p))
            .collect();
        let rotate = p(center.x, y0 - WARP_ROTATE_OFFSET);
        outline.push((p(center.x, y0), rotate));
        outline.push((p(center.x - 5., center.y), p(center.x + 5., center.y)));
        outline.push((p(center.x, center.y - 5.), p(center.x, center.y + 5.)));
        handles.push((DeformerHandle::Rotate, rotate));
        let rotate_point = |p: ViewportPoint| {
            let (s, c) = frame_angle.sin_cos();
            let (x, y) = (p.x - center.x, p.y - center.y);
            ViewportPoint {
                x: center.x + x * c - y * s,
                y: center.y + x * s + y * c,
            }
        };
        Self {
            center,
            frame_angle,
            bounds: b,
            outline: outline
                .into_iter()
                .map(|(a, b)| (rotate_point(a), rotate_point(b)))
                .collect(),
            handles: handles
                .into_iter()
                .map(|(h, p)| (h, rotate_point(p)))
                .collect(),
        }
    }
}

impl DeformerGizmo {
    pub(crate) fn scale_span(&self, index: u8, centered: bool) -> [f32; 2] {
        let factor = if centered { 0.5 } else { 1. };
        let x = [-1., 0., 1., 1., 1., 0., -1., -1.][index as usize];
        let y = [-1., -1., -1., 0., 1., 1., 1., 0.][index as usize];
        [
            (self.bounds[2] - self.bounds[0]) * x * factor,
            (self.bounds[3] - self.bounds[1]) * y * factor,
        ]
    }
    pub fn scale_anchor(&self, index: u8) -> (ViewportPoint, ViewportPoint) {
        let [x0, y0, x1, y1] = self.bounds;
        let x = [0., 0.5, 1., 1., 1., 0.5, 0., 0.][index as usize];
        let y = [0., 0., 0., 0.5, 1., 1., 1., 0.5][index as usize];
        let rotate = |p: ViewportPoint| {
            let (s, c) = self.frame_angle.sin_cos();
            let (x, y) = (p.x - self.center.x, p.y - self.center.y);
            ViewportPoint {
                x: self.center.x + x * c - y * s,
                y: self.center.y + x * s + y * c,
            }
        };
        (
            rotate(ViewportPoint {
                x: x0 + (x1 - x0) * x,
                y: y0 + (y1 - y0) * y,
            }),
            rotate(ViewportPoint {
                x: x1 - (x1 - x0) * x,
                y: y1 - (y1 - y0) * y,
            }),
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeformerAdjustment {
    pub translation: crate::ModelVector,
    /// Positive angles turn counterclockwise on the canvas.
    pub angle: f32,
    pub scale: f32,
}

/// Frozen component destination shared by numeric and pointer transforms.
#[derive(Clone, Debug, PartialEq)]
pub struct WarpTransformTarget {
    pub object: crate::ObjectRef,
    pub components: Vec<crate::ComponentRef>,
    revision: (u64, u64),
    pub(crate) viewport: Option<crate::ViewportId>,
    frame: Option<crate::WarpFrame>,
}
impl WarpTransformTarget {
    pub fn viewport(&self) -> Option<crate::ViewportId> {
        self.viewport
    }
}

impl crate::tools::ToolController {
    pub(crate) fn warp_transform_target(
        &self,
        document: &crate::Document,
        object: &crate::ObjectRef,
    ) -> Option<WarpTransformTarget> {
        let model = document.model();
        model.get_transform(&object.id)?.warp()?;
        let components = if self.warp_whole {
            vec![]
        } else {
            self.components
                .iter()
                .filter(|c| {
                    document.context().selections.contains(&c.object)
                        && model
                            .get_transform(&c.object.id)
                            .is_some_and(|t| t.warp().is_some())
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let components = if components.iter().any(|c| c.object == *object) {
            components
        } else {
            vec![]
        };
        Some(WarpTransformTarget {
            object: object.clone(),
            components,
            revision: (document.model_generation(), document.preview_revision()),
            viewport: None,
            frame: None,
        })
    }
    pub(crate) fn warp_transform_destination(
        &self,
        document: &crate::Document,
        object: &crate::ObjectRef,
        viewport: Option<crate::ViewportId>,
    ) -> Option<WarpTransformTarget> {
        let mut target = self.warp_transform_target(document, object)?;
        target.viewport = viewport;
        target.frame = viewport
            .and_then(|viewport| self.warp_frame_state(document, viewport, object))
            // Numeric point edits retain their explicit target. A single point
            // must not inherit the whole-object frame's adjusted pivot.
            .filter(|state| state.target.components == target.components)
            .map(|state| state.info.frame);
        Some(target)
    }
}

impl crate::Editor {
    pub fn warp_transform_target(&self, object: &crate::ObjectRef) -> Option<WarpTransformTarget> {
        let session = self.active_session()?;
        session.modeling.tools.warp_transform_destination(
            &session.document,
            object,
            self.primary_viewport_id(),
        )
    }
    pub fn warp_transform_target_for(
        &self,
        binding: crate::ViewportBinding,
        object: &crate::ObjectRef,
    ) -> Option<WarpTransformTarget> {
        self.viewport_binding_valid(binding).then_some(())?;
        let session = self.active_session()?;
        session.modeling.tools.warp_transform_destination(
            &session.document,
            object,
            Some(binding.viewport),
        )
    }
}

impl crate::Document {
    pub(crate) fn adjust_warp(
        &mut self,
        target: &WarpTransformTarget,
        adjustment: DeformerAdjustment,
    ) -> anyhow::Result<()> {
        use crate::control_edit::{ControlChange, ControlEdit, ControlScope};
        use anyhow::ensure;
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
        let control = scene
            .control(&target.object.id)
            .filter(|c| c.kind == ControlKind::Warp)
            .ok_or_else(|| anyhow::anyhow!("Warp is unavailable"))?;
        let points = scene.warp_selected_points(&target.components);
        ensure!(
            points.len() == target.components.len(),
            "Warp point selection changed"
        );
        let (objects, bounds, scope) = if points.is_empty() {
            (
                vec![target.object.clone()],
                crate::picking::bounds(control.points.iter().copied()),
                ControlScope::Whole,
            )
        } else {
            let mut objects = Vec::new();
            for (component, _) in &points {
                if !objects.contains(&component.object) {
                    objects.push(component.object.clone());
                }
            }
            (
                objects,
                crate::picking::bounds(points.iter().map(|(_, p)| *p)),
                ControlScope::Selected,
            )
        };
        ensure!(
            objects.iter().all(|o| scene.in_isolation_scope(o)),
            "Warp selection is outside this editing scope"
        );
        ensure!(
            !objects
                .iter()
                .any(|o| scene.ancestors(o).iter().any(|p| objects.contains(p))),
            "Select controls from one deformation level at a time"
        );
        let edits = objects
            .iter()
            .map(|o| ControlEdit::prepare(self, &scene, o))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let b = bounds.ok_or_else(|| anyhow::anyhow!("Warp has no points"))?;
        self.begin_control_edit(&edits)?;
        let result = self
            .preview_control_edit(
                &edits,
                &target.components,
                ControlChange::Transform {
                    axes: 0.,
                    scope,
                    pivot: target.frame.map(|f| f.center).unwrap_or(crate::ModelPoint {
                        x: (b[0] + b[2]) / 2.,
                        y: (b[1] + b[3]) / 2.,
                    }),
                    scale: [adjustment.scale; 2],
                    radians: adjustment.angle.to_radians(),
                    translation: adjustment.translation,
                },
            )
            .and_then(|_| self.finish_edit());
        if result.is_err() {
            self.cancel_edit();
        }
        result
    }

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
                axes: 0.,
                scope: crate::control_edit::ControlScope::Whole,
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
