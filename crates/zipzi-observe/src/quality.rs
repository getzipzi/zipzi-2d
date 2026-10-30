//! Source-alpha coverage is measured in UV space, independently of rendered visibility.
use crate::{ObservationError, RenderRequest, ResolvedObservation};
use zipzi_sdk::{MeshQuality, TextureCoverage};

impl ResolvedObservation {
    pub(crate) fn quality(
        &self,
        request: &RenderRequest,
    ) -> Result<Vec<MeshQuality>, ObservationError> {
        if !request.needs_quality() {
            return Ok(vec![]);
        }
        let reference = self
            .input
            .reference_frame
            .as_ref()
            .ok_or_else(|| ObservationError {
                code: "QUALITY_REFERENCE_REQUIRED".into(),
                message:
                    "Capture with quality enabled before requesting reports or strain overlays"
                        .into(),
                asset_id: None,
            })?;
        let mut reports =
            zipzi_sdk::mesh_quality(&self.input.frame, reference, &Default::default()).map_err(
                |e| ObservationError {
                    code: e.code.into(),
                    message: e.message.into(),
                    asset_id: None,
                },
            )?;
        let mut work = 8_000_000usize;
        let selected = crate::topology::selection(
            &self.input.authoring.meshes,
            &self.input.authoring.parts,
            request.topology.as_ref(),
        )?;
        if let Some(selected) = &selected {
            reports.retain(|r| selected.contains(&r.mesh_id));
        }
        if request.quality {
            for report in &mut reports {
                let d = self
                    .input
                    .frame
                    .drawables
                    .iter()
                    .find(|d| d.id == report.mesh_id)
                    .expect("quality belongs to frame");
                if let Some(texture) = self
                    .textures
                    .iter()
                    .find(|t| t.asset.id == d.texture_asset_id)
                {
                    let samples =
                        texture
                            .source_alpha
                            .as_ref()
                            .ok_or_else(|| {
                                ObservationError {
                            code: "SOURCE_ALPHA_REQUIRED".into(),
                            message:
                                "Capture source alpha samples before requesting texture coverage"
                                    .into(),
                            asset_id: Some(texture.asset.id.clone()),
                        }
                            })?;
                    report.texture_coverage =
                        Some(coverage_samples(&d.uvs, &d.indices, samples, &mut work)?);
                }
            }
        }
        Ok(reports)
    }
}

#[cfg(test)]
fn coverage(
    uvs: &[zipzi_core::Vec2],
    indices: &[u32],
    width: u32,
    height: u32,
    rgba: &[u8],
    work: &mut usize,
) -> Result<TextureCoverage, ObservationError> {
    coverage_samples(
        uvs,
        indices,
        &crate::alpha::SourceAlphaSamples::extract(width, height, rgba, &Default::default())?,
        work,
    )
}

fn coverage_samples(
    uvs: &[zipzi_core::Vec2],
    indices: &[u32],
    samples: &crate::alpha::SourceAlphaSamples,
    work: &mut usize,
) -> Result<TextureCoverage, ObservationError> {
    let crate::alpha::SourceAlphaSamples {
        source_size,
        step,
        cols,
        rows,
        alpha,
        version,
    } = samples;
    assert_eq!(*version, crate::alpha::SOURCE_ALPHA_SAMPLING_VERSION);
    let (width, height, step, cols, rows) =
        (source_size.width, source_size.height, *step, *cols, *rows);
    let mut covered = vec![false; cols * rows];
    let mut outside = 0;
    for tri in indices.chunks_exact(3) {
        let p = std::array::from_fn::<_, 3, _>(|slot| {
            let i = tri[slot];
            let uv = uvs[i as usize];
            (
                uv.x as f64 * width as f64,
                (1. - uv.y as f64) * height as f64,
            )
        });
        if tri.iter().any(|i| {
            let uv = uvs[*i as usize];
            uv.x < 0. || uv.x > 1. || uv.y < 0. || uv.y > 1.
        }) {
            outside += 1;
        }
        let signed = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[1].1 - p[0].1) * (p[2].0 - p[0].0);
        if signed == 0. {
            continue;
        }
        let y0 = p.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let y1 = p.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        let first = ((y0 - 0.5) / step as f64).ceil().clamp(0., rows as f64) as usize;
        let end = (((y1 - 0.5) / step as f64).floor() + 1.).clamp(0., rows as f64) as usize;
        for gy in first..end {
            if *work == 0 {
                return Err(ObservationError { code: "QUALITY_BUDGET_EXCEEDED".into(),
                message: "Alpha coverage exceeded 8000000 triangle scanlines; use CPU quality or simpler topology".into(), asset_id: None });
            }
            *work -= 1;
            let y = (gy as u32 * step) as f64 + 0.5;
            let mut min = f64::INFINITY;
            let mut max = f64::NEG_INFINITY;
            for i in 0..3 {
                let (a, b) = (p[i], p[(i + 1) % 3]);
                if a.1 != b.1 && y >= a.1.min(b.1) && y <= a.1.max(b.1) {
                    let x = a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0);
                    min = min.min(x);
                    max = max.max(x);
                }
            }
            if min > max {
                continue;
            }
            let x0 = ((min - 0.5) / step as f64).ceil().clamp(0., cols as f64) as usize;
            let x1 = (((max - 0.5) / step as f64).floor() + 1.).clamp(0., cols as f64) as usize;
            covered[gy * cols + x0..gy * cols + x1].fill(true);
        }
    }
    let (
        mut covered_pixels,
        mut transparent_covered_pixels,
        mut opaque_pixels,
        mut uncovered_opaque_pixels,
    ) = (0, 0, 0, 0);
    for gy in 0..rows {
        for gx in 0..cols {
            let opaque = alpha[gy * cols + gx] >= 8;
            let inside = covered[gy * cols + gx];
            covered_pixels += usize::from(inside);
            transparent_covered_pixels += usize::from(inside && !opaque);
            opaque_pixels += usize::from(opaque);
            uncovered_opaque_pixels += usize::from(opaque && !inside);
        }
    }
    Ok(TextureCoverage {
        alpha_threshold: 8,
        pixel_step: step,
        sampled_pixels: cols * rows,
        covered_pixels,
        transparent_covered_pixels,
        opaque_pixels,
        uncovered_opaque_pixels,
        transparent_coverage_fraction: (covered_pixels > 0)
            .then(|| transparent_covered_pixels as f64 / covered_pixels as f64),
        uncovered_opaque_fraction: (opaque_pixels > 0)
            .then(|| uncovered_opaque_pixels as f64 / opaque_pixels as f64),
        out_of_bounds_triangles: outside,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zipzi_core::Vec2;
    #[test]
    fn alpha_coverage_uses_union_and_top_down_pixel_centers() {
        let uv = [
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ];
        let rgba = [0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255];
        let mut work = 100;
        let q = coverage(&uv, &[0, 1, 2, 0, 2, 3, 0, 1, 2], 2, 2, &rgba, &mut work).unwrap();
        assert_eq!(
            (
                q.covered_pixels,
                q.transparent_covered_pixels,
                q.opaque_pixels,
                q.uncovered_opaque_pixels
            ),
            (4, 2, 2, 0)
        );
        assert_eq!(q.transparent_coverage_fraction, Some(0.5));
        let q = coverage(&uv, &[], 2, 2, &rgba, &mut work).unwrap();
        assert_eq!(q.uncovered_opaque_fraction, Some(1.));
        assert_eq!(q.transparent_coverage_fraction, None);
        assert!(coverage(&uv, &[0, 1, 2], 2, 2, &rgba, &mut 0).is_err());
    }
    #[test]
    fn coverage_records_sampling_and_outside_uvs() {
        let uv = [Vec2::new(-1., -1.), Vec2::new(2., -1.), Vec2::new(0., 2.)];
        let q = coverage(
            &uv,
            &[0, 1, 2],
            512,
            256,
            &vec![0; 512 * 256 * 4],
            &mut 10000,
        )
        .unwrap();
        assert_eq!(q.pixel_step, 2);
        assert_eq!(q.sampled_pixels, 256 * 128);
        assert_eq!(q.out_of_bounds_triangles, 1);
        assert_eq!(q.uncovered_opaque_fraction, None);
    }

    #[test]
    fn top_left_alpha_is_matched_to_y_up_uvs() {
        let uv = [Vec2::new(0., 1.), Vec2::new(0.5, 1.), Vec2::new(0., 0.5)];
        let mut rgba = vec![0; 4 * 4 * 4];
        rgba[3] = 255;
        let q = coverage(&uv, &[0, 1, 2], 4, 4, &rgba, &mut 100).unwrap();
        assert_eq!(q.covered_pixels, 3);
        assert_eq!(q.uncovered_opaque_pixels, 0);
        assert_eq!(q.transparent_covered_pixels, 2);
    }
}
