//! Bounds for the lattice residual discarded by an affine parent fit. Native
//! snapshot checks alone cannot cover another parameter pose or an interior cell.
use super::*;
use crate::{DeltaKeyforms, SceneTrack, WarpTransform};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) struct Bounds([f64; 4]);
impl Bounds {
    pub(super) fn point(p: Vec2) -> Self {
        Self([p.x.into(), p.y.into(), p.x.into(), p.y.into()])
    }
    fn include(&mut self, p: Vec2) {
        self.0[0] = self.0[0].min(p.x.into());
        self.0[1] = self.0[1].min(p.y.into());
        self.0[2] = self.0[2].max(p.x.into());
        self.0[3] = self.0[3].max(p.y.into());
    }
    fn include_points(
        &mut self,
        points: &[Vec2],
        control: &ConversionControl,
    ) -> Result<(), Status> {
        for chunk in points.chunks(256) {
            control.checkpoint()?;
            for &p in chunk {
                self.include(p);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct ParentResidual {
    inside: f64,
    exterior: f64,
    linear: f64,
    radial: bool,
    pub(super) direction_unproven: bool,
}
impl ParentResidual {
    pub(super) fn is_zero(self) -> bool {
        self.inside == 0. && self.exterior == 0. && self.linear == 0.
    }
    pub(super) fn rotation(origin: f64, linear: f64) -> Self {
        Self {
            inside: origin,
            exterior: origin,
            linear,
            radial: true,
            direction_unproven: linear != 0.,
        }
    }
    fn lattice(error: f64) -> Self {
        // Each native virtual exterior corner is a linear combination of the
        // four real corners with an absolute coefficient sum of at most 5.5.
        Self {
            inside: error,
            exterior: 5.5 * error,
            linear: 0.,
            radial: false,
            direction_unproven: true,
        }
    }
    pub(super) fn bound(self, bounds: Bounds) -> f64 {
        let [x0, y0, x1, y1] = bounds.0;
        if self.radial {
            return self.inside
                + self.linear * x0.abs().max(x1.abs()).hypot(y0.abs().max(y1.abs()));
        }
        if x0 >= 0. && y0 >= 0. && x1 <= 1. && y1 <= 1. {
            return self.inside;
        }
        // Virtual cells span [-2, 0] and [1, 3]. Beyond them, the absolute
        // interpolation weights can exceed one, for triangles as well as quads.
        let axis = |low: f64, high: f64| 1.0f64.max(-low - 1.).max(high - 2.);
        self.exterior * axis(x0, x1) * axis(y0, y1)
    }
    pub(super) fn warp(warp: &WarpTransform, parent: ParentMap, fit: ParentMap) -> Self {
        let mut bounds = Bounds::point(warp.points[0]);
        let (mut inside, mut corners) = (0.0f64, 0.0f64);
        let [x, y] = fit.warp_basis.unwrap();
        for (i, &p) in warp.points.iter().enumerate() {
            bounds.include(p);
            let row = i / (warp.columns as usize + 1);
            let col = i % (warp.columns as usize + 1);
            let u = col as f64 / f64::from(warp.columns);
            let v = row as f64 / f64::from(warp.rows);
            let actual = parent.point(p);
            // Do not round the ideal affine lattice back to f32: that would
            // hide exactly the construction residual this bound must retain.
            let dx = f64::from(actual.x)
                - (f64::from(fit.origin.x) + f64::from(x.x) * u + f64::from(y.x) * v);
            let dy = f64::from(actual.y)
                - (f64::from(fit.origin.y) + f64::from(x.y) * u + f64::from(y.y) * v);
            let error = dx.hypot(dy);
            inside = inside.max(error);
            if (row == 0 || row == warp.rows as usize) && (col == 0 || col == warp.columns as usize)
            {
                corners = corners.max(error);
            }
        }
        let inherited = parent.residual.bound(bounds);
        // A separable, monotone rectangular lattice maps every vertical probe
        // to the same direction, even when its spacing has construction rounding.
        // This property composes through another such lattice or a Rotation
        // whose direction is already proved. Float32 evaluation still uses the
        // conversion's independent rounding envelope and native snapshot check.
        let stride = warp.columns as usize + 1;
        let separable = warp.points.iter().enumerate().all(|(i, p)| {
            p.x == warp.points[i % stride].x && p.y == warp.points[i / stride * stride].y
        });
        let monotone = |axis: usize, count: usize, step: usize| {
            let coordinate = |i: usize| {
                let p = warp.points[i * step];
                if axis == 0 {
                    p.x
                } else {
                    p.y
                }
            };
            let sign = (coordinate(count) - coordinate(0)).signum();
            (1..=count).all(|i| (coordinate(i) - coordinate(i - 1)) * sign > 0.)
        };
        let direction_unproven = (!parent.residual.is_zero() && parent.residual.direction_unproven)
            || !separable
            || !monotone(0, warp.columns as usize, 1)
            || !monotone(1, warp.rows as usize, stride);
        Self {
            inside: inside + inherited,
            exterior: (inside + inherited).max(5.5 * (corners + inherited)),
            linear: 0.,
            radial: false,
            direction_unproven,
        }
    }
}

impl Model<'_> {
    fn reparent_position_bounds(
        &self,
        id: &str,
        control: &ConversionControl,
    ) -> Result<Bounds, Status> {
        let origin = self
            .get_transform(id)
            .and_then(Transform::rotation)
            .map_or(Vec2::default(), |r| {
                Vec2::new(r.pose.origin.x as f32, r.pose.origin.y as f32)
            });
        let base = self
            .get_mesh(id)
            .map(|m| m.base_positions.as_slice())
            .or_else(|| {
                self.get_transform(id)
                    .and_then(Transform::warp)
                    .map(|w| w.points.as_slice())
            })
            .unwrap_or_else(|| std::slice::from_ref(&origin));
        let mut bounds = Bounds::point(base.first().copied().unwrap_or_default());
        bounds.include_points(base, control)?;
        if let Some(binding) = self.binding_for_mesh(id) {
            for form in &binding.keyforms {
                bounds.include_points(&form.positions, control)?;
            }
        }
        if let Some(binding) = self.binding_for_scene(id) {
            match &binding.track {
                SceneTrack::Warp { keyforms, .. } => {
                    for form in keyforms {
                        bounds.include_points(&form.positions, control)?;
                    }
                }
                SceneTrack::Rotation { keyforms, .. } => {
                    for form in keyforms {
                        control.checkpoint()?;
                        bounds.include(Vec2::new(
                            form.rotation.origin.x as f32,
                            form.rotation.origin.y as f32,
                        ));
                    }
                }
                _ => unreachable!(),
            }
        }
        // Ordinary interpolation is convex. Each active BlendShape contributes
        // at most the componentwise extrema of its deltas, including zero.
        for binding in self.iter_active_blend_bindings_for_target(id) {
            let mut delta = Bounds::point(Vec2::default());
            match &binding.keyforms {
                DeltaKeyforms::Mesh(forms) => {
                    for form in forms {
                        delta.include_points(&form.positions, control)?;
                    }
                }
                DeltaKeyforms::Warp(forms) => {
                    for form in forms {
                        delta.include_points(&form.points, control)?;
                    }
                }
                DeltaKeyforms::Rotation(forms) => {
                    for form in forms {
                        control.checkpoint()?;
                        if let Some(origin) = form.origin {
                            delta.include(origin);
                        }
                    }
                }
                _ => unreachable!(),
            }
            for (bound, delta) in bounds.0.iter_mut().zip(delta.0) {
                *bound += delta;
            }
        }
        Ok(bounds)
    }

    pub(super) fn reparent_residual_bound(
        &self,
        candidate: &Model<'_>,
        plan: &ReparentChanges,
        context: &ReparentContext,
        control: &ConversionControl,
    ) -> Result<f64, Status> {
        let mut errors = HashMap::new();
        let mut maximum = 0.0f64;
        let direction_unproven = |id: &str| {
            Status::error(
                "PARAMETER_DOMAIN_UNPROVEN",
                format!(
                    "{id}: a rounded Warp can change Rotation direction over the parameter domain"
                ),
            )
        };
        for id in &plan.affected {
            control.checkpoint()?;
            let Some(parent) = self.scene_deformer_parent(id) else {
                continue;
            };
            let new_parent = candidate.scene_deformer_parent(id).unwrap();
            if parent == new_parent {
                continue;
            }
            let old = self.reparent_parent_map(parent, context, true, true)?;
            let new = candidate.reparent_parent_map(new_parent, context, true, true)?;
            if old.residual.is_zero() && new.residual.is_zero() {
                continue;
            }
            let rotation = self
                .get_transform(id)
                .is_some_and(|t| t.rotation().is_some());
            if rotation
                && ((!old.residual.is_zero() && old.residual.direction_unproven)
                    || (!new.residual.is_zero() && new.residual.direction_unproven))
            {
                return Err(direction_unproven(id));
            }
            let error = old
                .residual
                .bound(self.reparent_position_bounds(id, control)?)
                + new
                    .residual
                    .bound(candidate.reparent_position_bounds(id, control)?);
            maximum = maximum.max(error);
            if self.get_transform(id).is_some() {
                errors.insert(id.clone(), (error, rotation));
            }
        }
        // A Warp stores its lattice in world space before applying it to its
        // descendants. Propagate the worst lattice error through every unchanged
        // descendant's full ordinary/BlendShape coordinate envelope.
        for id in self.sorted_transforms().iter().chain(self.mesh_order()) {
            control.checkpoint()?;
            if errors.contains_key(id) {
                continue;
            }
            let parent = self.scene_deformer_parent(id).unwrap();
            let Some(&(error, translation)) = errors.get(parent) else {
                continue;
            };
            if error == 0. {
                continue;
            }
            // A direction-preserving Rotation conversion changes only its
            // world origin at each pose. Every descendant shares that translation,
            // including nonlinear lattices and inherited Rotation frames.
            if translation {
                if self.get_transform(id).is_some() {
                    errors.insert(id.clone(), (error, true));
                }
                continue;
            }
            if self
                .get_transform(id)
                .is_some_and(|t| t.rotation().is_some())
            {
                return Err(direction_unproven(id));
            }
            let error =
                ParentResidual::lattice(error).bound(self.reparent_position_bounds(id, control)?);
            maximum = maximum.max(error);
            if self.get_transform(id).is_some() {
                errors.insert(id.clone(), (error, false));
            }
        }
        Ok(maximum)
    }
}
