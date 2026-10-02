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
    Controls,
}

struct PendingEdit {
    kind: Gesture,
    original: crate::control_edit::EditOriginal,
    edit: EditSession<'static>,
    preview: PreviewState,
    changed: OnceLock<bool>,
    topology_changed: bool,
}

/// The SDK owns all persistent model state and history. Selection is editor state.
/// A candidate edit lets a gesture preview without adding a history entry per event.
pub struct Document {
    session: AuthoringSession,
    import_name: Option<String>,
    selected: Option<String>,
    context: crate::PanelContext,
    projection_revision: u64,
    preview_revision: u64,
    object_index: std::sync::Mutex<Option<(u64, Arc<crate::ObjectIndex>)>>,
    canvas_topology: std::sync::Mutex<Option<Arc<crate::canvas_topology::CanvasTopology>>>,
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
        let projection = Arc::new(crate::ObjectIndex::build(session.document()));
        Ok(Self {
            session,
            import_name: None,
            context: crate::PanelContext {
                selections: selected
                    .iter()
                    .map(|id| crate::ObjectRef::new(crate::ObjectKind::Mesh, id))
                    .collect(),
                selection: selected
                    .as_ref()
                    .map(|id| crate::ObjectRef::new(crate::ObjectKind::Mesh, id)),
                inspected: selected
                    .as_ref()
                    .map(|id| crate::ObjectRef::new(crate::ObjectKind::Mesh, id)),
                ..Default::default()
            },
            projection_revision: 1,
            preview_revision: 0,
            object_index: std::sync::Mutex::new(Some((1, projection))),
            canvas_topology: std::sync::Mutex::new(None),
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

    pub(crate) fn model(&self) -> &kasane_core::Document {
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
        self.projection_revision
    }

    pub fn parameter_visuals(&self) -> HashMap<String, ParameterVisual> {
        let mut visuals = self.parameter_visuals_for_targets(
            &self
                .context
                .selections
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
        );
        if let Some(b) = self
            .context
            .active_binding
            .as_ref()
            .and_then(|r| self.object_index().binding(r).cloned())
        {
            for axis in &b.axes {
                if let Some(v) = visuals.get_mut(&axis.parameter_id) {
                    v.keys = axis
                        .keys
                        .iter()
                        .map(|&value| ParameterKey {
                            value,
                            selected: true,
                            partial: false,
                            targets: vec![b.target.id.clone()],
                            base: false,
                        })
                        .collect();
                }
            }
        }
        visuals
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

    pub fn preview_revision(&self) -> u64 {
        self.preview_revision
    }
    pub fn set_preview_parameter(&mut self, id: &str, value: f32) -> Result<()> {
        self.set_preview_parameters(&[(id.to_owned(), value)])
    }

    pub fn set_preview_parameters(&mut self, values: &[(String, f32)]) -> Result<()> {
        let mut next = self.session.preview_values().clone();
        let mut changed = false;
        for (id, value) in values {
            let parameter = self
                .session
                .parameter(id)
                .ok_or_else(|| anyhow!("Parameter not found: {id}"))?;
            ensure!(
                value.is_finite() && (parameter.minimum..=parameter.maximum).contains(value),
                "Parameter value is out of range: {id}"
            );
            changed |= self
                .session
                .preview_values()
                .get(id)
                .copied()
                .unwrap_or(parameter.default_value)
                != *value;
            next.insert(id.clone(), *value);
        }
        self.finish_edit()?;
        if changed {
            self.session.set_preview_values(next)?;
            self.preview_revision += 1;
        }
        Ok(())
    }

    pub fn mesh_tree(&self) -> Vec<TreeNode> {
        self.structure_tree(false)
    }
    pub fn deformer_tree(&self) -> Vec<TreeNode> {
        self.structure_tree(true)
    }
    fn structure_tree(&self, deformer: bool) -> Vec<TreeNode> {
        use crate::{ObjectKind as K, RelationKind as R};
        let index = self.object_index();
        let mut nodes = Vec::new();
        let mut references = Vec::new();
        for x in &index.objects {
            if x.diagnostic.is_some() {
                continue;
            }
            let kind = match x.reference.kind {
                K::Part if !deformer => TreeNodeKind::Part,
                K::Mesh => TreeNodeKind::Mesh,
                K::Deformer => match self
                    .model()
                    .get_transform(&x.reference.id)
                    .map(|t| t.kind())
                {
                    Some(kasane_core::TransformKind::Rotation) => TreeNodeKind::RotationDeformer,
                    _ => TreeNodeKind::WarpDeformer,
                },
                K::Offscreen if !deformer => TreeNodeKind::Offscreen,
                _ => continue,
            };
            let mut node = TreeNode::new(x.reference.id.clone(), x.name.clone(), kind);
            if kind == TreeNodeKind::Mesh {
                node.triangles = self.model().get_mesh(&node.id).map(|m| m.triangles.len());
                node.visible = x.enabled;
            }
            references.push(x.reference.clone());
            nodes.push(node);
        }
        let slots: HashMap<_, _> = references.iter().enumerate().map(|(i, r)| (r, i)).collect();
        let relation = if deformer {
            R::DeformParent
        } else {
            R::PartOwner
        };
        let mut parents = Vec::new();
        let mut unowned = Vec::new();
        let mut broken = Vec::new();
        for (i, r) in references.iter().enumerate() {
            let edge = index.outgoing(r).find(|e| e.kind == relation);
            let parent = edge.and_then(|e| slots.get(&e.target)).copied();
            if let Some(e) = edge.filter(|_| parent.is_none()) {
                nodes[i].diagnostic = Some(format!("Missing parent: {}", e.target.id));
                broken.push(i);
            } else if parent.is_none()
                && (r.kind == K::Mesh
                    || (!deformer && matches!(r.kind, K::Deformer | K::Offscreen)))
            {
                unowned.push(i);
            }
            parents.push(parent);
        }
        for i in 0..nodes.len() {
            let mut visited = std::collections::HashSet::new();
            let mut cursor = Some(i);
            while let Some(j) = cursor {
                if !visited.insert(j) {
                    nodes[i].diagnostic = Some("Parent cycle".into());
                    broken.push(i);
                    break;
                }
                cursor = parents[j];
            }
        }
        let mut add_group = |id: &str, name: &str, items: &[usize], nodes: &mut Vec<TreeNode>| {
            if !items.is_empty() {
                let slot = nodes.len();
                nodes.push(TreeNode::new(id.into(), name.into(), TreeNodeKind::Group));
                for &i in items {
                    parents[i] = Some(slot);
                }
                parents.push(None);
            }
        };
        unowned.retain(|i| !broken.contains(i));
        add_group(
            "unowned",
            if deformer {
                "No deformer parent"
            } else {
                "Unassigned to Part"
            },
            &unowned,
            &mut nodes,
        );
        add_group(
            "invalid",
            "Missing references / cycles",
            &broken,
            &mut nodes,
        );
        nest_nodes(nodes, parents)
    }

    pub fn selected_layer(&self) -> Option<Layer> {
        self.selected
            .as_deref()
            .and_then(|id| self.mesh(id))
            .map(Into::into)
    }

    /// Shared scalar inspector edits remain SDK transactions and history entries.
    pub fn set_object_property(
        &mut self,
        object: &crate::ObjectRef,
        field: &str,
        value: serde_json::Value,
    ) -> Result<()> {
        use crate::ObjectKind as K;
        ensure!(
            crate::editable_property(object.kind, field),
            "This property is read only"
        );
        self.finish_edit()?;
        let model = self.model();
        macro_rules! update {
            ($get:ident, $ty:ty, $replace:ident) => {{
                let old = model
                    .$get(&object.id)
                    .ok_or_else(|| anyhow!("Object no longer exists"))?;
                let mut properties = serde_json::to_value(old)?;
                properties[field] = value;
                let next: $ty = serde_json::from_value(properties)?;
                self.session
                    .edit("Edit object property", None, |edit| edit.$replace(next))?
                    .1
                    .changed
            }};
        }
        let changed = match object.kind {
            K::Part => update!(get_part, kasane_core::Part, replace_part),
            K::Deformer => update!(get_transform, kasane_core::Transform, replace_transform),
            K::Parameter => update!(get_parameter, kasane_core::Parameter, replace_parameter),
            K::Glue => update!(get_glue, kasane_core::Glue, replace_glue),
            K::Offscreen => update!(get_offscreen, kasane_core::Offscreen, replace_offscreen),
            K::Texture => update!(get_asset, kasane_core::ImageAsset, replace_asset),
            K::Expression => update!(
                get_expression,
                kasane_core::document::ExpressionAsset,
                replace_expression
            ),
            K::Motion => update!(
                get_motion,
                kasane_core::document::MotionClip,
                replace_motion
            ),
            K::Document => {
                let mut fields = serde_json::to_value(model.canvas())?;
                fields[field
                    .strip_prefix("canvas.")
                    .ok_or_else(|| anyhow!("Invalid canvas property"))?] = value;
                let canvas = serde_json::from_value(fields)?;
                self.session
                    .edit("Edit canvas property", None, |edit| {
                        edit.replace_canvas(canvas)
                    })?
                    .1
                    .changed
            }
            _ => return Err(anyhow!("This property is read only")),
        };
        self.session.drain_events();
        if changed {
            self.pristine_blank = false;
            self.projection_revision += 1;
            self.canvas_topology.get_mut().unwrap().take();
        }
        let index = self.object_index();
        self.context.reconcile(&index);
        Ok(())
    }
    pub fn context(&self) -> &crate::PanelContext {
        &self.context
    }
    pub fn object_index(&self) -> Arc<crate::ObjectIndex> {
        let model = self.model();
        let mut cache = self.object_index.lock().unwrap();
        if cache
            .as_ref()
            .is_none_or(|(revision, _)| *revision != self.projection_revision)
        {
            *cache = Some((
                self.projection_revision,
                Arc::new(crate::ObjectIndex::build(model)),
            ));
        }
        cache.as_ref().unwrap().1.clone()
    }
    pub fn select(&mut self, id: &str) -> Result<()> {
        self.select_object(crate::ObjectRef::new(crate::ObjectKind::Mesh, id))
    }
    pub fn select_object(&mut self, object: crate::ObjectRef) -> Result<()> {
        self.select_objects(vec![object])
    }
    pub fn select_objects(&mut self, objects: Vec<crate::ObjectRef>) -> Result<()> {
        let index = self.object_index();
        ensure!(
            objects.iter().all(|r| index.get(r).is_some()),
            "Object no longer exists"
        );
        self.finish_edit()?;
        let mut unique = Vec::new();
        for object in objects {
            if !unique.contains(&object) {
                unique.push(object);
            }
        }
        let primary = unique.last().cloned();
        self.context.selections = unique;
        self.context.inspected = primary.clone();
        self.context.selection = primary.clone();
        self.selected = primary
            .as_ref()
            .filter(|r| r.kind == crate::ObjectKind::Mesh)
            .map(|r| r.id.clone());
        if self.context.active_binding.as_ref().is_some_and(|r| {
            index
                .binding(r)
                .is_none_or(|b| Some(&b.target) != primary.as_ref())
        }) {
            self.context.active_binding = None;
        }
        Ok(())
    }
    pub fn focus_parameter(&mut self, id: &str) -> Result<()> {
        ensure!(
            self.model().get_parameter(id).is_some(),
            "Parameter not found"
        );
        self.finish_edit()?;
        let r = crate::ObjectRef::new(crate::ObjectKind::Parameter, id);
        self.context.parameter_focus = Some(r.clone());
        self.context.inspected = Some(r);
        Ok(())
    }
    pub fn open_binding(&mut self, binding: crate::ObjectRef) -> Result<()> {
        let index = self.object_index();
        let b = index
            .binding(&binding)
            .ok_or_else(|| anyhow!("Binding no longer exists"))?;
        self.select_object(b.target.clone())?;
        self.context.active_binding = Some(binding.clone());
        self.context.inspected = Some(binding);
        Ok(())
    }
    pub fn navigate(&mut self, object: crate::ObjectRef) -> Result<()> {
        ensure!(
            self.object_index().get(&object).is_some(),
            "Object no longer exists"
        );
        self.context.remember();
        if object.kind.is_binding() {
            self.open_binding(object)
        } else if object.kind == crate::ObjectKind::Parameter {
            self.focus_parameter(&object.id)
        } else if object.kind.is_edit_target() {
            self.select_object(object)
        } else {
            self.finish_edit()?;
            self.context.inspected = Some(object);
            Ok(())
        }
    }
    pub fn navigate_back(&mut self) -> Result<()> {
        self.finish_edit()?;
        if let Some(n) = self.context.back.pop() {
            self.context.selections = n.selections;
            self.context.selection = n.selection;
            self.context.parameter_focus = n.parameter_focus;
            self.context.active_binding = n.active_binding;
            self.context.inspected = n.inspected;
            let index = self.object_index();
            self.context.reconcile(&index);
            self.selected = self
                .context
                .selection
                .as_ref()
                .filter(|r| r.kind == crate::ObjectKind::Mesh)
                .map(|r| r.id.clone());
        }
        Ok(())
    }
    pub fn preview_keyform(&mut self, binding: &crate::ObjectRef, point: usize) -> Result<()> {
        ensure!(
            self.context.active_binding.as_ref() == Some(binding),
            "Binding is no longer active"
        );
        let index = self.object_index();
        let b = index
            .binding(binding)
            .ok_or_else(|| anyhow!("Binding not found"))?;
        let keys = b
            .points
            .get(point)
            .ok_or_else(|| anyhow!("Keyform not found"))?;
        ensure!(keys.len() == b.axes.len(), "Invalid stored keyform");
        let values = b
            .axes
            .iter()
            .zip(keys)
            .map(|(a, &v)| (a.parameter_id.clone(), v))
            .collect::<Vec<_>>();
        self.set_preview_parameters(&values)
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
            Gesture::Controls => "Edit canvas controls",
        };
        self.pending = Some(PendingEdit {
            kind,
            original: crate::control_edit::EditOriginal::Mesh(original),
            edit: self.session.begin_owned_edit(label, None)?,
            preview: PreviewState::default(),
            changed: OnceLock::new(),
            topology_changed: false,
        });
        Ok(())
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
    pub(crate) fn begin_control_edit(
        &mut self,
        targets: &[crate::control_edit::ControlEdit],
    ) -> Result<()> {
        self.finish_edit()?;
        self.pending = Some(PendingEdit {
            kind: Gesture::Controls,
            original: crate::control_edit::EditOriginal::Batch(
                targets.iter().map(|t| t.original.clone()).collect(),
            ),
            edit: self
                .session
                .begin_owned_edit("Edit canvas controls", None)?,
            preview: PreviewState::default(),
            changed: OnceLock::new(),
            topology_changed: false,
        });
        Ok(())
    }
    pub(crate) fn preview_control_edit(
        &mut self,
        targets: &[crate::control_edit::ControlEdit],
        selected: &[crate::ComponentRef],
        change: crate::control_edit::ControlChange,
    ) -> Result<()> {
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| anyhow!("No canvas edit"))?;
        ensure!(pending.kind == Gesture::Controls, "Another edit is active");
        pending.changed.take();
        self.projection_revision += 1;
        for target in targets {
            let ids = selected
                .iter()
                .filter(|c| c.object == target.object)
                .map(|c| c.id)
                .collect::<Vec<_>>();
            let shape = || {
                pending
                    .edit
                    .candidate_document()
                    .get_transform(&target.object.id)
                    .and_then(|t| t.warp())
                    .map(|w| (w.rows, w.columns))
            };
            let before = shape();
            target.preview(&mut pending.edit, &ids, change)?;
            let after = pending
                .edit
                .candidate_document()
                .get_transform(&target.object.id)
                .and_then(|t| t.warp())
                .map(|w| (w.rows, w.columns));
            if before != after {
                pending.topology_changed = true;
                self.canvas_topology.get_mut().unwrap().take();
            }
        }
        Ok(())
    }
    pub fn geometry(&self, scene: &SceneSnapshot) -> Arc<crate::CanvasScene> {
        Arc::new(crate::CanvasScene::new(self, scene.clone()))
    }

    pub(crate) fn canvas_topology(&self) -> Arc<crate::canvas_topology::CanvasTopology> {
        self.canvas_topology
            .lock()
            .unwrap()
            .get_or_insert_with(|| {
                Arc::new(crate::canvas_topology::CanvasTopology::new(self.model()))
            })
            .clone()
    }

    pub fn preview_name(&mut self, name: &str) -> Result<()> {
        self.begin_gesture(Gesture::Rename)?;
        self.canvas_topology.get_mut().unwrap().take();
        let pending = self.pending.as_mut().unwrap();
        pending.changed.take();
        self.projection_revision += 1;
        let result = pending.edit.rename_mesh(pending.original.id(), name);
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
        self.projection_revision += 1;
        let result = pending
            .edit
            .set_mesh_opacity(pending.original.id(), opacity);
        if result.is_err() {
            self.pending = None;
        }
        Ok(result?)
    }

    fn pending_changed(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            *pending.changed.get_or_init(|| {
                !pending
                    .original
                    .unchanged(pending.edit.candidate_document())
            })
        })
    }

    pub fn finish_edit(&mut self) -> Result<()> {
        let changed = self.pending_changed();
        if let Some(pending) = self.pending.take() {
            self.projection_revision += 1;
            if pending.kind == Gesture::Rename || pending.topology_changed {
                self.canvas_topology.get_mut().unwrap().take();
            }
            if changed {
                pending.edit.commit_to(&mut self.session)?;
                self.session.drain_events();
                self.pristine_blank = false;
            }
        }
        Ok(())
    }

    pub fn cancel_edit(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.projection_revision += 1;
            if pending.kind == Gesture::Rename || pending.topology_changed {
                self.canvas_topology.get_mut().unwrap().take();
            }
        }
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
        self.projection_revision += 1;
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
        self.projection_revision += 1;
        self.canvas_topology.get_mut().unwrap().take();
        self.session.drain_events();
        let index = self.object_index();
        self.context.reconcile(&index);
        self.selected = self
            .context
            .selection
            .as_ref()
            .filter(|r| r.kind == crate::ObjectKind::Mesh)
            .map(|r| r.id.clone());
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
