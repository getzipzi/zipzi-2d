use crate::{SceneSnapshot, SceneTexture, TreeNode, TreeNodeKind, tree::nest_nodes};
use anyhow::{Result, anyhow, ensure};
use kasane_core::{Canvas, Mesh, ParameterKind, Vec2, preview::PreviewState};
use kasane_project::read_project_asset;
use kasane_sdk::{AuthoringSession, EditSession, MeshProperties, SaveReceipt};
use std::collections::HashMap;
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};
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

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterValue {
    pub id: String,
    pub name: String,
    pub minimum: f32,
    pub maximum: f32,
    pub default_value: f32,
    pub value: f32,
    pub decimal_places: usize,
}

/// Presentation data derived from the bindings, rather than persisted on a parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterVisual {
    pub kind: ParameterKind,
    pub repeat: bool,
    pub keys: Vec<ParameterKey>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterKey {
    pub value: f32,
    /// At least one selected target has this key.
    pub selected: bool,
    /// Some, but not all, selected targets have this key.
    pub partial: bool,
    /// Direct binding targets; inherited deformer effects are not direct bindings.
    pub targets: Vec<String>,
    pub base: bool,
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
    preview: PreviewState,
    changed: OnceLock<bool>,
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

    fn model(&self) -> &kasane_core::Document {
        self.pending
            .as_ref()
            .map_or_else(|| self.session.document(), |p| p.edit.candidate_document())
    }

    pub fn layers(&self) -> Vec<Layer> {
        self.session
            .mesh_ids()
            .iter()
            .filter_map(|id| self.mesh(id).map(Into::into))
            .collect()
    }

    pub fn parameters(&self) -> Vec<ParameterValue> {
        self.session
            .parameter_ids()
            .iter()
            .filter_map(|id| {
                let parameter = self.session.parameter(id)?;
                Some(ParameterValue {
                    id: id.clone(),
                    name: if parameter.name.is_empty() {
                        parameter.runtime_id
                    } else {
                        parameter.name
                    },
                    minimum: parameter.minimum,
                    maximum: parameter.maximum,
                    default_value: parameter.default_value,
                    value: self
                        .session
                        .preview_values()
                        .get(id)
                        .copied()
                        .unwrap_or(parameter.default_value),
                    decimal_places: parameter.decimal_places.clamp(0, 9) as usize,
                })
            })
            .collect()
    }

    pub fn parameter_model_revision(&self) -> u64 {
        self.model().revision()
    }

    pub fn parameter_visuals(&self) -> HashMap<String, ParameterVisual> {
        self.parameter_visuals_for_targets(&self.selected.iter().cloned().collect::<Vec<_>>())
    }

    /// Aggregate keys for an explicit selection without losing target coverage.
    pub fn parameter_visuals_for_targets(
        &self,
        selected_targets: &[String],
    ) -> HashMap<String, ParameterVisual> {
        let selected_targets: std::collections::HashSet<_> = selected_targets.iter().collect();
        let model = self.model();
        let mut visuals: HashMap<_, _> = self
            .session
            .parameter_ids()
            .iter()
            .filter_map(|id| {
                let p = model.get_parameter(id)?;
                Some((
                    id.clone(),
                    ParameterVisual {
                        kind: p.kind,
                        repeat: p.repeat,
                        keys: Vec::new(),
                    },
                ))
            })
            .collect();
        let mut add_axis = |parameter_id: &str, values: &[f32], target: &str, base: Option<f32>| {
            if let Some(visual) = visuals.get_mut(parameter_id) {
                for &value in values {
                    if let Some(key) = visual.keys.iter_mut().find(|key| key.value == value) {
                        if !key.targets.iter().any(|id| id == target) {
                            key.targets.push(target.to_owned());
                        }
                        key.base |= base == Some(value);
                    } else {
                        visual.keys.push(ParameterKey {
                            value,
                            selected: false,
                            partial: false,
                            targets: vec![target.to_owned()],
                            base: base == Some(value),
                        });
                    }
                }
            }
        };
        for id in model.binding_order() {
            if let Some(binding) = model.get_binding(id) {
                for axis in &binding.axes {
                    add_axis(&axis.parameter_id, &axis.keys, &binding.mesh_id, None);
                }
            }
        }
        for id in model.scene_binding_order() {
            if let Some(binding) = model.get_scene_binding(id) {
                for axis in &binding.axes {
                    add_axis(&axis.parameter_id, &axis.keys, binding.target_id(), None);
                }
            }
        }
        for id in model.glue_order() {
            if let Some(binding) = model.get_glue(id).and_then(|glue| glue.binding.as_ref()) {
                for axis in &binding.axes {
                    add_axis(&axis.parameter_id, &axis.keys, id, None);
                }
            }
        }
        // A key table alone is not an object binding (it can exist unused).
        for id in model.blend_binding_order() {
            if let Some(binding) = model.get_blend_binding(id) {
                if let Some(table) = model.get_blend_key_table(&binding.key_table_id) {
                    add_axis(
                        &table.parameter_id,
                        &table.keys,
                        &binding.target_id,
                        table.keys.get(table.base_key_idx).copied(),
                    );
                }
            }
        }
        for visual in visuals.values_mut() {
            for key in &mut visual.keys {
                key.targets.sort();
                let count = key
                    .targets
                    .iter()
                    .filter(|id| selected_targets.contains(id))
                    .count();
                key.selected = count > 0;
                key.partial = count > 0 && count < selected_targets.len();
            }
            visual.keys.sort_by(|a, b| a.value.total_cmp(&b.value));
        }
        visuals
    }

    pub fn set_preview_parameter(&mut self, id: &str, value: f32) -> Result<()> {
        let parameter = self
            .session
            .parameter(id)
            .ok_or_else(|| anyhow!("Parameter not found: {id}"))?;
        ensure!(
            value.is_finite() && (parameter.minimum..=parameter.maximum).contains(&value),
            "Parameter value is out of range"
        );
        self.finish_edit()?;
        self.session.set_preview_parameter(id, value)?;
        Ok(())
    }

    pub fn set_preview_parameters(&mut self, values: &[(String, f32)]) -> Result<()> {
        let mut next = self.session.preview_values().clone();
        for (id, value) in values {
            let parameter = self
                .session
                .parameter(id)
                .ok_or_else(|| anyhow!("Parameter not found: {id}"))?;
            ensure!(
                value.is_finite() && (parameter.minimum..=parameter.maximum).contains(value),
                "Parameter value is out of range: {id}"
            );
            next.insert(id.clone(), *value);
        }
        self.finish_edit()?;
        self.session.set_preview_values(next)?;
        Ok(())
    }

    /// Cubism's Parts-style hierarchy: nested parts with their art meshes.
    pub fn mesh_tree(&self) -> Vec<TreeNode> {
        let parts: Vec<_> = self
            .session
            .part_ids()
            .iter()
            .filter_map(|id| self.session.part(id))
            .collect();
        let part_indices: HashMap<_, _> = parts
            .iter()
            .enumerate()
            .map(|(index, part)| (part.id.as_str(), index))
            .collect();
        let mut nodes = Vec::new();
        let mut parents = Vec::new();
        for part in &parts {
            parents.push(part_indices.get(part.parent_id.as_str()).copied());
            nodes.push(TreeNode::new(
                part.id.clone(),
                part.name.clone(),
                TreeNodeKind::Part,
            ));
        }
        for id in self.session.mesh_ids() {
            if let Some(mesh) = self.model().get_mesh(id) {
                parents.push(part_indices.get(mesh.part_id.as_str()).copied());
                let mut node =
                    TreeNode::new(mesh.id.clone(), mesh.name.clone(), TreeNodeKind::Mesh);
                node.triangles = Some(mesh.triangles.len());
                node.visible = Some(mesh.enabled);
                nodes.push(node);
            }
        }
        nest_nodes(nodes, parents)
    }

    /// Cubism's Deformer hierarchy: transform parents with art mesh leaves.
    pub fn deformer_tree(&self) -> Vec<TreeNode> {
        let transforms: Vec<_> = self
            .session
            .transform_ids()
            .iter()
            .filter_map(|id| self.session.transform(id))
            .collect();
        let transform_indices: HashMap<_, _> = transforms
            .iter()
            .enumerate()
            .map(|(index, transform)| (transform.id.as_str(), index))
            .collect();
        let mut nodes = Vec::new();
        let mut parents = Vec::new();
        for transform in &transforms {
            parents.push(transform_indices.get(transform.parent()).copied());
            nodes.push(TreeNode::new(
                transform.id.clone(),
                transform.name.clone(),
                match transform.kind() {
                    kasane_core::TransformKind::Warp => TreeNodeKind::WarpDeformer,
                    kasane_core::TransformKind::Rotation => TreeNodeKind::RotationDeformer,
                },
            ));
        }
        for id in self.session.mesh_ids() {
            if let Some(mesh) = self.model().get_mesh(id) {
                parents.push(transform_indices.get(mesh.deformer_id.as_str()).copied());
                let mut node =
                    TreeNode::new(mesh.id.clone(), mesh.name.clone(), TreeNodeKind::Mesh);
                node.triangles = Some(mesh.triangles.len());
                node.visible = Some(mesh.enabled);
                nodes.push(node);
            }
        }
        nest_nodes(nodes, parents)
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
            preview: PreviewState::default(),
            changed: OnceLock::new(),
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
        let model = self.model();
        if model.binding_for_mesh(&mesh.id).is_some() {
            return Some("This mesh has parameter keyforms".into());
        }
        if !mesh.deformer_id.is_empty() {
            return Some("This mesh depends on a deformer".into());
        }
        if !model.blend_bindings_for_target(&mesh.id).is_empty() {
            return Some("This mesh has BlendShape dependencies".into());
        }
        if !model.glues_for_mesh(&mesh.id).is_empty() {
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
        pending.changed.take();
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
                let mesh = self.model().get_mesh(&drawable.id)?;
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
                    triangles: mesh.triangles.clone(),
                })
            })
            .collect::<Vec<_>>()
            .into()
    }

    pub fn preview_name(&mut self, name: &str) -> Result<()> {
        self.begin_gesture(Gesture::Rename)?;
        let pending = self.pending.as_mut().unwrap();
        pending.changed.take();
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
        pending.changed.take();
        let result = pending.edit.set_mesh_opacity(&pending.original.id, opacity);
        if result.is_err() {
            self.pending = None;
        }
        Ok(result?)
    }

    fn pending_changed(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            *pending.changed.get_or_init(|| {
                pending
                    .edit
                    .candidate_document()
                    .get_mesh(&pending.original.id)
                    != Some(&pending.original)
            })
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
        let id = self
            .selected
            .clone()
            .ok_or_else(|| anyhow!("Select a layer first"))?;
        self.set_layer_visible(&id, visible)
    }

    pub fn set_layer_visible(&mut self, id: &str, visible: bool) -> Result<()> {
        self.finish_edit()?;
        let mesh = self
            .session
            .mesh(id)
            .ok_or_else(|| anyhow!("Layer not found: {id}"))?;
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
        let frame = if let Some(pending) = &mut self.pending {
            pending
                .preview
                .replace(
                    pending.edit.candidate_document(),
                    0,
                    self.session.preview_values().clone(),
                )
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?;
            pending
                .preview
                .frame(pending.edit.candidate_document(), 0)
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?
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
