use std::collections::HashMap;

use zipzi_core::Vec2;

use crate::{AuthoringSession, SdkError};

#[derive(
    Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape,
)]
pub struct GeometryBounds {
    /// Minimum XY corner of the geometry bounds in the snapshot coordinate space.
    pub min: Vec2,
    /// Maximum XY corner of the geometry bounds in the snapshot coordinate space.
    pub max: Vec2,
}

#[derive(
    Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, zipzi_api_derive::ApiShape,
)]
#[serde(default)]
pub struct GeometryChecks {
    /// Area in authored coordinates: model units squared at the root, or
    /// the appropriate rotation-local / Warp-parameter units under a parent.
    pub min_triangle_area: f64,
    /// Optional expected model bounds for root meshes only.
    pub model_bounds: Option<GeometryBounds>,
    /// Include per-pose mesh quality in evaluated Y-up model space, relative to this
    /// snapshot evaluated with default parameters. Only the requested poses are checked.
    pub quality: bool,
}

impl Default for GeometryChecks {
    fn default() -> Self {
        Self {
            min_triangle_area: 0.0,
            model_bounds: None,
            quality: false,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub enum GeometryDiagnosticKind {
    SmallTriangle,
    InconsistentWinding,
    OutsideModelBounds,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct GeometryDiagnostic {
    /// Typed geometry diagnostic category.
    pub kind: GeometryDiagnosticKind,
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Optional zero-based index of the offending triangle; None means the diagnostic is not
    /// triangle-specific.
    pub triangle_index: Option<usize>,
}

impl AuthoringSession {
    /// Advisory source-geometry checks. These findings never reject an edit.
    pub fn diagnose_geometry(
        &self,
        checks: GeometryChecks,
    ) -> Result<Vec<GeometryDiagnostic>, SdkError> {
        diagnose_geometry(
            self.document(),
            checks,
            &crate::ConversionControl::default(),
        )
    }
}
impl crate::AuthoringSnapshot {
    pub fn diagnose_geometry(
        &self,
        checks: GeometryChecks,
    ) -> Result<Vec<GeometryDiagnostic>, SdkError> {
        diagnose_geometry(
            self.document(),
            checks,
            &crate::ConversionControl::default(),
        )
    }
    pub fn diagnose_geometry_with_control(
        &self,
        checks: GeometryChecks,
        control: &crate::ConversionControl,
    ) -> Result<Vec<GeometryDiagnostic>, SdkError> {
        diagnose_geometry(self.document(), checks, control)
    }
}
fn diagnose_geometry(
    document: &zipzi_core::Document,
    checks: GeometryChecks,
    control: &crate::ConversionControl,
) -> Result<Vec<GeometryDiagnostic>, SdkError> {
    if !checks.min_triangle_area.is_finite() || checks.min_triangle_area < 0.0 {
        return Err(SdkError::new(
            "INVALID_DIAGNOSTIC_OPTIONS",
            "Minimum triangle area must be finite and non-negative",
            "diagnose_geometry",
        ));
    }
    if let Some(bounds) = checks.model_bounds {
        let finite = [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
            .into_iter()
            .all(f32::is_finite);
        if !finite || bounds.min.x >= bounds.max.x || bounds.min.y >= bounds.max.y {
            return Err(SdkError::new(
                "INVALID_DIAGNOSTIC_OPTIONS",
                "Canvas bounds require finite min below max",
                "diagnose_geometry",
            ));
        }
    }
    if let Some(issue) = document.validate_structure().into_iter().next() {
        return Err(SdkError::from_status(
            issue.status,
            "diagnose_geometry",
            vec![issue.object_id],
        ));
    }
    let mut diagnostics = Vec::new();
    for mesh_id in document.mesh_order() {
        control
            .check()
            .map_err(|s| SdkError::from_status(s, "diagnose_geometry", vec![]))?;
        let mesh = document.get_mesh(mesh_id).expect("validated mesh exists");
        let slots: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(index, &vertex)| (vertex, index))
            .collect();
        let mut signed_areas = Vec::with_capacity(mesh.triangles.len());
        let mut positive = 0usize;
        let mut negative = 0usize;
        for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
            if triangle_index % 1024 == 0 {
                control
                    .check()
                    .map_err(|s| SdkError::from_status(s, "diagnose_geometry", vec![]))?;
            }
            let a = mesh.base_positions[slots[&triangle[0]]];
            let b = mesh.base_positions[slots[&triangle[1]]];
            let c = mesh.base_positions[slots[&triangle[2]]];
            let cross = (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
                - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64);
            let area = cross * 0.5;
            if area.abs() <= checks.min_triangle_area {
                diagnostics.push(GeometryDiagnostic {
                    kind: GeometryDiagnosticKind::SmallTriangle,
                    mesh_id: mesh_id.clone(),
                    triangle_index: Some(triangle_index),
                });
            } else if area > 0.0 {
                positive += 1;
            } else {
                negative += 1;
            }
            signed_areas.push(area);
        }
        let expected_positive = positive >= negative;
        if positive > 0 && negative > 0 {
            for (triangle_index, area) in signed_areas.into_iter().enumerate() {
                if area.abs() > checks.min_triangle_area && (area > 0.0) != expected_positive {
                    diagnostics.push(GeometryDiagnostic {
                        kind: GeometryDiagnosticKind::InconsistentWinding,
                        mesh_id: mesh_id.clone(),
                        triangle_index: Some(triangle_index),
                    });
                }
            }
        }
        if mesh.deformer_id.is_empty()
            && checks.model_bounds.is_some_and(|bounds| {
                mesh.base_positions.iter().any(|p| {
                    p.x < bounds.min.x
                        || p.x > bounds.max.x
                        || p.y < bounds.min.y
                        || p.y > bounds.max.y
                })
            })
        {
            diagnostics.push(GeometryDiagnostic {
                kind: GeometryDiagnosticKind::OutsideModelBounds,
                mesh_id: mesh_id.clone(),
                triangle_index: None,
            });
        }
    }
    Ok(diagnostics)
}
