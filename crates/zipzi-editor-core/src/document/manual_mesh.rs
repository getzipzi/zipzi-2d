//! A manual gesture owns one Runtime candidate and one frozen source image.
use super::*;
use zipzi_contract::authoring::ManualMeshOperation;
use zipzi_contract::authoring::MeshGeometry;
use zipzi_sdk::{AffineUvMap, extract_manual_meshing};

#[derive(Clone, Debug)]
pub struct ManualMeshTarget {
    pub object: crate::ObjectRef,
    pub mapping: AffineUvMap,
    pub texture: SceneTexture,
    pub glue_vertices: Vec<zipzi_core::VertexId>,
    pub source: zipzi_sdk::QuerySource,
}

impl Document {
    pub fn manual_mesh_targets(&self) -> Result<Vec<ManualMeshTarget>> {
        let selection = &self.context().selections;
        ensure!(
            !selection.is_empty() && selection.iter().all(|o| o.kind == crate::ObjectKind::Mesh),
            "Select one or more meshes"
        );
        self.ensure_editable(selection)?;
        selection
            .iter()
            .map(|object| self.manual_mesh_target_for(object))
            .collect()
    }

    pub(crate) fn manual_vertex_allocator(
        &self,
        mesh_id: &str,
        geometry: &MeshGeometry,
    ) -> std::sync::Arc<std::sync::Mutex<zipzi_sdk::ManualVertexIdAllocator>> {
        self.manual_vertex_ids
            .borrow_mut()
            .entry(mesh_id.into())
            .or_insert_with(|| {
                std::sync::Arc::new(std::sync::Mutex::new(
                    zipzi_sdk::ManualVertexIdAllocator::new(geometry),
                ))
            })
            .clone()
    }

    /// Resolve the editable target and validate its image and geometry.
    /// Imported atlas artwork is normalized by the project import pipeline.
    pub fn manual_mesh_target(&self) -> Result<ManualMeshTarget> {
        let selection = &self.context().selections;
        ensure!(selection.len() == 1, "Select one mesh");
        let object = &selection[0];
        ensure!(object.kind == crate::ObjectKind::Mesh, "Select one mesh");
        self.manual_mesh_target_for(object)
    }

    /// Re-resolve a mode's stable identity, independently of inspector navigation.
    pub(crate) fn manual_mesh_target_for(
        &self,
        object: &crate::ObjectRef,
    ) -> Result<ManualMeshTarget> {
        self.ensure_editable(std::slice::from_ref(object))?;
        ensure!(!self.has_pending_edit(), "Finish the current edit first");
        let model = self.model();
        let mesh = model
            .mesh(&object.id)
            .ok_or_else(|| anyhow!("Mesh no longer exists"))?;
        ensure!(
            !self.session.mesh_ids().iter().any(|id| id != &object.id
                && model
                    .mesh(id)
                    .is_some_and(|other| other.texture_asset_id == mesh.texture_asset_id)),
            "Mesh Edit requires an independent source image"
        );
        let texture = self
            .textures
            .get(&mesh.texture_asset_id)
            .ok_or_else(|| anyhow!("Missing complete source image"))?
            .clone();
        ensure!(
            texture.width > 0
                && texture.height > 0
                && texture.rgba.len() == texture.width as usize * texture.height as usize * 4,
            "Source image is incomplete"
        );
        let geometry = MeshGeometry {
            vertex_ids: mesh.vertex_ids.clone(),
            positions: mesh.base_positions.clone(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            meshing: mesh.meshing.clone(),
        };
        extract_manual_meshing(&geometry)?;
        ensure!(
            geometry
                .uvs
                .iter()
                .all(|uv| (0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y)),
            "UV mesh extends beyond the complete source image"
        );
        // A dedicated source-image plane. Runtime UVs have Y up.
        let mapping = AffineUvMap {
            origin: zipzi_core::Vec2::default(),
            u: zipzi_core::Vec2::new(texture.width as f32, 0.),
            v: zipzi_core::Vec2::new(0., texture.height as f32),
        };
        Ok(ManualMeshTarget {
            object: object.clone(),
            mapping,
            texture,
            glue_vertices: model
                .glue_order()
                .iter()
                .filter_map(|id| model.glue(id))
                .flat_map(|g| {
                    g.pairs.iter().flat_map(|p| {
                        [
                            (g.mesh_a_id == object.id).then_some(p.vertex_a).flatten(),
                            (g.mesh_b_id == object.id).then_some(p.vertex_b).flatten(),
                        ]
                        .into_iter()
                        .flatten()
                    })
                })
                .collect(),
            source: zipzi_sdk::QuerySource::Published(self.session.version()),
        })
    }

    /// Mode ownership constrains transient navigation, never document history.
    pub(crate) fn retain_manual_mesh_context(
        &mut self,
        objects: &[crate::ObjectRef],
        active: &crate::ObjectRef,
    ) {
        let selection = {
            let model = self.model();
            objects
                .iter()
                .filter(|o| model.mesh(&o.id).is_some())
                .cloned()
                .collect::<Vec<_>>()
        };
        self.context.selection = selection.contains(active).then(|| active.clone());
        self.context.selections = selection;
        self.context.inspected = self.context.selection.clone();
        self.context.active_binding = None;
        self.context.parameter_focus = None;
        self.selected = self
            .context
            .selection
            .as_ref()
            .map(|object| object.id.clone());
    }

    pub(crate) fn validate_manual_target(&self, target: &ManualMeshTarget) -> Result<()> {
        self.ensure_editable(std::slice::from_ref(&target.object))?;
        ensure!(
            self.context().selections.contains(&target.object),
            "Mesh Edit target changed"
        );
        ensure!(
            zipzi_sdk::QuerySource::Published(self.session.version()) == target.source,
            "Mesh Edit source version changed"
        );
        let texture = self
            .textures
            .get(&target.texture.asset_id)
            .ok_or_else(|| anyhow!("Source image was removed"))?;
        ensure!(
            texture.cache_key == target.texture.cache_key
                && texture.width == target.texture.width
                && texture.height == target.texture.height,
            "Source image changed"
        );
        Ok(())
    }

    /// Add transparent room around a dedicated image without resampling any shape.
    /// Image bytes and normalized UVs publish together as one history step.
    pub fn pad_manual_mesh_source(
        &mut self,
        target: &ManualMeshTarget,
        margin: u32,
    ) -> Result<bool> {
        self.validate_manual_target(target)?;
        ensure!(!self.has_pending_edit(), "Finish the current edit first");
        let captured = self.session.capture_authoring();
        let source = zipzi_sdk::ManualMeshInspection::capture(
            captured.captured.snapshot.document(),
            &target.object.id,
            Default::default(),
        )?;
        let mut geometry = source.geometry.clone();
        let Some(texture) =
            super::mesh_generation::pad_texture_edges(&target.texture, &mut geometry, [margin; 4])?
        else {
            return Ok(false);
        };
        let image = self.session.resource_store().prepare_rgba(
            texture.width,
            texture.height,
            texture.rgba,
        )?;
        let id = target.object.id.clone();
        let texture_id = target.texture.asset_id.clone();
        let limits = zipzi_core::state::ConversionLimits::default();
        let candidate = crate::EditPreview::convert(
            &captured,
            "Expand mesh source image",
            limits,
            &Default::default(),
            move |commands, control| {
                let outcome = commands.remesh_with_texture_uvs(
                    &id,
                    source.geometry,
                    geometry.uvs,
                    limits,
                    control,
                )?;
                commands.replace_image(&texture_id, image)?;
                Ok(outcome)
            },
        )?;
        let receipt = candidate.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.refresh_textures()?;
            self.pristine_blank = None;
            self.after_history(true);
        }
        Ok(receipt.kind != zipzi_sdk::PublicationKind::Unchanged)
    }

    pub fn preview_manual_mesh(
        &mut self,
        target: &ManualMeshTarget,
        operation: ManualMeshOperation,
    ) -> Result<bool> {
        self.validate_manual_target(target)?;
        if self.manual_pending.is_none() {
            self.finish_edit()?;
            self.manual_pending = Some(ManualPending {
                edit: self
                    .session
                    .begin_draft(
                        "Mesh Edit",
                        Some(match target.source {
                            zipzi_sdk::QuerySource::Published(version) => version,
                            zipzi_sdk::QuerySource::Draft(source) => source.base,
                        }),
                    )?
                    .interactive()?,
                preview: PreviewState::default(),
                changed: false,
            });
        }
        let result = self
            .manual_pending
            .as_mut()
            .unwrap()
            .edit
            .apply_manual_mesh(&target.object.id, operation);
        if let Err(error) = result {
            if self.manual_pending.as_ref().is_some_and(|p| !p.changed) {
                self.manual_pending.take();
            }
            return Err(error.into());
        }
        let pending = self.manual_pending.as_mut().unwrap();
        pending.changed = true;
        self.publish_manual_mesh_projection(&[target.object.clone()]);
        Ok(true)
    }

    pub(crate) fn preview_manual_mesh_batch(
        &mut self,
        operations: Vec<(ManualMeshTarget, ManualMeshOperation)>,
    ) -> Result<bool> {
        ensure!(!operations.is_empty(), "No mesh operations");
        for (target, _) in &operations {
            self.validate_manual_target(target)?;
        }
        if self.manual_pending.is_none() {
            self.finish_edit()?;
            let zipzi_sdk::QuerySource::Published(version) = operations[0].0.source else {
                anyhow::bail!("Mesh Edit requires a published source");
            };
            self.manual_pending = Some(ManualPending {
                edit: self
                    .session
                    .begin_draft("Mesh Edit", Some(version))?
                    .interactive()?,
                preview: PreviewState::default(),
                changed: false,
            });
        }
        let objects: Vec<_> = operations
            .iter()
            .map(|(target, _)| target.object.clone())
            .collect();
        let operations = operations
            .into_iter()
            .map(|(target, op)| (target.object.id, op))
            .collect();
        let result = self
            .manual_pending
            .as_mut()
            .unwrap()
            .edit
            .apply_manual_mesh_batch(operations);
        if let Err(error) = result {
            if self.manual_pending.as_ref().is_some_and(|p| !p.changed) {
                self.manual_pending.take();
            }
            return Err(error.into());
        }
        self.manual_pending.as_mut().unwrap().changed = true;
        self.publish_manual_mesh_projection(&objects);
        Ok(true)
    }

    fn publish_manual_mesh_projection(&mut self, objects: &[crate::ObjectRef]) {
        let previous_revision = self.projection_revision;
        self.invalidate_projection(false);
        let changed = {
            let model = self.model();
            let mut cache = self.object_index.lock().unwrap();
            if let Some((revision, index)) = cache.as_mut().filter(|(r, _)| *r == previous_revision)
            {
                let refreshed = objects.iter().all(|object| {
                    let mesh = model.mesh(&object.id).unwrap();
                    let unchanged = index.get(object).is_some_and(|object| {
                        object.properties["vertices"].as_u64() == Some(mesh.vertex_ids.len() as u64)
                            && object.properties["triangles"].as_u64()
                                == Some(mesh.triangles.len() as u64)
                    });
                    unchanged
                        || std::sync::Arc::make_mut(index).refresh_mesh_geometry_counts(
                            object,
                            mesh.vertex_ids.len(),
                            mesh.triangles.len(),
                        )
                });
                if refreshed {
                    *revision = self.projection_revision;
                }
            }
            let topology = self.canvas_topology.lock().unwrap();
            objects
                .iter()
                .filter(|object| {
                    let mesh = model.mesh(&object.id).unwrap();
                    topology
                        .as_ref()
                        .and_then(|t| t.meshes.get(&object.id))
                        .is_none_or(|t| !t.matches(mesh))
                })
                .map(|object| object.id.clone())
                .collect::<std::collections::HashSet<_>>()
        };
        // Compare the displayed candidate, rather than repeatedly invalidating
        // against the drag's frozen source after its first reconnection.
        if !changed.is_empty() {
            self.invalidate_canvas_topology(Some(changed));
        }
    }

    pub(crate) fn has_manual_mesh_gesture(&self) -> bool {
        self.manual_pending.is_some()
    }

    pub fn finish_manual_mesh_gesture(&mut self) -> Result<()> {
        let Some(mut pending) = self.manual_pending.take() else {
            return Ok(());
        };
        if pending.changed {
            let receipt = match pending.edit.commit_to(&mut self.session) {
                Ok(receipt) => receipt,
                Err(error) => {
                    if pending.edit.is_active() {
                        self.manual_pending = Some(pending);
                    } else {
                        self.invalidate_projection(false);
                    }
                    return Err(error.into());
                }
            };
            if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
                self.pristine_blank = None;
            }
            self.consume_authoring_receipt(&receipt, None);
        }
        self.invalidate_projection(false);
        Ok(())
    }

    pub fn cancel_manual_mesh_gesture(&mut self) -> bool {
        let Some(pending) = self.manual_pending.take() else {
            return false;
        };
        if pending.changed {
            self.invalidate_projection(true);
        }
        true
    }

    pub fn apply_manual_mesh_once(
        &mut self,
        target: &ManualMeshTarget,
        operation: ManualMeshOperation,
    ) -> Result<bool> {
        let changed = self.preview_manual_mesh(target, operation)?;
        self.finish_manual_mesh_gesture()?;
        Ok(changed)
    }
}
