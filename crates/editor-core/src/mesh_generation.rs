//! Immutable generation previews and atomic, undoable topology replacement.
#[path = "mesh_generation_alpha.rs"]
mod alpha;
use super::Document;
use crate::{ObjectKind, ObjectRef};
use alpha::TextureTransform;
use anyhow::{Result, ensure};
pub use kasane_sdk::AlphaMeshOptions;
use kasane_sdk::{
    AlphaMask, MeshGeometry, TopologyReplacement, alpha_mesh_geometry, rectangle_grid_geometry,
};

/// Each backend owns its typed configuration. Preview and apply share this dispatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeshGenerationOptions {
    Grid(MeshGridOptions),
    Alpha(AlphaMeshOptions),
}

/// Immutable input for background generation; no document or history is mutated.
#[derive(Clone, Debug)]
pub struct MeshGenerationSource {
    target: ObjectRef,
    revision: u64,
    geometry: MeshGeometry,
    square: bool,
    alpha_transform: Option<TextureTransform>,
    alpha_error: Option<String>,
    grid_available: bool,
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
    padded_texture: Option<crate::SceneTexture>,
}

impl MeshGenerationSource {
    pub fn alpha_error(&self) -> Option<&str> {
        self.alpha_error.as_deref()
    }
    pub fn grid_available(&self) -> bool {
        self.grid_available
    }

    pub fn generate(&self, options: MeshGenerationOptions) -> Result<MeshGenerationPreview> {
        let mut padded_texture = None;
        let geometry = match options {
            MeshGenerationOptions::Alpha(options) => {
                ensure!(
                    self.alpha_error.is_none(),
                    "{}",
                    self.alpha_error
                        .as_deref()
                        .unwrap_or("Alpha generation unavailable")
                );
                let texture = self
                    .texture
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing source texture"))?;
                let alpha: Vec<_> = texture
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|p| p[3])
                    .collect();
                let mut geometry = alpha_mesh_geometry(
                    AlphaMask {
                        width: texture.width,
                        height: texture.height,
                        alpha: &alpha,
                    },
                    &options,
                )?;
                self.alpha_transform.unwrap().map(&mut geometry);
                padded_texture = alpha::pad_texture(texture, &mut geometry)?;
                geometry
            }
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
        let mut source_bounds = match options {
            MeshGenerationOptions::Alpha(_) => self.alpha_transform.unwrap().bounds(),
            MeshGenerationOptions::Grid(_) => {
                [self.geometry.positions[0], self.geometry.positions[2]]
            }
        };
        for p in &geometry.positions {
            source_bounds[0].x = source_bounds[0].x.min(p.x);
            source_bounds[0].y = source_bounds[0].y.min(p.y);
            source_bounds[1].x = source_bounds[1].x.max(p.x);
            source_bounds[1].y = source_bounds[1].y.max(p.y);
        }
        Ok(MeshGenerationPreview {
            target: self.target.clone(),
            revision: self.revision,
            options,
            geometry,
            minimum_angle,
            source_bounds,
            padded_texture,
        })
    }
}

impl MeshGenerationPreview {
    pub fn render_texture(&self, source: crate::SceneTexture) -> crate::SceneTexture {
        self.padded_texture.clone().unwrap_or(source)
    }
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
        self.validate_mesh_generation_target(object)?;
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
            alpha_transform: self.alpha_transform(object).ok(),
            alpha_error: self
                .validate_mesh_alpha(object)
                .err()
                .map(|e| e.to_string()),
            grid_available: self
                .validate_mesh_grid(
                    object,
                    MeshGridOptions {
                        columns: 1,
                        rows: 1,
                    },
                )
                .is_ok(),
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
            MeshGenerationOptions::Alpha(_) => {
                ensure!(
                    self.mesh_generation_revision() == preview.revision && !self.has_pending_edit(),
                    "Document has changed"
                );
                self.validate_mesh_alpha(&preview.target)?;
                let source = self
                    .session
                    .geometry(&preview.target.id)
                    .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
                let mut mesh = self.session.mesh(&preview.target.id).unwrap();
                let asset = if let Some(texture) = &preview.padded_texture {
                    let previous = self.session.asset(&mesh.texture_asset_id).unwrap();
                    Some(self.session.prepare_rgba_asset(
                        &previous.id,
                        &previous.name,
                        texture.width,
                        texture.height,
                        texture.rgba.clone(),
                    )?)
                } else {
                    None
                };
                let g = &preview.geometry;
                mesh.vertex_ids = g.vertex_ids.clone();
                mesh.base_positions = g.positions.clone();
                mesh.uvs = g.uvs.clone();
                mesh.triangles = g.triangles.clone();
                let replacement = TopologyReplacement {
                    mesh,
                    binding: None,
                    blend_bindings: vec![],
                    glues: vec![],
                    vertex_mapping: source.vertex_ids.iter().map(|&id| (id, None)).collect(),
                };
                let (_, receipt) =
                    self.session
                        .edit("generate alpha mesh", Some(source.version), |edit| {
                            if let Some(asset) = asset {
                                edit.replace_asset(asset)?;
                            }
                            edit.replace_topology(&source, replacement)
                        })?;
                if receipt.changed {
                    self.refresh_textures()?;
                    self.pristine_blank = false;
                    self.after_history(true);
                }
                Ok(())
            }
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
        ensure!(selection.len() == 1, "Select one mesh");
        let object = &selection[0];
        self.validate_mesh_generation_target(object)?;
        Ok(object.clone())
    }

    fn validate_mesh_generation_target(&self, object: &ObjectRef) -> Result<()> {
        self.validate_mesh_grid(
            object,
            MeshGridOptions {
                columns: 1,
                rows: 1,
            },
        )
        .or_else(|_| self.validate_mesh_alpha(object))
    }

    fn alpha_transform(&self, object: &ObjectRef) -> Result<TextureTransform> {
        let mesh = self
            .mesh(&object.id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        TextureTransform::from_geometry(&MeshGeometry {
            vertex_ids: mesh.vertex_ids,
            positions: mesh.base_positions,
            uvs: mesh.uvs,
            triangles: mesh.triangles,
        })
    }

    pub fn validate_mesh_alpha(&self, object: &ObjectRef) -> Result<()> {
        ensure!(object.kind == ObjectKind::Mesh, "Select one mesh");
        self.ensure_editable(std::slice::from_ref(object))?;
        ensure!(
            !self.mesh_grid_requires_square(object)
                && !self.session.glue_ids().iter().any(|id| self
                    .session
                    .glue(id)
                    .is_some_and(|g| g.mesh_a_id == object.id || g.mesh_b_id == object.id)),
            "Alpha generation is unavailable for meshes with keyforms, BlendShapes or Glue"
        );
        let mesh = self
            .mesh(&object.id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        ensure!(
            self.texture_slots.contains_key(&mesh.texture_asset_id),
            "Missing source texture"
        );
        ensure!(
            !self.session.mesh_ids().iter().any(|id| id != &object.id
                && self
                    .session
                    .mesh(id)
                    .is_some_and(|m| m.texture_asset_id == mesh.texture_asset_id)),
            "Alpha generation requires a dedicated source texture, not a shared atlas"
        );
        self.alpha_transform(object)?;
        Ok(())
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
