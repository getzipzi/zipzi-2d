//! Advisory metrics in evaluated model space, separate from authored validity checks.
use std::collections::HashMap;

use zipzi_core::{DrawableFrame, Vec2};

use crate::{ConversionControl, SdkError};

pub use zipzi_contract::authoring::{TopologyOverlay, TopologyThresholds};
pub trait TopologyOverlayValidation {
    fn validate(&self) -> Result<(), SdkError>;
    fn needs_quality(&self) -> bool;
}
pub trait TopologyThresholdValidation {
    fn is_anomaly(&self, triangle: &TriangleQuality) -> bool;
}
impl TopologyOverlayValidation for TopologyOverlay {
    /// Validate transport-independent drawing options before capture or GPU work.
    fn validate(&self) -> Result<(), SdkError> {
        let t = self.thresholds;
        if self.targets.as_ref().is_some_and(|t| t.len() > 256)
            || !self.heat_opacity.is_finite()
            || !(0. ..=1.).contains(&self.heat_opacity)
            || !(1..=4).contains(&self.label_scale)
            || self.label_spacing > 32
            || self.max_labels > 4096
            || !t.min_angle_degrees.is_finite()
            || !(0. ..=60.).contains(&t.min_angle_degrees)
            || !t.max_aspect_ratio.is_finite()
            || t.max_aspect_ratio < 1.
            || !t.max_edge_stretch.is_finite()
            || t.max_edge_stretch < 1.
        {
            return Err(SdkError::new(
                "INVALID_TOPOLOGY_OPTIONS",
                "Invalid topology selection, opacity, label layout or anomaly thresholds",
                "observation",
            ));
        }
        Ok(())
    }
    fn needs_quality(&self) -> bool {
        self.strain || self.anomalies_only
    }
}
impl TopologyThresholdValidation for TopologyThresholds {
    /// Degeneracy, orientation reversal and undefined reference ratios are never hidden.
    fn is_anomaly(&self, t: &TriangleQuality) -> bool {
        t.area == 0.
            || t.relative_area.is_none_or(|a| a < 0.)
            || t.edge_stretch.is_none()
            || t.min_angle_degrees < self.min_angle_degrees
            || t.aspect_ratio.is_none_or(|a| a > self.max_aspect_ratio)
            || t.edge_stretch.is_some_and(|s| s > self.max_edge_stretch)
    }
}

impl crate::AuthoringSnapshot {
    /// Default-parameter evaluation including hidden meshes, for model-space quality comparisons.
    pub fn quality_reference(&self) -> Result<DrawableFrame, SdkError> {
        let mut frame = DrawableFrame::default();
        let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
            self.document(),
            &Default::default(),
            &mut frame,
        );
        if !status.is_ok() {
            return Err(SdkError::from_status(status, "mesh_quality", vec![]));
        }
        Ok(frame)
    }
}

pub use zipzi_contract::authoring::{MeshQuality, TextureCoverage, TriangleQuality};

fn cross(p: [Vec2; 3]) -> f64 {
    let [a, b, c] = p;
    (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
        - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64)
}

fn lengths(p: [Vec2; 3]) -> [f64; 3] {
    std::array::from_fn(|i| {
        let (a, b) = (p[i], p[(i + 1) % 3]);
        (a.x as f64 - b.x as f64).hypot(a.y as f64 - b.y as f64)
    })
}

fn maximum(values: impl Iterator<Item = f64>) -> Option<f64> {
    values.reduce(f64::max)
}

/// Compare identically indexed evaluated topology from one immutable authoring source.
/// References must contain every sampled mesh, including meshes hidden at default parameters.
pub fn mesh_quality(
    frame: &DrawableFrame,
    reference: &DrawableFrame,
    control: &ConversionControl,
) -> Result<Vec<MeshQuality>, SdkError> {
    control
        .check()
        .map_err(|s| SdkError::from_status(s, "mesh_quality", vec![]))?;
    let fail = || {
        SdkError::new(
            "QUALITY_TOPOLOGY_MISMATCH",
            "Quality requires matching finite reference topology",
            "mesh_quality",
        )
    };
    let references: HashMap<_, _> = reference
        .drawables
        .iter()
        .map(|d| (d.id.as_str(), d))
        .collect();
    frame
        .drawables
        .iter()
        .map(|d| {
            let r = references.get(d.id.as_str()).ok_or_else(fail)?;
            if d.indices != r.indices
                || d.positions.len() != r.positions.len()
                || d.indices.len() % 3 != 0
            {
                return Err(fail());
            }
            let mut triangles = Vec::with_capacity(d.indices.len() / 3);
            for (triangle_index, indices) in d.indices.chunks_exact(3).enumerate() {
                if triangle_index % 1024 == 0 {
                    control
                        .check()
                        .map_err(|s| SdkError::from_status(s, "mesh_quality", vec![]))?;
                }
                let points = |positions: &[Vec2]| -> Result<[Vec2; 3], SdkError> {
                    let mut p = [Vec2::default(); 3];
                    for (i, &index) in indices.iter().enumerate() {
                        p[i] = *positions.get(index as usize).ok_or_else(fail)?;
                        if !p[i].x.is_finite() || !p[i].y.is_finite() {
                            return Err(fail());
                        }
                    }
                    Ok(p)
                };
                let (p, base) = (points(&d.positions)?, points(&r.positions)?);
                let (signed, baseline) = (cross(p), cross(base));
                let (edges, base_edges) = (lengths(p), lengths(base));
                let longest = edges.into_iter().fold(0., f64::max);
                let min_angle_degrees = (0..3)
                    .map(|i| {
                        let a = p[i];
                        let b = p[(i + 1) % 3];
                        let c = p[(i + 2) % 3];
                        let u = (b.x as f64 - a.x as f64, b.y as f64 - a.y as f64);
                        let v = (c.x as f64 - a.x as f64, c.y as f64 - a.y as f64);
                        (u.0 * v.1 - u.1 * v.0)
                            .abs()
                            .atan2(u.0 * v.0 + u.1 * v.1)
                            .to_degrees()
                    })
                    .fold(180., f64::min);
                triangles.push(TriangleQuality {
                    triangle_index,
                    area: signed.abs() * 0.5,
                    min_angle_degrees,
                    aspect_ratio: (signed != 0.).then(|| longest * longest / signed.abs()),
                    relative_area: (baseline != 0.).then(|| signed / baseline),
                    edge_stretch: edges
                        .iter()
                        .zip(base_edges)
                        .all(|(a, b)| *a > 0. && b > 0.)
                        .then(|| {
                            edges
                                .iter()
                                .zip(base_edges)
                                .map(|(a, b)| (a / b).max(b / a))
                                .fold(1., f64::max)
                        }),
                });
            }
            Ok(MeshQuality {
                mesh_id: d.id.clone(),
                visible: d.enabled && d.visible && d.opacity > 0.,
                vertex_count: d.positions.len(),
                min_angle_degrees: triangles
                    .iter()
                    .map(|t| t.min_angle_degrees)
                    .reduce(f64::min),
                max_aspect_ratio: maximum(triangles.iter().filter_map(|t| t.aspect_ratio)),
                max_edge_stretch: maximum(triangles.iter().filter_map(|t| t.edge_stretch)),
                degenerate_triangles: triangles.iter().filter(|t| t.area == 0.).count(),
                flipped_triangles: triangles
                    .iter()
                    .filter(|t| t.relative_area.is_some_and(|a| a < 0.))
                    .count(),
                texture_coverage: None,
                triangles,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn topology_options_validate_bounds_and_classify_shape_and_reference_anomalies() {
        let options = TopologyOverlay::default();
        options.validate().unwrap();
        assert_eq!(
            serde_json::from_str::<TopologyOverlay>("{}").unwrap(),
            options
        );
        let valid = TriangleQuality {
            triangle_index: 0,
            area: 0.5,
            min_angle_degrees: 45.,
            aspect_ratio: Some(2.),
            relative_area: Some(1.),
            edge_stretch: Some(1.),
        };
        let t = TopologyThresholds::default();
        assert!(!t.is_anomaly(&valid));
        assert!(t.is_anomaly(&TriangleQuality {
            min_angle_degrees: 1.,
            ..valid.clone()
        }));
        assert!(t.is_anomaly(&TriangleQuality {
            aspect_ratio: Some(100.),
            ..valid.clone()
        }));
        assert!(t.is_anomaly(&TriangleQuality {
            edge_stretch: Some(3.),
            ..valid.clone()
        }));
        assert!(t.is_anomaly(&TriangleQuality {
            relative_area: Some(-1.),
            ..valid.clone()
        }));
        assert!(t.is_anomaly(&TriangleQuality {
            relative_area: None,
            ..valid.clone()
        }));
        assert!(t.is_anomaly(&TriangleQuality {
            edge_stretch: None,
            ..valid.clone()
        }));
        for value in [-0.1, 1.1, f32::NAN] {
            assert!(TopologyOverlay {
                heat_opacity: value,
                ..options.clone()
            }
            .validate()
            .is_err());
        }
        assert!(TopologyOverlay {
            label_scale: 0,
            ..options.clone()
        }
        .validate()
        .is_err());
        assert!(TopologyOverlay {
            max_labels: 4097,
            ..options.clone()
        }
        .validate()
        .is_err());
        assert!(TopologyOverlay {
            thresholds: TopologyThresholds {
                min_angle_degrees: f64::NAN,
                ..t
            },
            ..options.clone()
        }
        .validate()
        .is_err());
        assert!(TopologyOverlay {
            targets: Some(vec![crate::ObjectTarget::Mesh("id".into()); 257]),
            ..options
        }
        .validate()
        .is_err());
    }
    fn frame(p: [[f32; 2]; 3]) -> DrawableFrame {
        let mut frame = DrawableFrame::default();
        frame.drawables.push(zipzi_core::Drawable {
            id: "mesh".into(),
            positions: p.map(|[x, y]| Vec2::new(x, y)).to_vec(),
            indices: vec![0, 1, 2].into(),
            ..Default::default()
        });
        frame
    }
    #[test]
    fn detects_skinny_flipped_collapsed_and_stretched_triangles() {
        let base = frame([[0., 0.], [1., 0.], [0., 1.]]);
        let check = |f: &DrawableFrame| {
            mesh_quality(f, &base, &ConversionControl::default())
                .unwrap()
                .remove(0)
        };
        let same = check(&base);
        assert!((same.min_angle_degrees.unwrap() - 45.).abs() < 1e-10);
        assert_eq!(same.max_edge_stretch, Some(1.));
        assert!((same.max_aspect_ratio.unwrap() - 2.).abs() < 1e-10);
        let stretched = check(&frame([[3., 4.], [13., 4.], [3., 4.01]]));
        assert!(stretched.min_angle_degrees.unwrap() < 0.1);
        assert!(stretched.max_aspect_ratio.unwrap() > 999.);
        assert!(stretched.max_edge_stretch.unwrap() > 99.);
        assert_eq!(
            check(&frame([[0., 0.], [0., 1.], [1., 0.]])).flipped_triangles,
            1
        );
        let collapsed = check(&frame([[0., 0.], [1., 0.], [2., 0.]]));
        assert_eq!(collapsed.degenerate_triangles, 1);
        assert_eq!(collapsed.triangles[0].aspect_ratio, None);
        let q = mesh_quality(
            &base,
            &frame([[0., 0.], [0., 0.], [0., 0.]]),
            &ConversionControl::default(),
        )
        .unwrap();
        assert_eq!(q[0].triangles[0].relative_area, None);
        assert_eq!(q[0].triangles[0].edge_stretch, None);
        serde_json::to_string(&q).unwrap();
    }
    #[test]
    fn rejects_other_topology_and_honors_cancellation() {
        let base = frame([[0., 0.], [1., 0.], [0., 1.]]);
        let mut different = base.clone();
        different.drawables[0].indices = vec![1, 0, 2].into();
        assert_eq!(
            mesh_quality(&different, &base, &ConversionControl::default())
                .unwrap_err()
                .code
                .as_ref(),
            "QUALITY_TOPOLOGY_MISMATCH"
        );
        let control = ConversionControl::default();
        control.cancel();
        assert!(mesh_quality(&base, &base, &control).is_err());
    }
}
