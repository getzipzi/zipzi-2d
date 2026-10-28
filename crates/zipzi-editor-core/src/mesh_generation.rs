//! Immutable generation previews and atomic, undoable topology replacement.
#[path = "mesh_generation_alpha.rs"]
mod alpha;
use super::Document;
pub use crate::adaptive_mesh::MeshSettings;
use crate::adaptive_mesh::{MeshGenerationStage, generate_mesh};
use crate::{ObjectKind, ObjectRef};
use alpha::TextureTransform;
use anyhow::{Context, Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use zipzi_sdk::{
    ManualMeshOperation, ManualVertexIdAllocator, MeshGeometry, TopologyReplacement,
    add_manual_interior_sites, edit_manual_mesh, rectangle_grid_geometry,
};

/// Each backend owns its typed configuration. Preview and apply share this dispatch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeshGenerationOptions {
    Grid(MeshGridOptions),
    Alpha(MeshSettings),
    ManualEdge(MeshSettings),
    ManualInterior(MeshSettings),
    ManualFull(MeshSettings),
    ManualReconnect,
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
    manual_target: Option<crate::ManualMeshTarget>,
    generation: Arc<AtomicU64>,
}

#[derive(Clone, Debug)]
pub struct MeshGenerationPreview {
    target: ObjectRef,
    revision: u64,
    options: MeshGenerationOptions,
    geometry: MeshGeometry,
    minimum_angle: f64,
    source_bounds: [zipzi_core::Vec2; 2],
    padded_texture: Option<crate::SceneTexture>,
    manual_target: Option<crate::ManualMeshTarget>,
    texture_identity: Option<(String, u32, u32)>,
    generation: Arc<AtomicU64>,
    generation_id: u64,
}

impl MeshGenerationSource {
    pub fn alpha_error(&self) -> Option<&str> {
        self.alpha_error.as_deref()
    }
    pub fn grid_available(&self) -> bool {
        self.grid_available
    }
    pub fn manual_available(&self) -> bool {
        self.manual_target.is_some()
    }
    pub fn replaces_modeling_topology(&self) -> bool {
        self.geometry.meshing.is_some()
    }
    pub fn invalidate_previews(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    pub fn is_current(&self, document: &Document) -> bool {
        document.mesh_generation_revision() == self.revision
            && !document.has_pending_edit()
            && self
                .manual_target
                .as_ref()
                .is_none_or(|target| document.validate_manual_target(target).is_ok())
            && self.texture.as_ref().is_none_or(|original| {
                document
                    .mesh(&self.target.id)
                    .and_then(|mesh| document.textures.get(&mesh.texture_asset_id))
                    .is_some_and(|current| {
                        current.cache_key == original.cache_key
                            && current.width == original.width
                            && current.height == original.height
                    })
            })
    }

    pub fn generate(&self, options: MeshGenerationOptions) -> Result<MeshGenerationPreview> {
        let generation_id = self.generation.load(Ordering::SeqCst);
        let phase = options;
        let mut padded_texture = None;
        let geometry = match options {
            MeshGenerationOptions::Alpha(alpha_options)
            | MeshGenerationOptions::ManualEdge(alpha_options)
            | MeshGenerationOptions::ManualInterior(alpha_options)
            | MeshGenerationOptions::ManualFull(alpha_options) => {
                if self.manual_target.is_some() {
                    ensure!(
                        alpha_options.clip_to_image,
                        "Mesh Edit stages require sites inside the complete source image"
                    );
                }
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
                // The chosen phase is kept in the outer enum, separate from
                // alpha tuning values shared by all phases.
                let mut geometry = match phase {
                    MeshGenerationOptions::ManualEdge(_) => {
                        generate_mesh(
                            texture.width,
                            texture.height,
                            &alpha,
                            &alpha_options,
                            MeshGenerationStage::Edges,
                        )?
                        .geometry
                    }
                    MeshGenerationOptions::ManualInterior(_) => {
                        let sites = generate_mesh(
                            texture.width,
                            texture.height,
                            &alpha,
                            &alpha_options,
                            MeshGenerationStage::Full,
                        )?;
                        let additions = filter_interior_sites(
                            &self.geometry,
                            &sites.interior_uvs,
                            texture.width,
                            texture.height,
                            (alpha_options.interior_spacing * 0.3).max(0.001),
                        );
                        ensure!(
                            self.geometry.vertex_ids.len() + additions.len()
                                <= alpha_options.max_vertices,
                            "Interior sites exceed the mesh vertex budget"
                        );
                        let mut allocator = ManualVertexIdAllocator::new(&self.geometry);
                        add_manual_interior_sites(&self.geometry, &additions, &mut allocator)?
                            .geometry
                    }
                    MeshGenerationOptions::ManualFull(_) | MeshGenerationOptions::Alpha(_) => {
                        generate_mesh(
                            texture.width,
                            texture.height,
                            &alpha,
                            &alpha_options,
                            MeshGenerationStage::Full,
                        )?
                        .geometry
                    }
                    _ => unreachable!(),
                };
                if !matches!(phase, MeshGenerationOptions::ManualInterior(_)) {
                    self.alpha_transform.unwrap().map(&mut geometry);
                    padded_texture = alpha::pad_texture(texture, &mut geometry)?;
                }
                if self.manual_target.is_some() {
                    zipzi_sdk::extract_manual_meshing(&geometry)
                        .context("Generated mesh is not editable")?;
                }
                geometry
            }
            MeshGenerationOptions::ManualReconnect => {
                edit_manual_mesh(&self.geometry, ManualMeshOperation::Reconnect)?.geometry
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
            MeshGenerationOptions::Alpha(_)
            | MeshGenerationOptions::ManualEdge(_)
            | MeshGenerationOptions::ManualInterior(_)
            | MeshGenerationOptions::ManualFull(_)
            | MeshGenerationOptions::ManualReconnect => self.alpha_transform.unwrap().bounds(),
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
            manual_target: self.manual_target.clone(),
            texture_identity: self
                .texture
                .as_ref()
                .map(|texture| (texture.cache_key.clone(), texture.width, texture.height)),
            generation: self.generation.clone(),
            generation_id,
        })
    }
}

fn filter_interior_sites(
    geometry: &MeshGeometry,
    sites: &[zipzi_core::Vec2],
    width: u32,
    height: u32,
    clearance: f64,
) -> Vec<zipzi_core::Vec2> {
    use std::collections::HashMap;
    // Refinement can place a point arbitrarily close to an existing structural
    // segment. Preserve a pixel-space clearance, not just vertex separation,
    // before the SDK retriangulates at normalized f32 UV precision.
    let slots: HashMap<_, _> = geometry
        .vertex_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let mut segments = Vec::new();
    let meshing = zipzi_sdk::extract_manual_meshing(geometry).ok();
    if let Some(m) = &meshing {
        for i in 0..m.boundary.len() {
            segments.push([m.boundary[i], m.boundary[(i + 1) % m.boundary.len()]]);
        }
        segments.extend(m.constraints.iter().map(|c| c.endpoints));
    }
    let near_segment = |uv: zipzi_core::Vec2| {
        segments.iter().any(|e| {
            let a = geometry.uvs[slots[&e[0]]];
            let b = geometry.uvs[slots[&e[1]]];
            let dx = (b.x as f64 - a.x as f64) * width as f64;
            let dy = (b.y as f64 - a.y as f64) * height as f64;
            let px = (uv.x as f64 - a.x as f64) * width as f64;
            let py = (uv.y as f64 - a.y as f64) * height as f64;
            let t = ((px * dx + py * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0., 1.);
            (px - dx * t).hypot(py - dy * t) < clearance
        })
    };
    let mut occupied: HashMap<(i64, i64), Vec<(f64, f64)>> = HashMap::new();
    let cell = |x: f64, y: f64| {
        (
            (x / clearance).floor() as i64,
            (y / clearance).floor() as i64,
        )
    };
    let mut insert = |uv: zipzi_core::Vec2, test: bool| {
        let x = uv.x as f64 * width as f64;
        let y = uv.y as f64 * height as f64;
        let (cx, cy) = cell(x, y);
        if test
            && (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    occupied.get(&(cx + dx, cy + dy)).is_some_and(|points| {
                        points
                            .iter()
                            .any(|&(px, py)| (px - x).hypot(py - y) < clearance)
                    })
                })
            })
        {
            return false;
        }
        occupied.entry((cx, cy)).or_default().push((x, y));
        true
    };
    for &uv in &geometry.uvs {
        insert(uv, false);
    }
    sites
        .iter()
        .copied()
        .filter(|&uv| !near_segment(uv) && insert(uv, true))
        .collect()
}

impl MeshGenerationPreview {
    pub fn is_manual(&self) -> bool {
        self.manual_target.is_some()
    }
    pub fn render_texture(&self, source: crate::SceneTexture) -> crate::SceneTexture {
        self.padded_texture.clone().unwrap_or(source)
    }
    pub fn source_bounds(&self) -> [zipzi_core::Vec2; 2] {
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
    pub fn mesh_generation_source_manual(
        &self,
        target: &crate::ManualMeshTarget,
    ) -> Result<MeshGenerationSource> {
        self.validate_manual_target(target)?;
        ensure!(!self.has_pending_edit(), "Finish the current edit first");
        let mesh = self
            .mesh(&target.object.id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        let geometry = MeshGeometry {
            vertex_ids: mesh.vertex_ids,
            positions: mesh.base_positions,
            uvs: mesh.uvs,
            triangles: mesh.triangles,
            meshing: mesh.meshing,
        };
        Ok(MeshGenerationSource {
            target: target.object.clone(),
            revision: self.mesh_generation_revision(),
            geometry: geometry.clone(),
            square: false,
            alpha_transform: Some(TextureTransform::from_geometry(&geometry)?),
            alpha_error: None,
            grid_available: false,
            texture: Some(target.texture.clone()),
            manual_target: Some(target.clone()),
            generation: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn mesh_generation_source(&self, object: &ObjectRef) -> Result<MeshGenerationSource> {
        self.validate_mesh_generation_target(object)?;
        ensure!(!self.has_pending_edit(), "Document has changed");
        let mesh = self
            .mesh(&object.id)
            .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
        let texture = self.textures.get(&mesh.texture_asset_id).cloned();
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
                meshing: mesh.meshing,
            },
            manual_target: None,
            generation: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn apply_mesh_generation(&mut self, preview: &MeshGenerationPreview) -> Result<()> {
        ensure!(
            preview.generation.load(Ordering::SeqCst) == preview.generation_id,
            "Generation options changed"
        );
        if let Some(target) = &preview.manual_target {
            self.validate_manual_target(target)?;
        }
        ensure!(
            self.mesh_generation_revision() == preview.revision && !self.has_pending_edit(),
            "Document has changed"
        );
        if let Some((key, width, height)) = &preview.texture_identity {
            let mesh = self
                .mesh(&preview.target.id)
                .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
            let texture = self
                .textures
                .get(&mesh.texture_asset_id)
                .ok_or_else(|| anyhow::anyhow!("Source image was removed"))?;
            ensure!(
                texture.cache_key == *key && texture.width == *width && texture.height == *height,
                "Source image changed"
            );
        }
        match preview.options {
            MeshGenerationOptions::Alpha(_)
            | MeshGenerationOptions::ManualEdge(_)
            | MeshGenerationOptions::ManualInterior(_)
            | MeshGenerationOptions::ManualFull(_)
            | MeshGenerationOptions::ManualReconnect => {
                if preview.manual_target.is_none() {
                    self.validate_mesh_alpha(&preview.target)?;
                }
                let source = self
                    .session
                    .geometry(&preview.target.id)
                    .ok_or_else(|| anyhow::anyhow!("Mesh no longer exists"))?;
                let mut mesh = self.session.mesh(&preview.target.id).unwrap();
                let image = if let Some(texture) = &preview.padded_texture {
                    Some(self.session.prepare_rgba_image(
                        texture.width,
                        texture.height,
                        texture.rgba.clone(),
                    )?)
                } else {
                    None
                };
                let asset_id = mesh.texture_asset_id.to_string();
                let g = &preview.geometry;
                mesh.vertex_ids = g.vertex_ids.clone();
                mesh.base_positions = g.positions.clone();
                mesh.uvs = g.uvs.clone();
                mesh.triangles = g.triangles.clone();
                mesh.meshing = g.meshing.clone();
                let replacement = TopologyReplacement {
                    mesh,
                    binding: None,
                    blend_bindings: vec![],
                    glues: vec![],
                    vertex_mapping: source
                        .vertex_ids
                        .iter()
                        .map(|&id| (id, g.vertex_ids.contains(&id).then_some(id)))
                        .collect(),
                };
                let (_, receipt) =
                    self.session
                        .edit("generate alpha mesh", Some(source.version), |edit| {
                            if let Some(image) = image {
                                edit.replace_prepared_image(&asset_id, &image)?;
                            }
                            edit.replace_topology(&source, replacement)
                        })?;
                if receipt.changed {
                    self.refresh_textures()?;
                    self.pristine_blank = None;
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
            meshing: mesh.meshing,
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
            self.textures.contains(&mesh.texture_asset_id),
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
        self.session
            .binding_for_mesh(&object.id)
            .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
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
                meshing: mesh.meshing,
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
            self.pristine_blank = None;
            self.after_history(true);
        }
        Ok(())
    }
}
