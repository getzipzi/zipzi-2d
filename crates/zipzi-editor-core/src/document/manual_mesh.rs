//! A manual gesture owns one Runtime candidate and one frozen source image.
use super::*;
use zipzi_sdk::{AffineUvMap, ManualMeshOperation, MeshGeometry, extract_manual_meshing};

#[derive(Clone, Debug)]
pub struct ManualMeshTarget {
    pub object: crate::ObjectRef,
    pub mapping: AffineUvMap,
    pub texture: SceneTexture,
    pub glue_vertices: Vec<zipzi_core::VertexId>,
    pub source: zipzi_runtime::SourceRef,
}

impl Document {
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
            .get_mesh(&object.id)
            .ok_or_else(|| anyhow!("Mesh no longer exists"))?;
        ensure!(
            !self.session.mesh_ids().iter().any(|id| id != &object.id
                && model
                    .get_mesh(id)
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
                .filter_map(|id| model.get_glue(id))
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
            source: zipzi_runtime::SourceRef::Formal {
                session: self.session.id,
                version: self.session.version(),
            },
        })
    }

    /// Mode ownership constrains transient navigation, never document history.
    pub(crate) fn retain_manual_mesh_context(&mut self, object: &crate::ObjectRef) {
        let exists = self.model().get_mesh(&object.id).is_some();
        self.context.selections = if exists { vec![object.clone()] } else { vec![] };
        self.context.selection = exists.then(|| object.clone());
        self.context.inspected = self.context.selection.clone();
        self.context.active_binding = None;
        self.context.parameter_focus = None;
        self.selected = exists.then(|| object.id.clone());
    }

    pub(crate) fn validate_manual_target(&self, target: &ManualMeshTarget) -> Result<()> {
        self.ensure_editable(std::slice::from_ref(&target.object))?;
        ensure!(
            self.context().selections.as_slice() == [target.object.clone()],
            "Mesh Edit target changed"
        );
        ensure!(
            self.session.version() == target.source.version(),
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
                    .begin_owned_edit("Mesh Edit", Some(target.source.version()))?
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
        let report = match result {
            Ok(report) => report,
            Err(error) => {
                if self.manual_pending.as_ref().is_some_and(|p| !p.changed) {
                    self.manual_pending.take();
                }
                return Err(error.into());
            }
        };
        let pending = self.manual_pending.as_mut().unwrap();
        pending.changed = true;
        self.invalidate_projection(false);
        if report.reconnected || report.added > 0 || report.removed > 0 {
            self.invalidate_canvas_topology(Some(std::collections::HashSet::from([target
                .object
                .id
                .clone()])));
        }
        Ok(true)
    }

    pub(crate) fn has_manual_mesh_gesture(&self) -> bool {
        self.manual_pending.is_some()
    }

    pub fn finish_manual_mesh_gesture(&mut self) -> Result<()> {
        let Some(pending) = self.manual_pending.take() else {
            return Ok(());
        };
        if pending.changed {
            let receipt = pending.edit.commit_to(&mut self.session)?;
            if receipt.changed {
                self.pristine_blank = None;
            }
            self.consume_authoring_receipt(&receipt, false);
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
