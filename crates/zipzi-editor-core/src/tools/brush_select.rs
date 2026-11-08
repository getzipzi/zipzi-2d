//! Paint a component selection without acquiring a model edit.
use super::{gesture::Press, hints::*, region::brush_points};
use crate::{CommandId, ComponentRef, Document, ViewportPoint};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrushShape {
    #[default]
    Circle,
    Square,
    Line,
}

/// Viewport logical pixels, normalized strength/hardness and straight RGBA color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrushSelectionSettings {
    pub diameter: f32,
    /// Paint strength, independent of the accumulated component weights.
    pub weight: f32,
    pub hardness: f32,
    pub shape: BrushShape,
    pub angle: f32,
    pub color: [f32; 4],
}
impl BrushSelectionSettings {
    pub const MIN_DIAMETER: f32 = 4.;
    pub const MAX_DIAMETER: f32 = 1024.;

    pub(crate) fn validate(self) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.diameter.is_finite()
                && (Self::MIN_DIAMETER..=Self::MAX_DIAMETER).contains(&self.diameter)
                && self.weight.is_finite()
                && (0.0..=1.0).contains(&self.weight)
                && self.hardness.is_finite()
                && (0.0..=1.0).contains(&self.hardness)
                && self.angle.is_finite()
                && (-180.0..=180.0).contains(&self.angle)
                && self
                    .color
                    .iter()
                    .all(|c| c.is_finite() && (0.0..=1.0).contains(c)),
            "Invalid brush selection settings"
        );
        Ok(self)
    }
    pub(crate) fn set_diameter(&mut self, diameter: f32) -> anyhow::Result<()> {
        *self = Self { diameter, ..*self }.validate()?;
        Ok(())
    }
    pub fn half_extents(self) -> [f32; 2] {
        [
            self.diameter / 2.,
            self.diameter / 2.
                * if self.shape == BrushShape::Line {
                    0.1
                } else {
                    1.
                },
        ]
    }
    fn sample_spacing(self) -> f64 {
        let [rx, ry] = self.half_extents();
        rx.min(ry) as f64 / 8.
    }
    /// Minimum normalized distance over the continuous stroke, including a stationary dab.
    pub fn influence(self, p: ViewportPoint, start: ViewportPoint, end: ViewportPoint) -> f32 {
        let distance = if self.shape == BrushShape::Circle {
            crate::picking::segment_distance2(p, start, end).sqrt() / (self.diameter / 2.)
        } else {
            let [rx, ry] = self.half_extents();
            let (sin, cos) = self.angle.to_radians().sin_cos();
            let local = |center: ViewportPoint| {
                let (x, y) = (p.x - center.x, p.y - center.y);
                [(x * cos + y * sin) / rx, (-x * sin + y * cos) / ry]
            };
            let a = local(start);
            let b = local(end);
            let d = [b[0] - a[0], b[1] - a[1]];
            let norm = |t: f32| (a[0] + d[0] * t).abs().max((a[1] + d[1] * t).abs());
            let mut best = norm(0.).min(norm(1.));
            for (offset, slope) in [
                (a[0], d[0]),
                (a[1], d[1]),
                (a[0] - a[1], d[0] - d[1]),
                (a[0] + a[1], d[0] + d[1]),
            ] {
                if slope.abs() > 1e-8 {
                    let t = (-offset / slope).clamp(0., 1.);
                    best = best.min(norm(t));
                }
            }
            best
        };
        if !distance.is_finite() || distance > 1. {
            return 0.;
        }
        if distance <= self.hardness || self.hardness == 1. {
            return self.weight;
        }
        let t = ((distance - self.hardness) / (1. - self.hardness)).clamp(0., 1.);
        self.weight * (1. - t * t * (3. - 2. * t))
    }
}
impl Default for BrushSelectionSettings {
    fn default() -> Self {
        Self {
            diameter: 64.,
            weight: 0.2,
            hardness: 0.5,
            shape: BrushShape::Circle,
            angle: 0.,
            color: [1., 0., 0., 0.5],
        }
    }
}

/// Distance-spaced dabs share their phase across pointer events. A motion dab
/// contributes spacing / diameter of a full dab, so event frequency and brush
/// spacing do not multiply paint strength. The press itself is one full dab.
struct StrokeSamples {
    start: ViewportPoint,
    direction: [f64; 2],
    length: f64,
    first: f64,
    spacing: f64,
}
impl StrokeSamples {
    fn influence(&self, settings: BrushSelectionSettings, point: ViewportPoint) -> f32 {
        let [rx, ry] = settings.half_extents();
        let radius = if settings.shape == BrushShape::Circle {
            rx as f64
        } else {
            (rx as f64).hypot(ry as f64)
        };
        let delta = [
            point.x as f64 - self.start.x as f64,
            point.y as f64 - self.start.y as f64,
        ];
        let along = delta[0] * self.direction[0] + delta[1] * self.direction[1];
        let perpendicular = [
            delta[0] - along * self.direction[0],
            delta[1] - along * self.direction[1],
        ];
        if perpendicular[0].hypot(perpendicular[1]) > radius {
            return 0.;
        }
        // Visit only dabs near this point, even for a very long pointer jump.
        let low = (along - radius).max(0.);
        let high = (along + radius).min(self.length);
        let first = self.first + ((low - self.first) / self.spacing).ceil().max(0.) * self.spacing;
        if first > high {
            return 0.;
        }
        let count = ((high - first) / self.spacing + 1.)
            .min((2. * radius / self.spacing).ceil() + 2.) as usize;
        let mut remaining_log = 0.;
        for i in 0..count {
            let offset = first - along + i as f64 * self.spacing;
            let center = ViewportPoint {
                x: (point.x as f64 - perpendicular[0] + self.direction[0] * offset) as f32,
                y: (point.y as f64 - perpendicular[1] + self.direction[1] * offset) as f32,
            };
            let influence = settings.influence(point, center, center) as f64;
            if influence == 1. {
                return 1.;
            }
            remaining_log += (-influence).ln_1p();
        }
        (-((remaining_log * self.spacing / settings.diameter as f64).exp_m1())) as f32
    }
}

pub(super) struct BrushSelectionGesture {
    pub press: Press,
    settings: BrushSelectionSettings,
    previous: ViewportPoint,
    candidates: Vec<(ComponentRef, ViewportPoint)>,
    started: bool,
    distance_to_next: f64,
    original_diameter: f32,
    pub resizing: bool,
}
impl BrushSelectionGesture {
    pub fn new(
        document: &Document,
        press: Press,
        settings: BrushSelectionSettings,
        resizing: bool,
    ) -> Option<Self> {
        let candidates = if resizing {
            Vec::new()
        } else {
            brush_points(document, &press)
        };
        if !resizing && candidates.is_empty() {
            return None;
        }
        Some(Self {
            previous: press.point,
            press,
            settings,
            candidates,
            started: false,
            distance_to_next: settings.sample_spacing(),
            original_diameter: settings.diameter,
            resizing,
        })
    }

    pub fn update(
        &mut self,
        components: &mut Vec<ComponentRef>,
        settings: &mut BrushSelectionSettings,
        weights: &mut HashMap<ComponentRef, f32>,
    ) {
        if self.resizing {
            settings.diameter =
                (self.original_diameter + (self.press.current.x - self.press.point.x) * 2.).clamp(
                    BrushSelectionSettings::MIN_DIAMETER,
                    BrushSelectionSettings::MAX_DIAMETER,
                );
            return;
        }
        if !self.started && self.press.operation == super::region::SelectionOperation::Replace {
            // Reset once on ordinary press; the captured snapshot still owns cancellation.
            components.clear();
            weights.clear();
        }
        let dx = self.press.current.x as f64 - self.previous.x as f64;
        let dy = self.press.current.y as f64 - self.previous.y as f64;
        let length = dx.hypot(dy);
        if self.started && length == 0. {
            return;
        }
        let samples = self.started.then(|| StrokeSamples {
            start: self.previous,
            direction: [dx / length, dy / length],
            length,
            first: self.distance_to_next,
            spacing: self.settings.sample_spacing(),
        });
        let mut selected: HashSet<_> = components.iter().cloned().collect();
        for (component, position) in &self.candidates {
            let influence = samples.as_ref().map_or_else(
                || {
                    self.settings
                        .influence(*position, self.previous, self.previous)
                },
                |samples| samples.influence(self.settings, *position),
            );
            if influence <= 0. {
                continue;
            }
            let old = if selected.contains(component) {
                *weights.get(component).unwrap_or(&1.)
            } else {
                0.
            };
            let erasing = self.press.operation == super::region::SelectionOperation::Remove;
            let mut value = match self.press.operation {
                super::region::SelectionOperation::Replace
                | super::region::SelectionOperation::Add => old + (1. - old) * influence,
                super::region::SelectionOperation::Remove => old * (1. - influence),
            };
            // Reach exact endpoints after repeated low-strength passes.
            if erasing && value < 1e-4 {
                value = 0.;
            } else if !erasing && value >= 1. - 1e-4 {
                value = 1.;
            }
            if value > 0. {
                selected.insert(component.clone());
                weights.insert(component.clone(), value);
            } else {
                selected.remove(component);
                weights.remove(component);
            }
        }
        components.retain(|component| selected.remove(component));
        for (component, _) in &self.candidates {
            if selected.remove(component) {
                components.push(component.clone());
            }
        }
        if let Some(samples) = samples {
            self.distance_to_next = (samples.first - length).rem_euclid(samples.spacing);
            if self.distance_to_next == 0. {
                self.distance_to_next = samples.spacing;
            }
        }
        self.started = true;
        self.previous = self.press.current;
    }

    pub fn cancel(
        &self,
        components: &mut Vec<ComponentRef>,
        settings: &mut BrushSelectionSettings,
        weights: &mut HashMap<ComponentRef, f32>,
    ) {
        components.clone_from(&self.press.components);
        weights.clone_from(&self.press.weights);
        if self.resizing {
            settings.diameter = self.original_diameter;
        }
    }
}

pub(super) const IDLE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::BrushSelect),
    InteractionHint::Shift(HintAction::AddSelection),
    InteractionHint::Alt(HintAction::RemoveSelection),
    InteractionHint::Primary(HintAction::Transform),
    InteractionHint::Command(CommandId::SelectTool),
];
pub(super) const CAPTURE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::BrushSelect),
    InteractionHint::Shift(HintAction::AddSelection),
    InteractionHint::Alt(HintAction::RemoveSelection),
    InteractionHint::Command(CommandId::CancelGesture),
];
pub(super) const RESIZE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::BrushSize),
    InteractionHint::Command(CommandId::CancelGesture),
];

impl super::ToolController {
    /// Cmd dragging can only acquire a painted point, never a frame or a child-preservation edit.
    pub(super) fn brush_drag_hit(
        &self,
        document: &Document,
        frame: &super::CanvasFrame,
        point: ViewportPoint,
    ) -> Option<crate::HitTarget> {
        let selection = &document.context().selections;
        let access = self.edit_access(document, &frame.geometry, selection);
        let locks = document.object_state_index();
        self.components
            .iter()
            .filter_map(|component| {
                if !selection.contains(&component.object)
                    || locks.is_locked(&component.object)
                    || !frame.geometry.in_isolation_scope(&component.object)
                    || !matches!(access.probe(component), Some(Ok(())))
                {
                    return None;
                }
                let position = if component.object.kind == crate::ObjectKind::Mesh {
                    if !frame.geometry.mesh_visible(&component.object.id) {
                        return None;
                    }
                    frame
                        .geometry
                        .mesh(&component.object.id)?
                        .vertices
                        .iter()
                        .find(|(id, _)| *id == component.id)?
                        .1
                } else {
                    let control = frame.geometry.control(&component.object.id)?;
                    if control.kind != crate::ControlKind::Warp
                        || !control.visible
                        || !control.enabled
                    {
                        return None;
                    }
                    *control.points.get(component.id as usize)?
                };
                let distance =
                    crate::picking::distance2(frame.transform.to_viewport(position), point);
                (distance <= crate::picking::CONTROL_RADIUS.powi(2))
                    .then_some((component, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(component, _)| crate::HitTarget::Component(component.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f32, y: f32) -> ViewportPoint {
        ViewportPoint { x, y }
    }
    #[test]
    fn hardness_falloff_shapes_angle_and_continuous_sweep_agree() {
        let s = BrushSelectionSettings {
            diameter: 100.,
            weight: 0.8,
            hardness: 0.5,
            ..Default::default()
        };
        assert_eq!(s.influence(p(0., 0.), p(0., 0.), p(0., 0.)), 0.8);
        assert!((s.influence(p(37.5, 0.), p(0., 0.), p(0., 0.)) - 0.4).abs() < 1e-6);
        assert_eq!(s.influence(p(51., 0.), p(0., 0.), p(0., 0.)), 0.);
        let square = BrushSelectionSettings {
            shape: BrushShape::Square,
            hardness: 1.,
            ..s
        };
        assert_eq!(square.influence(p(49., 49.), p(0., 0.), p(0., 0.)), 0.8);
        assert_eq!(s.influence(p(49., 49.), p(0., 0.), p(0., 0.)), 0.);
        let line = BrushSelectionSettings {
            shape: BrushShape::Line,
            angle: 90.,
            ..square
        };
        assert_eq!(line.influence(p(0., 45.), p(0., 0.), p(0., 0.)), 0.8);
        assert_eq!(line.influence(p(45., 0.), p(0., 0.), p(0., 0.)), 0.);
        assert_eq!(square.influence(p(0., 49.), p(-100., 0.), p(100., 0.)), 0.8);
    }
}
