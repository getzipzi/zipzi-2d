use super::*;
impl Model<'_> {
    pub(crate) fn validate_mesh_snapshot(&self, mesh: &Mesh, geometry_validated: bool) -> Status {
        if !mesh.texture_asset_id.validate().is_ok() {
            return mesh.texture_asset_id.validate();
        }
        if mesh
            .texture_asset_id
            .resolved_id()
            .is_some_and(|id| !self.content.assets.contains_key(id))
        {
            return Status::error("MISSING_ASSET", "Texture asset does not exist.");
        }
        let status = self.validate_mesh_properties(mesh);
        if !status.is_ok() {
            return status;
        }
        if mesh.runtime_id.is_empty()
            || self
                .content
                .meshes
                .values()
                .any(|other| other.id != mesh.id && other.runtime_id == mesh.runtime_id)
        {
            return Status::error("DUPLICATE_RUNTIME_ID", &mesh.id);
        }
        if geometry_validated {
            return Status::ok();
        }
        #[cfg(feature = "bench-instrumentation")]
        crate::benchmark::count("core.validate_structure.full.mesh_geometry_checks", 1);
        if mesh.vertex_ids.len() != mesh.base_positions.len() {
            return Status::error("INVALID_LENGTH", "Vertex IDs must match positions.");
        }
        let slots: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        if slots.len() != mesh.vertex_ids.len() {
            return Status::error(
                "DUPLICATE_VERTEX",
                "Vertex IDs must be unique within a mesh.",
            );
        }
        let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
        for triangle in &mesh.triangles {
            for id in triangle {
                let Some(slot) = slots.get(id) else {
                    return Status::error(
                        "MISSING_VERTEX",
                        "Triangle references an unknown vertex ID.",
                    );
                };
                indices.push(*slot as u32);
            }
        }
        let status = validate_render_mesh(&mesh.base_positions, &mesh.uvs, &indices);
        if !status.is_ok() {
            return status;
        }
        match &mesh.meshing {
            Some(meshing) => crate::geometry::validate_mesh_meshing(
                &mesh.vertex_ids,
                &mesh.uvs,
                &mesh.triangles,
                meshing,
            ),
            None => Status::ok(),
        }
    }
}
