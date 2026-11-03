//! Explicit edit-state baking through a nonlinear source parent. Destination
//! inversion remains affine and unique; bindings retain their identities and
//! scalar channels, with geometry guaranteed only at the captured inputs.
use super::*;
use crate::document::deformer_original::OriginalParentSpace;

pub(super) struct BakedMesh {
    pub mesh: Mesh,
    pub binding: Option<MeshBinding>,
    pub blends: Vec<crate::BlendShapeBinding>,
}

impl Document {
    pub(super) fn can_bake_reparent_source(
        &self,
        edit: &ReparentEdit,
        id: &str,
        status: &Status,
    ) -> bool {
        edit.strategy == ReparentStrategy::PreserveEditState
            && (self.get_mesh(id).is_some() || self.get_transform(id).is_some())
            && status.code == "NONLINEAR_PARENT_UNPROVEN"
    }

    pub(super) fn bake_transform_parent(
        &self,
        id: &str,
        parent: &str,
        new: ParentMap,
        context: &ReparentContext,
        control: &ConversionControl,
    ) -> Result<warp::WarpChange, Status> {
        let transform = self.get_transform(id).unwrap();
        let old = OriginalParentSpace::new(self, transform.parent(), &context.parameter_values)?;
        if transform.rotation().is_some() {
            // Freeze the inherited frame at this Rotation's captured origin.
            // Its native subtree depends only on world origin, direction and
            // scalar scale, so it can retain all of its own editable channels.
            let pose =
                crate::evaluation::authoring_rotation_pose(self, id, &context.parameter_values);
            let local = Vec2::new(pose.origin.x as f32, pose.origin.y as f32);
            let world = old.point(local);
            let mut frame = ParentMap {
                base: 0.,
                angle: old.direction(local),
                scale: old.inherited_scale(),
                origin: PsmVec2::default(),
                rx: false,
                ry: false,
                warp_basis: None,
                residual: ParentResidual::default(),
            };
            let offset = frame.point(local);
            frame.origin = PsmVec2::new(world.x - offset.x, world.y - offset.y);
            let converted = self.convert_rotation_parent(id, parent, frame, new, control)?;
            return Ok(warp::WarpChange {
                transforms: vec![converted.transform],
                bindings: converted.binding.into_iter().collect(),
                blends: converted.blends,
                magnitude: converted.magnitude,
                error: converted.error,
                axes: converted.axes,
            });
        }
        let reference =
            crate::evaluation::authoring_warp_positions(self, id, &context.parameter_values)?;
        let point = |p| new.inverse(old.point(p));
        let mut desired = reference.clone();
        for chunk in desired.chunks_mut(256) {
            control.checkpoint()?;
            for p in chunk {
                *p = point(*p)?;
            }
        }
        let mut convert = |points: &mut [Vec2], delta: bool| -> Result<(), Status> {
            for (i, p) in points.iter_mut().enumerate() {
                if i % 256 == 0 {
                    control.checkpoint()?;
                }
                *p = if delta {
                    let q = point(Vec2::new(reference[i].x + p.x, reference[i].y + p.y))?;
                    Vec2::new(q.x - desired[i].x, q.y - desired[i].y)
                } else {
                    point(*p)?
                };
            }
            Ok(())
        };
        let mut change = self.convert_warp_channels(id, parent, &mut convert)?;
        let mut candidate = self.clone();
        for t in &change.transforms {
            candidate.content.transforms.insert(t.id.clone(), t.clone());
        }
        for b in &change.bindings {
            candidate
                .content
                .scene_bindings
                .insert(b.id.clone(), b.clone());
        }
        for b in &change.blends {
            candidate
                .content
                .blend_bindings
                .insert(b.id.clone(), b.clone());
        }
        let actual =
            crate::evaluation::authoring_warp_positions(&candidate, id, &context.parameter_values)?;
        let correction: Vec<_> = desired
            .iter()
            .zip(actual)
            .map(|(a, b)| Vec2::new(a.x - b.x, a.y - b.y))
            .collect();
        let shift = |points: &mut [Vec2]| -> Result<(), Status> {
            for (i, (p, d)) in points.iter_mut().zip(&correction).enumerate() {
                if i % 256 == 0 {
                    control.checkpoint()?;
                }
                p.x += d.x;
                p.y += d.y;
            }
            require_ok(crate::geometry::validate_positions(points))
        };
        shift(&mut change.transforms[0].warp_mut().unwrap().points)?;
        for binding in &mut change.bindings {
            if let crate::SceneTrack::Warp { keyforms, .. } = &mut binding.track {
                for form in keyforms {
                    shift(&mut form.positions)?;
                }
            }
            require_ok(self.canonicalize_scene_binding(binding))?;
        }
        self.compensate_warp_rotation_scale(
            id,
            old.inherited_scale(),
            new.scale,
            &mut change,
            control,
        )?;
        Ok(change)
    }

    pub(super) fn bake_mesh_parent(
        &self,
        id: &str,
        parent: &str,
        new: ParentMap,
        context: &ReparentContext,
        control: &ConversionControl,
    ) -> Result<BakedMesh, Status> {
        let mesh = self.get_mesh(id).unwrap();
        let old = OriginalParentSpace::new(self, &mesh.deformer_id, &context.parameter_values)?;
        let reference =
            crate::evaluation::authoring_mesh_positions(self, id, &context.parameter_values)?;
        let point = |p| new.inverse(old.point(p));
        let mut desired = reference.clone();
        for chunk in desired.chunks_mut(256) {
            control.checkpoint()?;
            for p in chunk {
                *p = point(*p)?;
            }
        }
        let convert = |points: &mut [Vec2]| -> Result<(), Status> {
            for chunk in points.chunks_mut(256) {
                control.checkpoint()?;
                for p in chunk {
                    *p = point(*p)?;
                }
            }
            Ok(())
        };
        let mut mesh = mesh.clone();
        convert(&mut mesh.base_positions)?;
        mesh.deformer_id = parent.to_owned();
        let mut binding = self.binding_for_mesh(id).cloned();
        if let Some(b) = &mut binding {
            for form in &mut b.keyforms {
                convert(&mut form.positions)?;
            }
            require_ok(self.canonicalize_binding(b))?;
        }
        let mut blends = Vec::new();
        for blend in self.blend_bindings_for_target(id) {
            control.checkpoint()?;
            let mut blend = blend.clone();
            if let crate::DeltaKeyforms::Mesh(forms) = &mut blend.keyforms {
                for form in forms {
                    for (i, p) in form.positions.iter_mut().enumerate() {
                        if i % 256 == 0 {
                            control.checkpoint()?;
                        }
                        let q = point(Vec2::new(reference[i].x + p.x, reference[i].y + p.y))?;
                        *p = Vec2::new(q.x - desired[i].x, q.y - desired[i].y);
                    }
                }
            }
            require_ok(self.validate_blend_binding(&blend))?;
            blends.push(blend);
        }
        // Nonlinear maps do not commute with interpolation. Recenter all
        // ordinary samples by one displacement field so the frozen interpolated
        // pose (including active BlendShapes) matches native forward evaluation.
        let mut candidate = self.clone();
        candidate.content.meshes.insert(id.to_owned(), mesh.clone());
        if let Some(binding) = &binding {
            candidate
                .content
                .bindings
                .insert(binding.id.clone(), binding.clone());
        }
        for blend in &blends {
            candidate
                .content
                .blend_bindings
                .insert(blend.id.clone(), blend.clone());
        }
        let actual =
            crate::evaluation::authoring_mesh_positions(&candidate, id, &context.parameter_values)?;
        let correction: Vec<_> = desired
            .iter()
            .zip(actual)
            .map(|(a, b)| Vec2::new(a.x - b.x, a.y - b.y))
            .collect();
        let shift = |points: &mut [Vec2]| -> Result<(), Status> {
            for (i, (p, d)) in points.iter_mut().zip(&correction).enumerate() {
                if i % 256 == 0 {
                    control.checkpoint()?;
                }
                p.x += d.x;
                p.y += d.y;
            }
            require_ok(crate::geometry::validate_positions(points))
        };
        shift(&mut mesh.base_positions)?;
        if let Some(b) = &mut binding {
            for form in &mut b.keyforms {
                shift(&mut form.positions)?;
            }
            require_ok(self.canonicalize_binding(b))?;
        }
        require_ok(self.validate_mesh_properties(&mesh))?;
        Ok(BakedMesh {
            mesh,
            binding,
            blends,
        })
    }
}
