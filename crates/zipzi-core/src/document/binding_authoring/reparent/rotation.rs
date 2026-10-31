//! Compensate a Rotation's inherited frame while retaining its subtree's inputs.
use super::*;
use crate::{DeltaKeyforms, PreciseVec2, RotationPose, SceneBinding, SceneTrack};

pub(super) struct RotationChange {
    pub transform: Transform,
    pub binding: Option<SceneBinding>,
    pub blends: Vec<crate::BlendShapeBinding>,
    pub magnitude: f64,
    pub error: f64,
    pub axes: usize,
}

impl Document {
    pub(super) fn convert_rotation_parent(
        &self,
        id: &str,
        parent: &str,
        old: ParentMap,
        new: ParentMap,
        control: &ConversionControl,
    ) -> Result<RotationChange, Status> {
        let ratio = f64::from(old.scale) / f64::from(new.scale);
        self.rotation_scale_conversion_supported(id, ratio)?;
        let mut transform = self.get_transform(id).unwrap().clone();
        let rotation = transform.rotation_mut().unwrap();
        // Native rotation_points uses base_angle + angle. Compensating base_angle
        // leaves animated angles (and the BlendShape angle clamp) unchanged.
        rotation.base_angle = (f64::from(rotation.base_angle) + f64::from(old.direction())
            - f64::from(new.direction())) as f32;
        let mut magnitude = 1.0f64;
        let mut error = 0.0f64;
        let mut point = |p: Vec2, delta: bool| -> Result<Vec2, Status> {
            let old = ParentMap {
                origin: if delta {
                    PsmVec2::default()
                } else {
                    old.origin
                },
                ..old
            };
            let new = ParentMap {
                origin: if delta {
                    PsmVec2::default()
                } else {
                    new.origin
                },
                ..new
            };
            let world = old.point(p);
            let result = new.inverse(world)?;
            let after = new.point(result);
            error = error.max(
                (f64::from(after.x) - f64::from(world.x))
                    .hypot(f64::from(after.y) - f64::from(world.y)),
            );
            magnitude = magnitude.max(
                f64::from(world.x).abs()
                    + f64::from(world.y).abs()
                    + f64::from(old.origin.x).abs()
                    + f64::from(old.origin.y).abs()
                    + f64::from(new.origin.x).abs()
                    + f64::from(new.origin.y).abs(),
            );
            Ok(result)
        };
        let mut pose = |pose: &mut RotationPose| -> Result<(), Status> {
            let origin = point(Vec2::new(pose.origin.x as f32, pose.origin.y as f32), false)?;
            pose.origin = PreciseVec2::new(f64::from(origin.x), f64::from(origin.y));
            pose.scale = (f64::from(pose.scale) * ratio) as f32;
            Ok(())
        };
        pose(&mut rotation.pose)?;
        let mut binding = self.binding_for_scene(id).cloned();
        let axes = binding.as_ref().map_or(0, |b| {
            b.axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty() && a.keys.len() > 1)
                .count()
        });
        if let Some(binding) = &mut binding {
            if let SceneTrack::Rotation { keyforms, .. } = &mut binding.track {
                for form in keyforms {
                    control.checkpoint()?;
                    pose(&mut form.rotation)?;
                }
            }
            require_ok(self.canonicalize_scene_binding(binding))?;
        }
        let mut blends = Vec::new();
        for blend in self.blend_bindings_for_target(id) {
            control.checkpoint()?;
            let mut blend = blend.clone();
            if let DeltaKeyforms::Rotation(forms) = &mut blend.keyforms {
                for form in forms {
                    control.checkpoint()?;
                    if let Some(origin) = &mut form.origin {
                        *origin = point(*origin, true)?;
                    }
                    if let Some(scale) = &mut form.scale {
                        *scale = (f64::from(*scale) * ratio) as f32;
                    }
                }
            }
            require_ok(self.validate_blend_binding(&blend))?;
            blends.push(blend);
        }
        transform.parent_id = TransformId::optional(parent.to_owned());
        require_ok(self.validate_transform(&transform))?;
        // Include angle arithmetic in the same float32 envelope as affine Mesh
        // conversion. The subtree's stored channels provide a reference radius;
        // an independent native-frame check below also catches inherited errors.
        let angle = f64::from(transform.rotation().unwrap().base_angle).abs()
            + f64::from(
                self.get_transform(id)
                    .unwrap()
                    .rotation()
                    .unwrap()
                    .base_angle,
            )
            .abs()
            + f64::from(old.direction()).abs()
            + f64::from(new.direction()).abs();
        let radius = self.rotation_subtree_reference_radius(id, control)?;
        magnitude = magnitude.max(
            radius
                * (1. + angle.to_radians())
                * f64::from(old.scale)
                    .abs()
                    .max(f64::from(new.scale).abs())
                    .max(1.),
        );
        Ok(RotationChange {
            transform,
            binding,
            blends,
            magnitude,
            error,
            axes,
        })
    }

    fn rotation_scale_conversion_supported(&self, id: &str, ratio: f64) -> Result<(), Status> {
        if ratio == 1.
            || self
                .iter_active_blend_bindings_for_target(id)
                .next()
                .is_none()
        {
            return Ok(());
        }
        let scale = self
            .get_transform(id)
            .unwrap()
            .rotation()
            .unwrap()
            .pose
            .scale;
        let (mut low, mut high) = (f64::from(scale), f64::from(scale));
        if let Some(binding) = self
            .binding_for_scene(id)
            .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
        {
            if let SceneTrack::Rotation { keyforms, .. } = &binding.track {
                low = f64::INFINITY;
                high = f64::NEG_INFINITY;
                for form in keyforms {
                    low = low.min(f64::from(form.rotation.scale));
                    high = high.max(f64::from(form.rotation.scale));
                }
            }
        }
        for binding in self.iter_active_blend_bindings_for_target(id) {
            if let DeltaKeyforms::Rotation(forms) = &binding.keyforms {
                low += forms
                    .iter()
                    .map(|f| f64::from(f.scale.unwrap_or(0.)))
                    .fold(0., f64::min);
                high += forms
                    .iter()
                    .map(|f| f64::from(f.scale.unwrap_or(0.)))
                    .fold(0., f64::max);
            }
        }
        let minimum = f64::from(0.0001f32);
        if low < minimum || high > 100. || low * ratio < minimum || high * ratio > 100. {
            return Err(Status::error(
                "ROTATION_SCALE_CLAMP_UNPROVEN",
                format!(
                    "{id}: inherited scale compensation would change BlendShape scale clamping"
                ),
            ));
        }
        Ok(())
    }

    fn rotation_subtree_reference_radius(
        &self,
        root: &str,
        control: &ConversionControl,
    ) -> Result<f64, Status> {
        let mut radius = 1.0f64;
        let mut scale = 1.0f64;
        for id in self.mesh_order().iter().chain(self.transform_order()) {
            control.checkpoint()?;
            let mut parent = self.scene_deformer_parent(id).unwrap_or("");
            while !parent.is_empty() && parent != root {
                parent = self.scene_deformer_parent(parent).unwrap_or("");
            }
            if parent != root {
                continue;
            }
            let mut local = 0.0f64;
            let mut local_scale = 1.0f64;
            let mut add = |p: Vec2| {
                local = local.max(f64::from(p.x).hypot(f64::from(p.y)));
            };
            if let Some(mesh) = self.get_mesh(id) {
                for &p in &mesh.base_positions {
                    add(p);
                }
                if let Some(binding) = self.binding_for_mesh(id) {
                    for form in &binding.keyforms {
                        for &p in &form.positions {
                            add(p);
                        }
                    }
                }
            } else if let Some(warp) = self.get_transform(id).and_then(Transform::warp) {
                for &p in &warp.points {
                    add(p);
                }
            } else if let Some(r) = self.get_transform(id).and_then(Transform::rotation) {
                add(Vec2::new(r.pose.origin.x as f32, r.pose.origin.y as f32));
                local_scale = local_scale.max(f64::from(r.pose.scale));
            }
            if let Some(binding) = self.binding_for_scene(id) {
                match &binding.track {
                    SceneTrack::Rotation { keyforms, .. } => {
                        for form in keyforms {
                            add(Vec2::new(
                                form.rotation.origin.x as f32,
                                form.rotation.origin.y as f32,
                            ));
                            local_scale = local_scale.max(f64::from(form.rotation.scale));
                        }
                    }
                    SceneTrack::Warp { keyforms, .. } => {
                        for form in keyforms {
                            for &p in &form.positions {
                                add(p);
                            }
                        }
                    }
                    _ => {}
                }
            }
            for binding in self.blend_bindings_for_target(id) {
                let mut peak = 0.0f64;
                let mut add = |p: Vec2| {
                    peak = peak.max(f64::from(p.x).hypot(f64::from(p.y)));
                };
                match &binding.keyforms {
                    DeltaKeyforms::Mesh(forms) => {
                        for form in forms {
                            for &p in &form.positions {
                                add(p);
                            }
                        }
                    }
                    DeltaKeyforms::Warp(forms) => {
                        for form in forms {
                            for &p in &form.points {
                                add(p);
                            }
                        }
                    }
                    DeltaKeyforms::Rotation(forms) => {
                        let mut scale_delta = 0.0f64;
                        for form in forms {
                            if let Some(p) = form.origin {
                                add(p);
                            }
                            scale_delta = scale_delta.max(f64::from(form.scale.unwrap_or(0.)));
                        }
                        local_scale += scale_delta;
                    }
                    _ => {}
                }
                local += peak;
            }
            radius = radius.max(local);
            scale = scale.max(local_scale);
        }
        Ok(radius * scale)
    }

    pub(super) fn verify_rotation_reparent(
        &self,
        plan: &mut ReparentPlan,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        control.checkpoint()?;
        let context = plan.guarantee.context.as_ref().unwrap();
        let mut candidate = self.clone();
        require_ok(candidate.apply_reparent_plan(plan).status)?;
        let mut before = crate::DrawableFrame::default();
        let mut after = crate::DrawableFrame::default();
        require_ok(crate::evaluation::evaluate_frame_including_hidden(
            self,
            &context.parameter_values,
            &mut before,
        ))?;
        control.checkpoint()?;
        require_ok(crate::evaluation::evaluate_frame_including_hidden(
            &candidate,
            &context.parameter_values,
            &mut after,
        ))?;
        let affected: HashSet<_> = plan.affected.iter().map(String::as_str).collect();
        let mut measured = 0.0f64;
        let mut bounds: Option<[f64; 4]> = None;
        for (before, after) in before.drawables.iter().zip(&after.drawables) {
            control.checkpoint()?;
            if !affected.contains(before.id.as_str()) {
                continue;
            }
            for (a, b) in before.positions.iter().zip(&after.positions) {
                measured = measured
                    .max((f64::from(a.x) - f64::from(b.x)).hypot(f64::from(a.y) - f64::from(b.y)));
                let (x, y) = (f64::from(a.x), f64::from(a.y));
                bounds = Some(bounds.map_or([x, y, x, y], |[x0, y0, x1, y1]| {
                    [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
                }));
            }
        }
        let g = &mut plan.guarantee;
        g.reference_scale = g
            .reference_scale
            .max(bounds.map_or(1., |[x0, y0, x1, y1]| (x1 - x0).hypot(y1 - y0).max(1.)));
        g.measured_error = g.measured_error.max(measured);
        g.error_bound += measured;
        g.position_scope = if g.strategy == ReparentStrategy::PreserveParameterDomain {
            "Rotation inherited-frame compensation over the declared parameter domain; numeric verification at captured native edit inputs"
        } else {
            "Rotation channel compensation at the frozen ancestor pose; numeric verification at captured native edit inputs"
        }.into();
        if !g.error_bound.is_finite()
            || g.error_bound > g.tolerance + g.relative_tolerance * g.reference_scale
        {
            return Err(Status::error(
                "REPARENT_TOLERANCE_EXCEEDED",
                "Rotation subtree conversion exceeds the requested position tolerance",
            ));
        }
        plan.report.reparent_guarantee = Some(g.clone());
        Ok(())
    }
}
