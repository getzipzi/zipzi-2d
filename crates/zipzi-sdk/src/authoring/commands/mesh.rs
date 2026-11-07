use super::*;
use crate::manual_mesh_transfer::{ManualMeshSource, MeshTransfer};
use crate::{ManualMeshOperation, ManualMeshOutcome, MeshCreateSpec, MeshGeometry};
impl Commands<'_> {
    fn install_mesh_transfer(
        &mut self,
        changes: MeshTransfer,
    ) -> Result<ManualMeshOutcome, Status> {
        self.builder.replace_mesh_topology(
            changes.mesh,
            changes.binding,
            changes.blend_bindings,
            changes.glues,
            changes.vertex_mapping,
        )?;
        Ok(ManualMeshOutcome {
            report: changes.report,
            provenance: changes.provenance,
            affected_edges: changes.affected_edges,
        })
    }
    pub fn edit_manual_mesh(
        &mut self,
        id: &str,
        operation: ManualMeshOperation,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshOutcome, Status> {
        self.run(|this| {
            let state = this.builder.preview()?;
            let source = ManualMeshSource::new(&state, id, limits)?;
            let changes = source.plan(operation, limits, control)?;
            this.install_mesh_transfer(changes)
        })
    }
    pub fn remesh(
        &mut self,
        id: &str,
        geometry: MeshGeometry,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshOutcome, Status> {
        self.run(|this| {
            let state = this.builder.preview()?;
            let source = ManualMeshSource::new(&state, id, limits)?;
            let changes = source.plan_geometry(geometry, limits, control)?;
            this.install_mesh_transfer(changes)
        })
    }
    pub fn remesh_with_texture_uvs(
        &mut self,
        id: &str,
        geometry: MeshGeometry,
        texture_uvs: Vec<Vec2>,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshOutcome, Status> {
        self.run(|this| {
            let state = this.builder.preview()?;
            let source = ManualMeshSource::new(&state, id, limits)?;
            let changes =
                source.plan_geometry_with_texture_uvs(geometry, texture_uvs, limits, control)?;
            this.install_mesh_transfer(changes)
        })
    }
    pub fn create_mesh_spec(&mut self, spec: MeshCreateSpec) -> Result<String, Status> {
        let id = spec.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.create_mesh(Mesh {
            id: id.clone(),
            runtime_id: id.clone(),
            name: spec.name,
            texture_asset_id: spec.texture_asset_id.into(),
            vertex_ids: spec.geometry.vertex_ids,
            base_positions: spec.geometry.positions,
            uvs: spec.geometry.uvs,
            triangles: spec.geometry.triangles,
            meshing: spec.geometry.meshing,
            ..Default::default()
        })?;
        Ok(id)
    }
    pub fn remesh_rectangle_grid(
        &mut self,
        id: &str,
        columns: usize,
        rows: usize,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ManualMeshOutcome, Status> {
        self.run(|this| {
            let state = this.builder.preview()?;
            let source = ManualMeshSource::new(&state, id, limits)?;
            let geometry = crate::rectangle_grid_geometry(source.geometry(), columns, rows)?;
            let changes = source.plan_geometry(geometry, limits, control)?;
            this.install_mesh_transfer(changes)
        })
    }
}
