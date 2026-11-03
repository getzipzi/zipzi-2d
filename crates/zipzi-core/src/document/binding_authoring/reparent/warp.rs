//! Re-express a Warp's lattice and controllers without flattening its subtree.
use super::*;
use crate::{DeltaKeyforms, SceneBinding, SceneTrack};

pub(super) struct WarpChange {
    pub transforms: Vec<Transform>,
    pub bindings: Vec<SceneBinding>,
    pub blends: Vec<crate::BlendShapeBinding>,
    pub magnitude: f64,
    pub error: f64,
    pub axes: usize,
}

impl Document {
    pub(super) fn convert_warp_parent(
        &self,
        id: &str,
        parent: &str,
        old: ParentMap,
        new: ParentMap,
        control: &ConversionControl,
    ) -> Result<WarpChange, Status> {
        let mut error = 0.;
        let mut magnitude = 1.0f64;
        let mut convert = |points: &mut [Vec2], delta: bool| -> Result<(), Status> {
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
            for chunk in points.chunks_mut(256) {
                control.checkpoint()?;
                for point in chunk {
                    let before_linear = ParentMap {
                        origin: PsmVec2::default(),
                        ..old
                    }
                    .point(*point);
                    let world = old.point(*point);
                    *point = new.inverse(world)?;
                    let after_linear = ParentMap {
                        origin: PsmVec2::default(),
                        ..new
                    }
                    .point(*point);
                    keep_children::measure_error(&mut error, world, new.point(*point))?;
                    for p in [world, before_linear, after_linear] {
                        magnitude = magnitude.max(f64::from(p.x).abs() + f64::from(p.y).abs());
                    }
                }
            }
            Ok(())
        };
        let mut change = self.convert_warp_channels(id, parent, &mut convert)?;
        change.error = error;
        change.magnitude = magnitude;
        self.compensate_warp_rotation_scale(id, old.scale, new.scale, &mut change, control)?;
        Ok(change)
    }

    pub(super) fn convert_warp_channels(
        &self,
        id: &str,
        parent: &str,
        convert: &mut impl FnMut(&mut [Vec2], bool) -> Result<(), Status>,
    ) -> Result<WarpChange, Status> {
        let mut change = WarpChange {
            transforms: Vec::new(),
            bindings: Vec::new(),
            blends: Vec::new(),
            magnitude: 1.,
            error: 0.,
            axes: 0,
        };
        let mut transform = self.get_transform(id).unwrap().clone();
        let warp = transform.warp_mut().unwrap();
        convert(&mut warp.points, false)?;
        keep_children::convert_controller(&mut warp.bezier, convert)?;
        transform.parent_id = TransformId::optional(parent.to_owned());
        require_ok(self.validate_transform(&transform))?;
        change.transforms.push(transform);
        if let Some(binding) = self.binding_for_scene(id) {
            let mut binding = binding.clone();
            change.axes = binding
                .axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty() && a.keys.len() > 1)
                .count();
            if let SceneTrack::Warp { keyforms, .. } = &mut binding.track {
                for form in keyforms {
                    convert(&mut form.positions, false)?;
                    keep_children::convert_controller(&mut form.bezier, convert)?;
                }
            }
            require_ok(self.canonicalize_scene_binding(&mut binding))?;
            change.bindings.push(binding);
        }
        for blend in self.blend_bindings_for_target(id) {
            let mut blend = blend.clone();
            if let DeltaKeyforms::Warp(forms) = &mut blend.keyforms {
                for form in forms {
                    convert(&mut form.points, true)?;
                }
            }
            require_ok(self.validate_blend_binding(&blend))?;
            change.blends.push(blend);
        }
        Ok(change)
    }

    pub(super) fn compensate_warp_rotation_scale(
        &self,
        id: &str,
        old_scale: f32,
        new_scale: f32,
        change: &mut WarpChange,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        // Warp lattices carry the world-space shape, but Rotation descendants
        // independently inherit scalar scale from the nearest Rotation ancestor.
        // Compensate the first Rotation on each branch; its descendants then
        // inherit the restored scale. All deeper local data remain unchanged.
        for child in self.transform_order() {
            control.checkpoint()?;
            let t = self.get_transform(child).unwrap();
            if t.rotation().is_none() {
                continue;
            }
            let mut ancestor = t.parent();
            while !ancestor.is_empty() && ancestor != id {
                let t = self.get_transform(ancestor).unwrap();
                if t.rotation().is_some() {
                    break;
                }
                ancestor = t.parent();
            }
            if ancestor != id || old_scale == new_scale {
                continue;
            }
            let identity = ParentMap {
                base: 0.,
                angle: 0.,
                scale: 1.,
                origin: PsmVec2::default(),
                rx: false,
                ry: false,
                warp_basis: Some([Vec2::new(1., 0.), Vec2::new(0., 1.)]),
                residual: ParentResidual::default(),
            };
            let converted = self.convert_rotation_parent(
                child,
                t.parent(),
                ParentMap {
                    scale: old_scale,
                    ..identity
                },
                ParentMap {
                    scale: new_scale,
                    ..identity
                },
                control,
            )?;
            change.magnitude = change.magnitude.max(converted.magnitude);
            change.error = change.error.max(converted.error);
            change.axes = change.axes.max(converted.axes);
            change.transforms.push(converted.transform);
            change.bindings.extend(converted.binding);
            change.blends.extend(converted.blends);
        }
        Ok(())
    }
}
