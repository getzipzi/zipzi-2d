//! Rectangle grid generation uses the SDK's atomic dependency migration.
use super::Document;
use crate::{ObjectKind, ObjectRef};
use anyhow::{Result, ensure};
use kasane_sdk::{MeshGeometry, rectangle_grid_geometry};

/// Each backend owns its typed configuration. Preview and apply share this dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshGenerationOptions {
    Grid(MeshGridOptions),
}

/// Immutable input for background generation; no document or history is mutated.
#[derive(Clone, Debug)]
pub struct MeshGenerationSource {
    target: ObjectRef,
    revision: u64,
    geometry: MeshGeometry,
    square: bool,
    pub texture: Option<crate::SceneTexture>,
}

#[derive(Clone, Debug)]
pub struct MeshGenerationPreview {
    target: ObjectRef,
    revision: u64,
    options: MeshGenerationOptions,
    geometry: MeshGeometry,
    minimum_angle: f64,
    source_bounds: [kasane_core::Vec2; 2],
}

impl MeshGenerationSource {
    pub fn generate(&self, options: MeshGenerationOptions) -> Result<MeshGenerationPreview> {
        let geometry = match options {
            MeshGenerationOptions::Grid(grid) => {
                grid.validate()?;
                ensure!(
                    !self.square || grid.columns == grid.rows,
                    "Meshes with keyforms or BlendShapes require equal rows and columns"
                );
                rectangle_grid_geometry(&self.geometry, grid.columns, grid.rows)?
            }
        };
        let positions: std::collections::HashMap<_, _> = geometry
            .vertex_ids
            .iter()
            .copied()
            .zip(geometry.positions.iter())
            .collect();
        let mut minimum_angle = 180.0_f64;
        for triangle in &geometry.triangles {
            for i in 0..3 {
                let a = positions[&triangle[i]];
                let b = positions[&triangle[(i + 1) % 3]];
                let c = positions[&triangle[(i + 2) % 3]];
                let u = (b.x as f64 - a.x as f64, b.y as f64 - a.y as f64);
                let v = (c.x as f64 - a.x as f64, c.y as f64 - a.y as f64);
                let angle = (u.0 * v.1 - u.1 * v.0)
                    .abs()
                    .atan2(u.0 * v.0 + u.1 * v.1)
                    .to_degrees();
                minimum_angle = minimum_angle.min(angle);
            }
        }
        Ok(MeshGenerationPreview {
            target: self.target.clone(),
            revision: self.revision,
            options,
            geometry,
            minimum_angle,
            source_bounds: [self.geometry.positions[0], self.geometry.positions[2]],
        })
    }
}

impl MeshGenerationPreview {
    pub fn source_bounds(&self) -> [kasane_core::Vec2; 2] {
        self.source_bounds
    }
    pub fn geometry(&self) -> &MeshGeometry {
        &self.geometry
    }
    pub fn options(&self) -> MeshGenerationOptions {
        self.options
    }
    /// Geometric diagnostic only; does not measure deformation suitability.
    pub fn minimum_angle(&self) -> f64 {
        self.minimum_angle
    }
}

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
    pub fn mesh_generation_source(&self, object: &ObjectRef) -> Result<MeshGenerationSource> {
        self.validate_mesh_grid(
            object,
            MeshGridOptions {
                columns: 1,
                rows: 1,
            },
        )?;
        ensure!(!self.has_pending_edit(), "Document has changed");
        let mesh = self
            .mesh(&object.id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        let texture = self
            .texture_slots
            .get(&mesh.texture_asset_id)
            .map(|&slot| self.textures[slot].clone());
        Ok(MeshGenerationSource {
            target: object.clone(),
            revision: self.mesh_generation_revision(),
            square: self.mesh_grid_requires_square(object),
            texture,
            geometry: MeshGeometry {
                vertex_ids: mesh.vertex_ids,
                positions: mesh.base_positions,
                uvs: mesh.uvs,
                triangles: mesh.triangles,
            },
        })
    }

    pub fn apply_mesh_generation(&mut self, preview: &MeshGenerationPreview) -> Result<()> {
        match preview.options {
            // The deterministic SDK generator uses the same source and options as the
            // preview. The revision guard prevents applying to a different source.
            MeshGenerationOptions::Grid(options) => {
                self.generate_mesh_grid(&preview.target, options, preview.revision)
            }
        }
    }

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
