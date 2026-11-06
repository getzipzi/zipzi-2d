//! Frozen inverses for model-space topology gestures. No authoring pose is selected.
use crate::{CanvasFrame, Document, ModelPoint, ModelVector, control_edit::ControlSpace};
use anyhow::Result;
use std::{collections::HashMap, sync::Arc};
use zipzi_contract::authoring::ManualVertexTarget;
use zipzi_contract::authoring::MeshGeometry;
use zipzi_core::{Vec2, VertexId};
use zipzi_sdk::ManualMeshUvMap;

pub(super) struct PositionUvMap {
    mapping: ManualMeshUvMap,
}

impl PositionUvMap {
    fn new(geometry: MeshGeometry) -> Result<Self> {
        Ok(Self {
            mapping: ManualMeshUvMap::new(geometry)?,
        })
    }

    pub(super) fn displayed(
        document: &Document,
        frame: &CanvasFrame,
        mesh_id: &str,
    ) -> Result<Self> {
        let model = document.model();
        let mesh = model
            .mesh(mesh_id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        let shown = frame
            .geometry
            .mesh(mesh_id)
            .ok_or_else(|| anyhow::anyhow!("Mesh is unavailable in this view"))?;
        Self::new(MeshGeometry {
            vertex_ids: mesh.vertex_ids.clone(),
            positions: shown
                .vertices
                .iter()
                .map(|(_, p)| Vec2::new(p.x, p.y))
                .collect(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            meshing: None,
        })
    }

    pub(super) fn uv(&self, point: Vec2) -> Result<Vec2> {
        Ok(self.mapping.uv(point)?)
    }
    fn position(&self, uv: Vec2) -> Result<Vec2> {
        Ok(self.mapping.position(uv)?)
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
            .mesh(mesh_id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        let local = zipzi_core::evaluation::authoring_mesh_positions(
            &*model,
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
        })?;
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
        let selection: Vec<_> = starts
            .iter()
            .map(|v| (self.mesh_id.clone(), v.id))
            .collect();
        self.move_targets_with_selection(starts, delta, &selection)
    }

    pub(super) fn move_targets_with_selection(
        &self,
        starts: &[ManualVertexTarget],
        delta: ModelVector,
        selection: &[(String, VertexId)],
    ) -> Result<Vec<ManualVertexTarget>> {
        if delta.x == 0. && delta.y == 0. {
            return Ok(starts.to_vec());
        }
        let requested: Vec<_> = selection.iter().map(|v| (v.clone(), delta)).collect();
        let solved = self.glue.solve(&requested)?;
        let slots: HashMap<_, _> = selection
            .iter()
            .enumerate()
            .map(|(index, (mesh, id))| ((mesh.as_str(), *id), index))
            .collect();
        starts
            .iter()
            .map(|v| {
                let index = slots[&(self.mesh_id.as_str(), v.id)];
                let delta = solved[index];
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
