//! Rectangle grid generation uses the SDK's atomic dependency migration.
use super::Document;
use crate::{ObjectKind, ObjectRef};
use anyhow::{Result, ensure};
use kasane_sdk::{MeshGeometry, rectangle_grid_geometry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshGridOptions {
    pub columns: usize,
    pub rows: usize,
}

impl Default for MeshGridOptions {
    fn default() -> Self {
        Self {
            columns: 8,
            rows: 8,
        }
    }
}

impl MeshGridOptions {
    pub fn vertex_count(self) -> Option<usize> {
        self.columns
            .checked_add(1)?
            .checked_mul(self.rows.checked_add(1)?)
    }

    pub fn validate(self) -> Result<()> {
        ensure!(
            self.columns > 0 && self.rows > 0 && self.vertex_count().is_some_and(|n| n <= 65_536),
            "Use positive row and column counts with at most 65536 vertices"
        );
        Ok(())
    }
}

impl Document {
    pub fn mesh_generation_revision(&self) -> u64 {
        self.projection_revision
    }
    /// The tool operates on one explicit mesh selection, using authored geometry.
    pub fn mesh_generation_target(&self) -> Result<ObjectRef> {
        let selection = &self.context().selections;
        ensure!(selection.len() == 1, "Select one rectangle mesh");
        let object = &selection[0];
        self.validate_mesh_grid(
            object,
            MeshGridOptions {
                columns: 1,
                rows: 1,
            },
        )?;
        Ok(object.clone())
    }

    pub fn mesh_grid_requires_square(&self, object: &ObjectRef) -> bool {
        self.session.binding_for_mesh(&object.id).is_some()
            || self.session.blend_binding_ids().iter().any(|id| {
                self.session
                    .blend_binding(id)
                    .is_some_and(|b| b.target_id == object.id)
            })
    }

    pub fn validate_mesh_grid(&self, object: &ObjectRef, options: MeshGridOptions) -> Result<()> {
        options.validate()?;
        ensure!(object.kind == ObjectKind::Mesh, "Select one rectangle mesh");
        self.ensure_editable(std::slice::from_ref(object))?;
        let mesh = self
            .mesh(&object.id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        // Let the SDK define supported corner ordering, UVs, diagonal and bounds.
        rectangle_grid_geometry(
            &MeshGeometry {
                vertex_ids: mesh.vertex_ids,
                positions: mesh.base_positions,
                uvs: mesh.uvs,
                triangles: mesh.triangles,
            },
            1,
            1,
        )?;
        ensure!(
            !self.mesh_grid_requires_square(object) || options.columns == options.rows,
            "Meshes with keyforms or BlendShapes require equal rows and columns"
        );
        Ok(())
    }

    pub fn generate_mesh_grid(
        &mut self,
        object: &ObjectRef,
        options: MeshGridOptions,
        expected_revision: u64,
    ) -> Result<()> {
        ensure!(
            self.mesh_generation_revision() == expected_revision && !self.has_pending_edit(),
            "Document has changed"
        );
        self.validate_mesh_grid(object, options)?;
        let expected = self.session.version();
        let receipt = self.session.remesh_rectangle_grid(
            &object.id,
            options.columns,
            options.rows,
            Some(expected),
        )?;
        if receipt.changed {
            self.pristine_blank = false;
            self.after_history(true);
        }
        Ok(())
    }
}
