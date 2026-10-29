//! Frozen inverses for model-space topology gestures. No authoring pose is selected.
use crate::{CanvasFrame, Document, ModelPoint, ModelVector, control_edit::ControlSpace};
use anyhow::{Result, bail, ensure};
use std::{collections::HashMap, sync::Arc};
use zipzi_core::{Vec2, VertexId};
use zipzi_sdk::{AffineUvMap, ManualVertexTarget, MeshGeometry};

struct Triangle {
    positions: [Vec2; 3],
    uvs: [Vec2; 3],
    bounds: [f32; 4],
}

fn weights(point: Vec2, [a, b, c]: [Vec2; 3]) -> Option<[f64; 3]> {
    let cross = |a: Vec2, b: Vec2, c: Vec2| {
        (f64::from(b.x) - f64::from(a.x)) * (f64::from(c.y) - f64::from(a.y))
            - (f64::from(b.y) - f64::from(a.y)) * (f64::from(c.x) - f64::from(a.x))
    };
    let det = cross(a, b, c);
    if det == 0. {
        return None;
    }
    let mut w = [
        cross(point, b, c) / det,
        cross(a, point, c) / det,
        cross(a, b, point) / det,
    ];
    if w.iter().any(|w| *w < -1e-6 || *w > 1. + 1e-6) {
        return None;
    }
    for w in &mut w {
        *w = w.max(0.);
    }
    let sum: f64 = w.iter().sum();
    Some(w.map(|w| w / sum))
}

fn sample(w: [f64; 3], points: [Vec2; 3]) -> Vec2 {
    Vec2::new(
        w.iter()
            .zip(points)
            .map(|(w, p)| w * f64::from(p.x))
            .sum::<f64>() as f32,
        w.iter()
            .zip(points)
            .map(|(w, p)| w * f64::from(p.y))
            .sum::<f64>() as f32,
    )
}

pub(super) struct PositionUvMap {
    triangles: Vec<Triangle>,
    affine: Option<AffineUvMap>,
}

impl PositionUvMap {
    fn new(geometry: MeshGeometry) -> Self {
        let slots: HashMap<_, _> = geometry
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, &v)| (v, i))
            .collect();
        let triangles = geometry
            .triangles
            .iter()
            .map(|ids| {
                let positions = ids.map(|id| geometry.positions[slots[&id]]);
                Triangle {
                    positions,
                    uvs: ids.map(|id| geometry.uvs[slots[&id]]),
                    bounds: [
                        positions.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
                        positions.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
                        positions
                            .iter()
                            .map(|p| p.x)
                            .fold(f32::NEG_INFINITY, f32::max),
                        positions
                            .iter()
                            .map(|p| p.y)
                            .fold(f32::NEG_INFINITY, f32::max),
                    ],
                }
            })
            .collect();
        Self {
            triangles,
            affine: AffineUvMap::from_geometry(&geometry).ok(),
        }
    }

    pub(super) fn displayed(
        document: &Document,
        frame: &CanvasFrame,
        mesh_id: &str,
    ) -> Result<Self> {
        let model = document.model();
        let mesh = model
            .get_mesh(mesh_id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        let shown = frame
            .geometry
            .mesh(mesh_id)
            .ok_or_else(|| anyhow::anyhow!("Mesh is unavailable in this view"))?;
        Ok(Self::new(MeshGeometry {
            vertex_ids: mesh.vertex_ids.clone(),
            positions: shown
                .vertices
                .iter()
                .map(|(_, p)| Vec2::new(p.x, p.y))
                .collect(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            meshing: None,
        }))
    }

    /// Refuse overlapping UV branches instead of picking a triangle by draw order.
    pub(super) fn uv(&self, point: Vec2) -> Result<Vec2> {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid model position"
        );
        let mut result: Option<Vec2> = None;
        for t in &self.triangles {
            let [x0, y0, x1, y1] = t.bounds;
            let eps = f32::EPSILON * point.x.abs().max(point.y.abs()).max(1.) * 4.;
            if point.x < x0 - eps || point.y < y0 - eps || point.x > x1 + eps || point.y > y1 + eps
            {
                continue;
            }
            let Some(w) = weights(point, t.positions) else {
                continue;
            };
            let uv = sample(w, t.uvs);
            if let Some(previous) = result {
                ensure!(
                    (previous.x - uv.x).hypot(previous.y - uv.y) < 1e-5,
                    "This pose overlaps multiple UV regions; use Edit on source image"
                );
            } else {
                result = Some(uv);
            }
        }
        if let Some(uv) = result {
            return Ok(uv);
        }
        // A globally affine image can still be edited beyond its current outline.
        // The SDK separately enforces the source image and dependency transfer domain.
        if let Some(affine) = self.affine {
            return Ok(affine.uv(point)?);
        }
        bail!("This position is outside the invertible mesh surface; use Edit on source image")
    }
    fn position(&self, uv: Vec2) -> Result<Vec2> {
        for t in &self.triangles {
            if let Some(w) = weights(uv, t.uvs) {
                return Ok(sample(w, t.positions));
            }
        }
        if let Some(affine) = self.affine {
            return Ok(affine.position(uv));
        }
        bail!("This UV is outside the source mesh")
    }
}

pub(super) struct PoseMapping {
    mesh_id: String,
    local: HashMap<VertexId, Vec2>,
    space: ControlSpace,
    uv: PositionUvMap,
    glue: Arc<crate::mesh_edit::GlueMap>,
}

impl PoseMapping {
    pub(super) fn capture(document: &Document, frame: &CanvasFrame, mesh_id: &str) -> Result<Self> {
        let model = document.model();
        let mesh = model
            .get_mesh(mesh_id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        let local = zipzi_core::evaluation::authoring_mesh_positions(
            &model,
            mesh_id,
            frame.geometry.parameter_values(),
        )
        .map_err(|e| anyhow::anyhow!("{}", e.message))?;
        let uv = PositionUvMap::new(MeshGeometry {
            vertex_ids: mesh.vertex_ids.clone(),
            positions: local.clone(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            meshing: None,
        });
        Ok(Self {
            mesh_id: mesh_id.into(),
            space: ControlSpace::new(document, &frame.geometry, &mesh.deformer_id)?,
            local: mesh.vertex_ids.iter().copied().zip(local).collect(),
            uv,
            glue: frame.geometry.glue_map.clone(),
        })
    }

    pub(super) fn move_targets(
        &self,
        starts: &[ManualVertexTarget],
        delta: ModelVector,
    ) -> Result<Vec<ManualVertexTarget>> {
        if delta.x == 0. && delta.y == 0. {
            return Ok(starts.to_vec());
        }
        let requested: Vec<_> = starts
            .iter()
            .map(|v| ((self.mesh_id.clone(), v.id), delta))
            .collect();
        let solved = self.glue.solve(&requested)?;
        starts
            .iter()
            .zip(solved)
            .map(|(v, delta)| {
                let local = self.local[&v.id];
                let before = self.space.to_model(local);
                let after = ModelPoint {
                    x: before.x + delta.x,
                    y: before.y + delta.y,
                };
                let local = self.inverse_parent(after, local)?;
                Ok(ManualVertexTarget {
                    id: v.id,
                    uv: self.uv.uv(local)?,
                })
            })
            .collect()
    }
    /// New vertices have no Glue endpoint. Refine the picked triangle's UV
    /// through the parent inverse so nonlinear deformation keeps the cursor target.
    pub(super) fn insert_uv(&self, point: ModelPoint, initial: Vec2) -> Result<Vec2> {
        let local = self.inverse_parent(point, self.uv.position(initial)?)?;
        self.uv.uv(local)
    }
    fn inverse_parent(&self, point: ModelPoint, initial: Vec2) -> Result<Vec2> {
        // UV transfer needs a tighter inverse than ordinary pose editing, while
        // respecting the attainable f32 precision at large model coordinates.
        let tolerance = (point.x.abs().max(point.y.abs()) * f32::EPSILON * 2.).max(2e-6);
        self.space
            .from_model_with_tolerance(point, initial, tolerance)
    }
}
