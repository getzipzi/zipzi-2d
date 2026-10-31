//! Constant coordinate conversion for a static affine Warp, including every
//! stored child channel. Nonlinear maps never fall back to node-only inversion.
use super::*;
use crate::document::organization::Delta;
use crate::{DeltaKeyforms, SceneTrack, WarpTransform};
use std::collections::HashMap;

impl ParentMap {
    fn warp(w: &WarpTransform, parent: Self) -> Result<Self, Status> {
        require_ok(crate::geometry::validate_positions(&w.points))?;
        let o = w.points[0];
        let x = w.points[w.columns as usize];
        let y = w.points[(w.rows * (w.columns + 1)) as usize];
        let x = Vec2::new(x.x - o.x, x.y - o.y);
        let y = Vec2::new(y.x - o.x, y.y - o.y);
        // Allow only the float32 construction envelope of an affine lattice.
        // The numeric publication budget below also accounts for this envelope.
        let magnitude = w
            .points
            .iter()
            .fold(1f32, |m, p| m.max(p.x.abs()).max(p.y.abs()));
        for row in 0..=w.rows {
            for col in 0..=w.columns {
                let u = f64::from(col) / f64::from(w.columns);
                let v = f64::from(row) / f64::from(w.rows);
                let expected = Vec2::new(
                    (f64::from(o.x) + f64::from(x.x) * u + f64::from(y.x) * v) as f32,
                    (f64::from(o.y) + f64::from(x.y) * u + f64::from(y.y) * v) as f32,
                );
                let p = w.points[(row * (w.columns + 1) + col) as usize];
                if (p.x - expected.x).abs().max((p.y - expected.y).abs())
                    > 8. * f32::EPSILON * magnitude
                {
                    return Err(Status::error(
                        "NONLINEAR_WARP_PRESERVATION_UNPROVEN",
                        "Keep children requires affine old and new Warp maps; nonlinear control edits are unsupported",
                    ));
                }
            }
        }
        let origin = parent.point(o);
        let linear = Self {
            origin: PsmVec2::default(),
            ..parent
        };
        let result = Self {
            base: 0.,
            angle: 0.,
            scale: parent.scale,
            origin: PsmVec2::new(origin.x, origin.y),
            rx: false,
            ry: false,
            warp_basis: Some([linear.point(x), linear.point(y)]),
        };
        result.inverse(origin)?;
        Ok(result)
    }
}

impl Document {
    pub(in crate::document) fn warp_keep_children_delta(
        &self,
        id: &str,
        next: &WarpTransform,
        control: &ConversionControl,
    ) -> Result<(Delta, Vec<String>, f64, usize), Status> {
        self.authoring_unlocked(id)?;
        let transform = self
            .get_transform(id)
            .filter(|t| t.warp().is_some())
            .ok_or_else(|| Status::error("NOT_WARP", id))?;
        let old = transform.warp().unwrap();
        if (old.rows, old.columns, old.quad) != (next.rows, next.columns, next.quad) {
            return Err(Status::error(
                "WARP_PRESERVATION_TOPOLOGY",
                "Keep children cannot change conversion density or interpolation",
            ));
        }
        if next.points.len() != ((old.rows + 1) * (old.columns + 1)) as usize {
            return Err(Status::error(
                "INVALID_WARP_POINTS",
                "Warp lattice point count changed",
            ));
        }
        if let Some(controller) = &next.bezier {
            require_ok(controller.validate())?;
        }
        let mut replacement = transform.clone();
        *replacement.warp_mut().unwrap() = next.clone();
        require_ok(self.validate_transform(&replacement))?;
        let mut ancestor = id;
        while !ancestor.is_empty() {
            if self.binding_for_scene(ancestor).is_some()
                || !self.blend_bindings_for_target(ancestor).is_empty()
            {
                return Err(Status::error(
                    "DRIVEN_WARP_PRESERVATION_UNPROVEN",
                    format!(
                        "{ancestor}: keep children requires a static Warp and static ancestor spaces"
                    ),
                ));
            }
            ancestor = self.get_transform(ancestor).unwrap().parent();
        }
        let context = ReparentContext::default();
        let parent = self.reparent_parent_map(transform.parent(), &context, false, true)?;
        let old_map = ParentMap::warp(old, parent)?;
        let new_map = ParentMap::warp(next, parent)?;
        let descendants: Vec<_> = self
            .mesh_order()
            .iter()
            .chain(self.transform_order())
            .filter(|child| {
                let mut parent = self.scene_deformer_parent(child).unwrap_or("");
                while !parent.is_empty() && parent != id {
                    parent = self.scene_deformer_parent(parent).unwrap_or("");
                }
                parent == id
            })
            .cloned()
            .collect();
        let mut affected = vec![id.to_owned()];
        let mut drivers = Vec::new();
        for child in &descendants {
            control.checkpoint()?;
            self.authoring_unlocked(child)?;
            affected.push(child.clone());
            drivers.extend(self.reparent_drivers(child));
            let binding_ids: Vec<_> = self
                .binding_for_mesh(child)
                .map(|b| &b.id)
                .into_iter()
                .chain(self.binding_for_scene(child).map(|b| &b.id))
                .chain(self.blend_bindings_for_target(child).iter().map(|b| &b.id))
                .cloned()
                .collect();
            for binding in binding_ids {
                self.authoring_unlocked(&binding)?;
                affected.push(binding);
            }
        }
        if old.points == next.points {
            let mut delta = Delta::default();
            if old != next {
                delta.transforms.push(replacement);
            }
            return Ok((delta, affected, 0., 0));
        }
        let mut magnitude = 1f64;
        let mut measured = 0f64;
        let mut samples = 0usize;
        let mut convert = |points: &mut [Vec2], delta: bool| -> Result<(), Status> {
            let old = ParentMap {
                origin: if delta {
                    PsmVec2::default()
                } else {
                    old_map.origin
                },
                ..old_map
            };
            let new = ParentMap {
                origin: if delta {
                    PsmVec2::default()
                } else {
                    new_map.origin
                },
                ..new_map
            };
            for p in points {
                let world = old.point(*p);
                *p = new.inverse(world)?;
                let after = new.point(*p);
                measured =
                    measured.max(f64::from(world.x - after.x).hypot(f64::from(world.y - after.y)));
                magnitude = magnitude.max(
                    f64::from(world.x).abs()
                        + f64::from(world.y).abs()
                        + f64::from(new.origin.x).abs()
                        + f64::from(new.origin.y).abs()
                        + f64::from(p.x).abs()
                        + f64::from(p.y).abs(),
                );
                samples += 1;
            }
            Ok(())
        };
        let mut delta = Delta::default();
        let mut rotation_magnitude = 1f64;
        let mut rotation_error = 0f64;
        delta.transforms.push(replacement);
        for child in descendants
            .iter()
            .filter(|child| self.scene_deformer_parent(child) == Some(id))
        {
            control.checkpoint()?;
            if let Some(mesh) = self.get_mesh(child) {
                let mut mesh = mesh.clone();
                convert(&mut mesh.base_positions, false)?;
                delta.meshes.push(mesh);
                if let Some(binding) = self.binding_for_mesh(child) {
                    let mut binding = binding.clone();
                    for form in &mut binding.keyforms {
                        convert(&mut form.positions, false)?;
                    }
                    delta.bindings.push(binding);
                }
            } else if let Some(warp) = self.get_transform(child).and_then(Transform::warp) {
                let mut transform = self.get_transform(child).unwrap().clone();
                let mut warp = warp.clone();
                convert(&mut warp.points, false)?;
                convert_controller(&mut warp.bezier, &mut convert)?;
                *transform.warp_mut().unwrap() = warp;
                delta.transforms.push(transform);
                if let Some(binding) = self.binding_for_scene(child) {
                    let mut binding = binding.clone();
                    if let SceneTrack::Warp { keyforms, .. } = &mut binding.track {
                        for form in keyforms {
                            convert(&mut form.positions, false)?;
                            convert_controller(&mut form.bezier, &mut convert)?;
                        }
                    }
                    delta.scene_bindings.push(binding);
                }
            } else {
                // Native Rotation inherits direction and scalar Rotation scale,
                // not the Warp basis lengths. Reuse the shared channel plan.
                let changed = self.convert_rotation_parent(child, id, old_map, new_map, control)?;
                rotation_magnitude = rotation_magnitude.max(changed.magnitude);
                rotation_error = rotation_error.max(changed.error);
                delta.transforms.push(changed.transform);
                delta.scene_bindings.extend(changed.binding);
                delta.blends.extend(changed.blends);
                continue;
            }
            for blend in self.blend_bindings_for_target(child) {
                let mut blend = blend.clone();
                match &mut blend.keyforms {
                    DeltaKeyforms::Mesh(forms) => {
                        for f in forms {
                            convert(&mut f.positions, true)?;
                        }
                    }
                    DeltaKeyforms::Warp(forms) => {
                        for f in forms {
                            convert(&mut f.points, true)?;
                        }
                    }
                    _ => return Err(Status::error("WARP_CHILD_CHANNEL_UNSUPPORTED", child)),
                }
                delta.blends.push(blend);
            }
        }
        // Independent interior/exterior samples exercise native bilinear/triangle
        // mapping, not just the control nodes used to derive the affine basis.
        drop(convert);
        magnitude = magnitude.max(rotation_magnitude);
        measured = measured.max(rotation_error);
        let flat = |w: &WarpTransform| w.points.iter().flat_map(|p| [p.x, p.y]).collect::<Vec<_>>();
        let old_flat = flat(old);
        let new_flat = flat(next);
        for v in [-3., -1.7, -0.1, 0.07, 0.29, 0.53, 0.81, 0.97, 1., 1.4, 3.2] {
            for u in [-3., -1.7, -0.1, 0.07, 0.29, 0.53, 0.81, 0.97, 1., 1.4, 3.2] {
                control.checkpoint()?;
                let p = Vec2::new(u, v);
                let q = new_map.inverse(old_map.point(p))?;
                let mut a = [0.; 2];
                let mut b = [0.; 2];
                crate::deformers::warp_points(
                    old.rows as i32,
                    old.columns as i32,
                    old.quad,
                    &old_flat,
                    &[u, v],
                    &mut a,
                    1,
                );
                crate::deformers::warp_points(
                    next.rows as i32,
                    next.columns as i32,
                    next.quad,
                    &new_flat,
                    &[q.x, q.y],
                    &mut b,
                    1,
                );
                let a = parent.point(Vec2::new(a[0], a[1]));
                let b = parent.point(Vec2::new(b[0], b[1]));
                measured = measured.max(f64::from(a.x - b.x).hypot(f64::from(a.y - b.y)));
                magnitude = magnitude.max(
                    f64::from(a.x).abs()
                        + f64::from(a.y).abs()
                        + f64::from(b.x).abs()
                        + f64::from(b.y).abs(),
                );
                samples += 1;
            }
        }
        let mut candidate = self.clone();
        delta.install(&mut candidate);
        drivers.sort();
        drivers.dedup();
        // Verification at default and each driver's endpoints/key midpoints.
        // The constant affine channel conversion commutes with interpolation;
        // these checks supplement that structural argument, not a domain proof.
        let mut inputs = vec![HashMap::new()];
        for driver in drivers {
            let p = self.get_parameter(&driver).unwrap();
            let mut keys = vec![p.minimum, p.default_value, p.maximum];
            for child in &descendants {
                for axis in self
                    .binding_for_mesh(child)
                    .into_iter()
                    .flat_map(|b| &b.axes)
                    .chain(
                        self.binding_for_scene(child)
                            .into_iter()
                            .flat_map(|b| &b.axes),
                    )
                {
                    if axis.parameter_id == driver {
                        keys.extend(&axis.keys);
                    }
                }
                for blend in self.blend_bindings_for_target(child) {
                    let table = self.get_blend_key_table(&blend.key_table_id).unwrap();
                    if table.parameter_id == driver {
                        keys.extend(&table.keys);
                    }
                }
            }
            keys.sort_by(f32::total_cmp);
            keys.dedup();
            let mids: Vec<_> = keys.windows(2).map(|w| (w[0] + w[1]) * 0.5).collect();
            keys.extend(mids);
            inputs.extend(
                keys.into_iter()
                    .map(|v| HashMap::from([(driver.clone(), v)])),
            );
        }
        if inputs.len() > 4096 {
            return Err(Status::error(
                "WARP_PRESERVATION_CAPACITY",
                "Too many parameter verification poses",
            ));
        }
        for input in inputs {
            control.checkpoint()?;
            let mut before = crate::DrawableFrame::default();
            let mut after = crate::DrawableFrame::default();
            require_ok(crate::evaluation::evaluate_frame_including_hidden(
                self,
                &input,
                &mut before,
            ))?;
            require_ok(crate::evaluation::evaluate_frame_including_hidden(
                &candidate, &input, &mut after,
            ))?;
            for a in before
                .drawables
                .iter()
                .filter(|d| descendants.contains(&d.id))
            {
                let b = after.drawables.iter().find(|d| d.id == a.id).unwrap();
                for (a, b) in a.positions.iter().zip(&b.positions) {
                    measured = measured.max(f64::from(a.x - b.x).hypot(f64::from(a.y - b.y)));
                    magnitude = magnitude.max(f64::from(a.x).abs() + f64::from(a.y).abs());
                    samples += 1;
                }
            }
        }
        // A float32 construction envelope supplements independent verification.
        // Absolute canvas tolerance does not depend on viewport zoom or pixels.
        let envelope = measured + 16. * f64::from(f32::EPSILON) * magnitude;
        if !envelope.is_finite() || envelope > 0.01 {
            return Err(Status::error(
                "WARP_PRESERVATION_TOLERANCE",
                format!("Preservation error budget {envelope:.6} exceeds 0.01 canvas units"),
            ));
        }
        Ok((delta, affected, measured, samples))
    }
}

fn convert_controller(
    controller: &mut Option<crate::WarpBezier>,
    convert: &mut impl FnMut(&mut [Vec2], bool) -> Result<(), Status>,
) -> Result<(), Status> {
    if let Some(controller) = controller {
        for node in &mut controller.nodes {
            convert(std::slice::from_mut(&mut node.position), false)?;
            convert(&mut node.handles, false)?;
        }
    }
    Ok(())
}
