//! Advisory metrics in evaluated model space, separate from authored validity checks.
use std::collections::HashMap;

use zipzi_core::{DrawableFrame, Vec2};

use crate::{ConversionControl, SdkError};

/// Explicit diagnostic drawing options; overlays ignore masks and occlusion.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(default, deny_unknown_fields)]
pub struct TopologyOverlay {
    /// Diagnostic selection only; the textured context image is unchanged. None selects all
    /// meshes, Some([]) selects none. Mesh/Part targets are unioned and deduplicated;
    /// Parts include nested Parts. Missing targets fail explicitly. Maximum 256 selectors.
    pub targets: Option<Vec<crate::ObjectTarget>>,
    /// Draw triangle edges in cyan, in final evaluated model coordinates.
    #[serde(default = "default_wireframe")]
    pub wireframe: bool,
    /// Draw vertex positions as yellow dots.
    pub vertices: bool,
    /// Draw stable authoring vertex IDs in yellow, rather than transient array offsets.
    pub vertex_ids: bool,
    /// Draw meshing boundary edges in green and internal constraints in magenta.
    pub constraints: bool,
    /// Fill triangles by symmetric edge stretch relative to the default pose: blue=1,
    /// orange=2, red>=4; flipped or collapsed triangles are magenta.
    pub strain: bool,
    /// Include disabled, hidden and zero-opacity meshes. Default draws visible meshes only.
    pub include_hidden: bool,
    /// Clear the textured image before diagnostic drawing, retaining the exact same view.
    pub topology_only: bool,
    /// Heat fill opacity in [0,1]. Applied once per pixel using the most severe overlapping
    /// triangle, so overlapping layers cannot accumulate opacity. Lines and labels stay opaque.
    #[serde(default = "default_heat_opacity")]
    pub heat_opacity: f32,
    /// Restrict every diagnostic layer to anomalous triangles and their incident vertices/edges.
    /// Degenerate, flipped and undefined-reference triangles always count as anomalies.
    pub anomalies_only: bool,
    /// Thresholds used by anomalies_only; reports themselves are never threshold-filtered.
    pub thresholds: TopologyThresholds,
    /// Decimal glyph scale in output pixels, 1..4. Labels avoid other labels globally;
    /// an ID is omitted when no nearby in-view position has enough space.
    #[serde(default = "default_label_scale")]
    pub label_scale: u32,
    /// Minimum clear gap between label rectangles in pixels, 0..32.
    #[serde(default = "default_label_spacing")]
    pub label_spacing: u32,
    /// Maximum labels per image, 0..4096. Vertex dots remain independent of this limit.
    #[serde(default = "default_max_labels")]
    pub max_labels: usize,
}
fn default_heat_opacity() -> f32 {
    0.25
}
fn default_label_scale() -> u32 {
    2
}
fn default_label_spacing() -> u32 {
    3
}
fn default_max_labels() -> usize {
    256
}
fn default_wireframe() -> bool {
    true
}
impl Default for TopologyOverlay {
    fn default() -> Self {
        Self {
            targets: None,
            wireframe: true,
            vertices: false,
            vertex_ids: false,
            constraints: false,
            strain: false,
            include_hidden: false,
            topology_only: false,
            heat_opacity: default_heat_opacity(),
            anomalies_only: false,
            thresholds: TopologyThresholds::default(),
            label_scale: default_label_scale(),
            label_spacing: default_label_spacing(),
            max_labels: default_max_labels(),
        }
    }
}

/// Advisory anomaly thresholds in evaluated model space, relative to the default pose.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(default, deny_unknown_fields)]
pub struct TopologyThresholds {
    /// An angle below this value is anomalous; finite degrees in [0,60]. Default 10.
    #[serde(default = "default_min_angle")]
    pub min_angle_degrees: f64,
    /// A longest-edge/altitude ratio above this is anomalous; finite and >= 1. Default 20.
    #[serde(default = "default_max_aspect")]
    pub max_aspect_ratio: f64,
    /// A symmetric edge-length ratio above this is anomalous; finite and >= 1. Default 2.
    #[serde(default = "default_max_stretch")]
    pub max_edge_stretch: f64,
}
fn default_min_angle() -> f64 {
    10.
}
fn default_max_aspect() -> f64 {
    20.
}
fn default_max_stretch() -> f64 {
    2.
}
impl Default for TopologyThresholds {
    fn default() -> Self {
        Self {
            min_angle_degrees: default_min_angle(),
            max_aspect_ratio: default_max_aspect(),
            max_edge_stretch: default_max_stretch(),
        }
    }
}
impl TopologyOverlay {
    /// Validate transport-independent drawing options before capture or GPU work.
    pub fn validate(&self) -> Result<(), SdkError> {
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
    pub fn needs_quality(&self) -> bool {
        self.strain || self.anomalies_only
    }
}
impl TopologyThresholds {
    /// Degeneracy, orientation reversal and undefined reference ratios are never hidden.
    pub fn is_anomaly(&self, t: &TriangleQuality) -> bool {
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

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct TriangleQuality {
    /// Zero-based triangle offset, in the sampled drawable's index order.
    pub triangle_index: usize,
    /// Unsigned evaluated area in model units squared.
    pub area: f64,
    /// Smallest interior angle in degrees; zero for a collapsed triangle.
    pub min_angle_degrees: f64,
    /// Longest edge divided by its altitude. None for a collapsed triangle.
    pub aspect_ratio: Option<f64>,
    /// Signed area / signed default-pose area. Negative means flipped; None if the reference is degenerate.
    pub relative_area: Option<f64>,
    /// Largest symmetric edge-length ratio max(current/reference, reference/current).
    /// None if any reference or sampled edge is collapsed. A rigid transform gives 1.
    pub edge_stretch: Option<f64>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct TextureCoverage {
    /// Alpha >= this value counts as painted. Coverage ignores filtering, masks and occlusion.
    pub alpha_threshold: u8,
    /// Top-down source-pixel grid spacing. Values above 1 mean an approximate report.
    pub pixel_step: u32,
    /// Number of source pixel centers tested on the regular grid.
    pub sampled_pixels: usize,
    /// Tested pixel centers covered by the union of UV triangles inside [0,1].
    pub covered_pixels: usize,
    /// Covered centers whose source alpha is below the threshold.
    pub transparent_covered_pixels: usize,
    /// Tested centers whose source alpha is at least the threshold.
    pub opaque_pixels: usize,
    /// Opaque centers outside the UV triangle union: a contour coverage error, not a distance.
    pub uncovered_opaque_pixels: usize,
    /// Transparent covered / covered; None if no tested center is covered.
    pub transparent_coverage_fraction: Option<f64>,
    /// Uncovered opaque / opaque; None for an entirely transparent sampled image.
    pub uncovered_opaque_fraction: Option<f64>,
    /// Number of UV triangles with vertices outside [0,1]; repeat-sampled coverage is not measured.
    pub out_of_bounds_triangles: usize,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct MeshQuality {
    /// Stable document mesh ID, in evaluated drawable order.
    pub mesh_id: String,
    /// Evaluated enabled/visible/positive-opacity state. Metrics are geometric and ignore occlusion.
    pub visible: bool,
    /// Number of evaluated vertices.
    pub vertex_count: usize,
    /// Per-triangle metrics in evaluated Y-up model coordinates, relative to default parameters
    /// of the same frozen authoring source. This does not certify unsampled parameter intervals.
    pub triangles: Vec<TriangleQuality>,
    /// Smallest angle over this mesh; None for an empty mesh.
    pub min_angle_degrees: Option<f64>,
    /// Largest finite aspect ratio; collapsed triangles are counted separately.
    pub max_aspect_ratio: Option<f64>,
    /// Largest finite symmetric edge-length ratio relative to the default pose.
    pub max_edge_stretch: Option<f64>,
    /// Number of exactly zero-area evaluated triangles.
    pub degenerate_triangles: usize,
    /// Number of negative relative-area triangles. Degenerate references are excluded.
    pub flipped_triangles: usize,
    /// Optional source-texture coverage. CPU-only sampling does not resolve image pixels.
    pub texture_coverage: Option<TextureCoverage>,
}

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
