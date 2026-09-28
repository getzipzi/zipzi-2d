use crate::{SceneSnapshot, SceneTexture};
use anyhow::{Result, anyhow, ensure};
use kasane_core::{Canvas, DrawableFrame, Mesh, PreviewValues, Vec2, evaluation::evaluate_frame};
use kasane_project::read_project_asset;
use kasane_sdk::{AuthoringSession, EditSession, MeshProperties, SaveReceipt};
use std::{path::Path, sync::Arc};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub opacity: f32,
    pub visible: bool,
    pub vertices: usize,
    pub triangles: usize,
}

impl From<Mesh> for Layer {
    fn from(mesh: Mesh) -> Self {
        Self {
            id: mesh.id,
            name: mesh.name,
            opacity: mesh.appearance.opacity,
            visible: mesh.enabled,
            vertices: mesh.vertex_ids.len(),
            triangles: mesh.triangles.len(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gesture {
    Rename,
    Opacity,
    Vertices,
}

struct PendingEdit {
    kind: Gesture,
    original: Mesh,
    edit: EditSession<'static>,
}

/// The SDK owns all persistent model state and history. Selection is editor state.
/// A candidate edit lets a gesture preview without adding a history entry per event.
pub struct Document {
    session: AuthoringSession,
    import_name: Option<String>,
    selected: Option<String>,
    pending: Option<PendingEdit>,
    textures: Arc<[SceneTexture]>,
    pristine_blank: bool,
}

impl Document {
    pub fn new_blank() -> Result<Self> {
        let mut document = Self::from_session(Self::empty_session()?)?;
        document.pristine_blank = true;
        Ok(document)
    }

    fn empty_session() -> Result<AuthoringSession> {
        Ok(AuthoringSession::new(
            &Uuid::new_v4().to_string(),
            Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
        )?)
    }

    /// Construct separately so a malformed project/resource never replaces the live document.
    pub fn open(path: &Path) -> Result<Self> {
        let mut session = Self::empty_session()?;
        session.open_project(path, None)?;
        Self::from_session(session)
    }

    pub fn import_model3(path: &Path) -> Result<(Self, Vec<String>)> {
        let mut session = Self::empty_session()?;
        let receipt = session.import_model3(path, None)?;
        let mut notices = receipt.project.warnings;
        notices.extend(
            receipt
                .project
                .diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message)),
        );
        let mut document = Self::from_session(session)?;
        document.import_name = model_name_from_path(path);
        Ok((document, notices))
    }

    pub fn import_psd(source: &Path, destination: &Path) -> Result<(Self, Vec<String>)> {
        let mut session = Self::empty_session()?;
        let receipt = session.import_psd(source, destination, None)?;
        let mut notices = receipt.project.warnings;
        notices.extend(
            receipt
                .project
                .diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message)),
        );
        Ok((Self::from_session(session)?, notices))
    }

    fn from_session(session: AuthoringSession) -> Result<Self> {
        let textures = Self::load_textures(&session)?;
        let selected = session.mesh_ids().first().cloned();
        Ok(Self {
            session,
            import_name: None,
            selected,
            pending: None,
            textures,
            pristine_blank: false,
        })
    }

    fn load_textures(session: &AuthoringSession) -> Result<Arc<[SceneTexture]>> {
        let root = session
            .project_path()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new(""));
        session
            .asset_ids()
            .iter()
            .map(|id| {
                let asset = session
                    .asset(id)
                    .ok_or_else(|| anyhow!("Texture asset not found: {id}"))?;
                let data = read_project_asset(root, &asset)
                    .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
                Ok(SceneTexture {
                    asset_id: id.clone(),
                    sha256: data.sha256,
                    width: data.width,
                    height: data.height,
                    rgba: data.rgba.into(),
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(Into::into)
    }

    pub fn save(&mut self, path: &Path) -> Result<SaveReceipt> {
        self.finish_edit()?;
        let receipt = self.session.save_project(path, None)?;
        self.pristine_blank = false;
        Ok(receipt)
    }

    pub fn canvas(&self) -> Canvas {
        self.session.canvas()
    }

    pub fn path(&self) -> Option<&Path> {
        self.session.project_path()
    }
    pub fn import_name(&self) -> Option<&str> {
        self.import_name.as_deref()
    }
    pub fn modified(&self) -> bool {
        (self.session.modified() && !self.pristine_blank) || self.pending_changed()
    }
    pub fn history_lengths(&self) -> (usize, usize) {
        self.session.history_lengths()
    }
    pub fn has_pending_edit(&self) -> bool {
        self.pending.is_some()
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    fn mesh(&self, id: &str) -> Option<Mesh> {
        match &self.pending {
            Some(pending) => pending.edit.candidate_document().get_mesh(id).cloned(),
            None => self.session.mesh(id),
        }
    }

    pub fn layers(&self) -> Vec<Layer> {
        self.session
            .mesh_ids()
            .iter()
            .filter_map(|id| self.mesh(id).map(Into::into))
            .collect()
    }

    pub fn selected_layer(&self) -> Option<Layer> {
        self.selected
            .as_deref()
            .and_then(|id| self.mesh(id))
            .map(Into::into)
    }

    pub fn select(&mut self, id: &str) -> Result<()> {
        ensure!(self.session.mesh(id).is_some(), "Layer not found");
        self.finish_edit()?;
        self.selected = Some(id.into());
        Ok(())
    }

    fn begin_gesture(&mut self, kind: Gesture) -> Result<()> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.kind == kind)
        {
            return Ok(());
        }
        self.finish_edit()?;
        let original = self
            .selected
            .as_deref()
            .and_then(|id| self.session.mesh(id))
            .ok_or_else(|| anyhow!("Select a layer first"))?;
        let label = match kind {
            Gesture::Rename => "Rename layer",
            Gesture::Opacity => "Change opacity",
            Gesture::Vertices => "Move vertices",
        };
        self.pending = Some(PendingEdit {
            kind,
            original,
            edit: self.session.begin_owned_edit(label, None)?,
        });
        Ok(())
    }

    pub fn selected_vertex_ids(&self) -> Vec<kasane_core::VertexId> {
        self.selected
            .as_deref()
            .and_then(|id| self.mesh(id))
            .map(|m| m.vertex_ids)
            .unwrap_or_default()
    }
    pub fn selected_vertex_positions(&self) -> Vec<(kasane_core::VertexId, crate::ModelPoint)> {
        self.selected
            .as_deref()
            .and_then(|id| self.mesh(id))
            .map(|m| {
                m.vertex_ids
                    .into_iter()
                    .zip(
                        m.base_positions
                            .into_iter()
                            .map(|p| crate::ModelPoint { x: p.x, y: p.y }),
                    )
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn vertex_unavailable(&self, mesh: &Mesh) -> Option<String> {
        if self.session.binding_for_mesh(&mesh.id).is_some() {
            return Some("This mesh has parameter keyforms".into());
        }
        if !mesh.deformer_id.is_empty() {
            return Some("This mesh depends on a deformer".into());
        }
        if self
            .session
            .blend_binding_ids()
            .iter()
            .filter_map(|id| self.session.blend_binding(id))
            .any(|b| b.target_id == mesh.id)
        {
            return Some("This mesh has BlendShape dependencies".into());
        }
        if self
            .session
            .glue_ids()
            .iter()
            .filter_map(|id| self.session.glue(id))
            .any(|g| g.mesh_a_id == mesh.id || g.mesh_b_id == mesh.id)
        {
            return Some("This mesh has Glue dependencies".into());
        }
        None
    }
    pub fn begin_vertices(&mut self) -> Result<()> {
        let mesh = self
            .selected
            .as_deref()
            .and_then(|id| self.mesh(id))
            .ok_or_else(|| anyhow!("Select a mesh"))?;
        ensure!(
            self.vertex_unavailable(&mesh).is_none(),
            "Mesh is not available for base vertex editing"
        );
        self.begin_gesture(Gesture::Vertices)
    }
    pub fn preview_vertices(
        &mut self,
        ids: &[kasane_core::VertexId],
        positions: &[crate::ModelPoint],
    ) -> Result<()> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| anyhow!("No vertex gesture"))?;
        ensure!(pending.kind == Gesture::Vertices, "Another edit is active");
        let positions = positions
            .iter()
            .map(|p| Vec2::new(p.x, p.y))
            .collect::<Vec<_>>();
        Ok(pending
            .edit
            .update_positions(&pending.original.id, ids, &positions)?)
    }
    pub fn geometry(&self, scene: &SceneSnapshot) -> std::sync::Arc<[crate::MeshGeometry]> {
        scene
            .frame
            .drawables
            .iter()
            .filter_map(|drawable| {
                let mesh = self.mesh(&drawable.id)?;
                Some(crate::MeshGeometry {
                    id: mesh.id.clone(),
                    unavailable: self.vertex_unavailable(&mesh),
                    vertices: mesh
                        .vertex_ids
                        .iter()
                        .copied()
                        .zip(drawable.positions.iter().map(|p| {
                            crate::ViewportTransform::evaluated_to_source(scene.frame.canvas, *p)
                        }))
                        .collect(),
                    triangles: mesh.triangles,
                })
            })
            .collect::<Vec<_>>()
            .into()
    }

    pub fn preview_name(&mut self, name: &str) -> Result<()> {
        self.begin_gesture(Gesture::Rename)?;
        let pending = self.pending.as_mut().unwrap();
        let result = pending.edit.rename_mesh(&pending.original.id, name);
        if result.is_err() {
            self.pending = None;
        }
        Ok(result?)
    }

    pub fn preview_opacity(&mut self, opacity: f32) -> Result<()> {
        ensure!(
            opacity.is_finite() && (0.0..=1.0).contains(&opacity),
            "Opacity must be between 0 and 1"
        );
        self.begin_gesture(Gesture::Opacity)?;
        let pending = self.pending.as_mut().unwrap();
        let mut properties = MeshProperties::from(&pending.original);
        properties.appearance.opacity = opacity;
        let result = pending
            .edit
            .update_mesh_properties(&pending.original.id, properties);
        if result.is_err() {
            self.pending = None;
        }
        Ok(result?)
    }

    fn pending_changed(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            pending
                .edit
                .candidate_document()
                .get_mesh(&pending.original.id)
                != Some(&pending.original)
        })
    }

    pub fn finish_edit(&mut self) -> Result<()> {
        let changed = self.pending_changed();
        if let Some(pending) = self.pending.take() {
            if changed {
                pending.edit.commit_to(&mut self.session)?;
                self.session.drain_events();
                self.pristine_blank = false;
            }
        }
        Ok(())
    }

    pub fn cancel_edit(&mut self) {
        self.pending = None;
    }

    pub fn set_visible(&mut self, visible: bool) -> Result<()> {
        self.finish_edit()?;
        let mesh = self
            .selected
            .as_deref()
            .and_then(|id| self.session.mesh(id))
            .ok_or_else(|| anyhow!("Select a layer first"))?;
        if mesh.enabled == visible {
            return Ok(());
        }
        let mut properties = MeshProperties::from(&mesh);
        properties.enabled = visible;
        self.session.edit("Change layer visibility", None, |edit| {
            edit.update_mesh_properties(&mesh.id, properties)
        })?;
        self.session.drain_events();
        self.pristine_blank = false;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<()> {
        self.finish_edit()?;
        self.session.undo()?;
        self.after_history();
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        self.finish_edit()?;
        self.session.redo()?;
        self.after_history();
        Ok(())
    }

    fn after_history(&mut self) {
        self.session.drain_events();
        if self
            .selected
            .as_deref()
            .is_none_or(|id| self.session.mesh(id).is_none())
        {
            self.selected = self.session.mesh_ids().first().cloned();
        }
    }

    pub fn snapshot(&mut self) -> Result<SceneSnapshot> {
        let frame = if let Some(pending) = &self.pending {
            let mut frame = DrawableFrame::default();
            let status = evaluate_frame(
                pending.edit.candidate_document(),
                &PreviewValues::default(),
                &mut frame,
            );
            ensure!(status.is_ok(), "{}: {}", status.code, status.message);
            Arc::new(frame)
        } else {
            self.session.preview_frame()?
        };
        Ok(SceneSnapshot {
            frame,
            textures: self.textures.clone(),
        })
    }
}

fn model_name_from_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy();
    let file_name_lower = file_name.to_ascii_lowercase();
    let name = if file_name_lower.ends_with(".model3.json") {
        &file_name[..file_name.len() - ".model3.json".len()]
    } else {
        path.file_stem()?.to_str()?
    };
    let name = name.trim();
    if name.is_empty() || name.eq_ignore_ascii_case("model") {
        let parent = path.parent()?.file_name()?.to_string_lossy();
        let parent = parent.trim();
        (!parent.is_empty()).then(|| parent.to_owned())
    } else {
        Some(name.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::model_name_from_path;
    use std::path::Path;

    #[test]
    fn derives_model_name_from_model3_path() {
        assert_eq!(
            model_name_from_path(Path::new("/models/Hiyori/Hiyori.model3.json")),
            Some("Hiyori".into())
        );
        assert_eq!(
            model_name_from_path(Path::new("/models/Hiyori/model.model3.json")),
            Some("Hiyori".into())
        );
    }
}
