//! Bounded, independent samples for the explicit Warp resize preview.
use super::*;
use crate::{ObjectKind, ObjectRef};
use zipzi_core::{DeltaKeyforms, SceneTrack, Vec2, WarpTransform};

#[derive(Clone, Copy, Debug, Default)]
pub struct WarpGridPreviewMetrics {
    pub shapes: usize,
    pub samples_per_shape: usize,
    /// Largest measured displacement in the Warp's parent authoring space.
    /// This is a sampling result, never a global error bound.
    pub maximum_sample_error: f64,
    pub controller_shapes: usize,
    pub maximum_controller_error: Option<f64>,
}

impl Document {
    pub fn warp_density_preview_metrics(
        &self,
        object: &ObjectRef,
        bezier: bool,
    ) -> Result<WarpGridPreviewMetrics> {
        let mut result = self.warp_grid_preview_metrics(object)?;
        if !bezier {
            return Ok(result);
        }
        use zipzi_core::WarpBezierEvaluation;
        let before = self.session.document();
        let after = self.model();
        let a = before.get_transform(&object.id).unwrap();
        let b = after.get_transform(&object.id).unwrap();
        let wa = a.warp().unwrap();
        let wb = b.warp().unwrap();
        let controller = |t: &zipzi_core::Transform,
                          points: &[Vec2],
                          stored: &Option<zipzi_core::WarpBezier>| {
            stored.clone().unwrap_or_else(|| {
                let w = t.warp().unwrap();
                t.warp_authoring
                    .as_ref()
                    .and_then(|s| s.bezier_grid)
                    .map_or_else(
                        || zipzi_core::WarpBezier::from_lattice(w.rows, w.columns, w.quad, points),
                        |grid| {
                            zipzi_core::WarpBezier::from_lattice_grid(
                                w.rows, w.columns, w.quad, points, grid,
                            )
                        },
                    )
            })
        };
        let mut maximum = 0f64;
        let mut measure = |pa: &[Vec2],
                           ca: &Option<zipzi_core::WarpBezier>,
                           pb: &[Vec2],
                           cb: &Option<zipzi_core::WarpBezier>|
         -> Result<()> {
            let a = controller(a, pa, ca);
            let b = controller(b, pb, cb);
            for y in 0..17 {
                for x in 0..17 {
                    let u = (x as f32 + 0.5) / 17.;
                    let v = (y as f32 + 0.5) / 17.;
                    let p = a.sample(u, v);
                    let q = b.sample(u, v);
                    let error =
                        (f64::from(p.x) - f64::from(q.x)).hypot(f64::from(p.y) - f64::from(q.y));
                    ensure!(
                        error.is_finite(),
                        "Cannot measure a non-finite controller refit"
                    );
                    maximum = maximum.max(error);
                }
            }
            result.controller_shapes += 1;
            Ok(())
        };
        measure(&wa.points, &wa.bezier, &wb.points, &wb.bezier)?;
        if let Some(binding) = before.binding_for_scene(&object.id) {
            let next = after.get_scene_binding(&binding.id).unwrap();
            if let (SceneTrack::Warp { keyforms: a, .. }, SceneTrack::Warp { keyforms: b, .. }) =
                (&binding.track, &next.track)
            {
                for (a, b) in a.iter().zip(b) {
                    measure(&a.positions, &a.bezier, &b.positions, &b.bezier)?;
                }
            }
        }
        result.maximum_controller_error = Some(maximum);
        Ok(result)
    }
    pub(crate) fn shape_warp(
        &mut self,
        object: &ObjectRef,
        action: zipzi_core::WarpShapeAction,
    ) -> Result<()> {
        self.finish_edit()?;
        let target = match self.property_scope(object) {
            crate::PropertyScope::Object => zipzi_core::document::ShapeTarget::Base,
            crate::PropertyScope::DeformerKeyform { binding, samples } => {
                zipzi_core::document::ShapeTarget::Ordinary {
                    binding_id: binding,
                    samples,
                }
            }
            _ => anyhow::bail!("Select a stored Warp shape before shaping the controller"),
        };
        let snapshot = self.snapshot()?;
        let scene = self.geometry(&snapshot);
        let prepared = crate::control_edit::ControlEdit::prepare(self, &scene, object)?;
        self.begin_control_edit(&[prepared])?;
        let result = (|| {
            self.pending.as_mut().unwrap().edit.apply(|edit| {
                edit.edit_scene_content(zipzi_sdk::SceneContentEdit::WarpShape {
                    transform_id: object.id.clone(),
                    target,
                    action,
                })?;
                Ok(())
            })?;
            self.pending.as_mut().unwrap().topology_changed = true;
            self.invalidate_projection(true);
            self.finish_edit()
        })();
        if result.is_err() {
            self.cancel_edit();
        }
        result
    }
    pub fn warp_grid_preview_metrics(&self, object: &ObjectRef) -> Result<WarpGridPreviewMetrics> {
        ensure!(
            object.kind == ObjectKind::Deformer,
            "Select a Warp deformer"
        );
        let before = self.session.document();
        let after = self.model();
        let old = before
            .get_transform(&object.id)
            .and_then(|t| t.warp())
            .ok_or_else(|| anyhow!("Warp no longer exists"))?;
        let new = after
            .get_transform(&object.id)
            .and_then(|t| t.warp())
            .ok_or_else(|| anyhow!("Warp no longer exists"))?;
        // Off-node cell centers plus corners; fixed work regardless of the
        // selected viewport's zoom or runtime lattice density.
        let mut input = vec![0., 0., 1., 0., 0., 1., 1., 1.];
        for r in 0..17 {
            for c in 0..17 {
                input.extend([(c as f32 + 0.5) / 17., (r as f32 + 0.5) / 17.]);
            }
        }
        let mut result = WarpGridPreviewMetrics {
            samples_per_shape: input.len() / 2,
            ..Default::default()
        };
        let mut measure = |a: &[Vec2], b: &[Vec2]| -> Result<()> {
            let sample = |grid: &WarpTransform, points: &[Vec2]| {
                let flat: Vec<_> = points.iter().flat_map(|p| [p.x, p.y]).collect();
                let mut output = vec![0.; input.len()];
                zipzi_core::deformers::warp_points(
                    grid.rows as i32,
                    grid.columns as i32,
                    grid.quad,
                    &flat,
                    &input,
                    &mut output,
                    input.len() / 2,
                );
                output
            };
            let a = sample(old, a);
            let b = sample(new, b);
            for (a, b) in a.chunks_exact(2).zip(b.chunks_exact(2)) {
                let error =
                    (f64::from(a[0]) - f64::from(b[0])).hypot(f64::from(a[1]) - f64::from(b[1]));
                ensure!(
                    error.is_finite(),
                    "Cannot measure a non-finite Warp preview"
                );
                result.maximum_sample_error = result.maximum_sample_error.max(error);
            }
            result.shapes += 1;
            Ok(())
        };
        measure(&old.points, &new.points)?;
        if let Some(binding) = before.binding_for_scene(&object.id) {
            let next = after
                .get_scene_binding(&binding.id)
                .ok_or_else(|| anyhow!("Binding changed"))?;
            let (SceneTrack::Warp { keyforms: a, .. }, SceneTrack::Warp { keyforms: b, .. }) =
                (&binding.track, &next.track)
            else {
                anyhow::bail!("Not a Warp binding")
            };
            ensure!(a.len() == b.len(), "Keyforms changed");
            for (a, b) in a.iter().zip(b) {
                measure(&a.positions, &b.positions)?;
            }
        }
        for binding in before.iter_blend_bindings_for_target(&object.id) {
            let next = after
                .get_blend_binding(&binding.id)
                .ok_or_else(|| anyhow!("BlendShape changed"))?;
            let (DeltaKeyforms::Warp(a), DeltaKeyforms::Warp(b)) =
                (&binding.keyforms, &next.keyforms)
            else {
                anyhow::bail!("Not a Warp BlendShape")
            };
            ensure!(a.len() == b.len(), "BlendShape keyforms changed");
            for (a, b) in a.iter().zip(b) {
                measure(&a.points, &b.points)?;
            }
        }
        Ok(result)
    }
}
