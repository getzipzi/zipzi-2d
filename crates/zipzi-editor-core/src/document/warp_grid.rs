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
}

impl Document {
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
