use crate::{SceneSnapshot, SceneTexture, TreeNode, TreeNodeKind, tree::nest_nodes};
use anyhow::{Result, anyhow, ensure};
use std::collections::HashMap;
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};
use uuid::Uuid;
use zipzi_core::{Mesh, ModelSetup, ParameterKind, preview::PreviewState};
use zipzi_sdk::{AuthoringSession, EditReceipt, EditSession, SaveReceipt};

#[path = "object_state.rs"]
mod object_state;
pub use object_state::{ObjectState, ObjectStateIndex};

#[path = "mesh_generation.rs"]
mod mesh_generation;
pub use mesh_generation::{
    AlphaMeshOptions, MeshGenerationOptions, MeshGenerationPreview, MeshGenerationSource,
    MeshGridOptions,
};

#[path = "sorting.rs"]
mod sorting;
pub use sorting::{OrganizationDestination, ParameterDestination, ReorderPosition};

#[path = "parameter.rs"]
mod parameter;
pub use parameter::{
    NewParameter, ParameterCreateError, ParameterDraft, ParameterGroupOption, ParameterGroupValue,
    ParameterManagementError, ParameterValidationError,
};

#[path = "parameter_group.rs"]
mod parameter_group;
#[path = "timeline_document.rs"]
mod timeline_document;
pub use parameter_group::ParameterGroupDraft;
#[path = "parameter_export.rs"]
mod parameter_export;
pub use parameter_export::ParameterExportDiagnostic;

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
    pub group_id: Option<String>,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gesture {
    Rename,
    Opacity,
    Controls,
    Properties(u64),
    ExpressionEntry(Uuid),
}

enum PendingOriginal {
    ExpressionEntry(zipzi_sdk::ExpressionEntryAddress),
    Controls(crate::control_edit::EditOriginal),
    Properties(
        crate::ObjectRef,
        crate::PropertyValues,
        crate::PropertyScope,
    ),
}
impl PendingOriginal {
    fn id(&self) -> &str {
        match self {
            Self::ExpressionEntry(address) => address.entry_id(),
            Self::Controls(original) => original.id(),
            Self::Properties(object, _, _) => &object.id,
        }
    }
    fn unchanged(&self, model: &zipzi_core::Document) -> bool {
        match self {
            Self::ExpressionEntry(address) => model
                .get_expression_entry(address.entry_id())
                .is_some_and(|(_, entry)| {
                    zipzi_sdk::ExpressionEntryFields::from(entry) == *address.expected()
                }),
            Self::Controls(original) => original.unchanged(model),
            Self::Properties(object, values, scope) => {
                Document::property_values_in(model, object, scope)
                    .is_ok_and(|current| current == *values)
            }
        }
    }
}

struct PendingEdit {
    kind: Gesture,
    original: PendingOriginal,
    edit: crate::runtime_host::HostedEdit,
    preview: PreviewState,
    changed: OnceLock<bool>,
    topology_changed: bool,
    draw_groups_changed: bool,
    property_write: Option<crate::property::PropertyWrite>,
}

/// CPU presentation prepared on a load worker; contains no writable SDK state.
pub(crate) struct PreparedDocument {
    version: zipzi_sdk::Version,
    textures: textures::DocumentTextures,
    projection: Arc<crate::ObjectIndex>,
}

/// The SDK owns all persistent model state and history. Selection is editor state.
/// A candidate edit lets a gesture preview without adding a history entry per event.
pub struct Document {
    pub(crate) session: crate::runtime_host::HostedSession,
    import_name: Option<String>,
    selected: Option<String>,
    context: crate::PanelContext,
    projection_revision: u64,
    animation_input_revision: u64,
    preview_revision: u64,
    object_state_index: std::sync::Mutex<Option<(u64, Arc<ObjectStateIndex>)>>,
    object_index: std::sync::Mutex<Option<(u64, Arc<crate::ObjectIndex>)>>,
    authoring_capability_cache: std::sync::Mutex<authoring::AuthoringCapabilityCache>,
    #[cfg(test)]
    object_index_builds: std::sync::atomic::AtomicUsize,
    pub(crate) animation_catalog: std::sync::Mutex<Option<(u64, Arc<crate::AnimationCatalog>)>>,
    canvas_topology: std::sync::Mutex<Option<Arc<crate::canvas_topology::CanvasTopology>>>,
    pending: Option<PendingEdit>,
    next_property_edit: u64,
    textures: textures::DocumentTextures,
    pristine_blank: Option<zipzi_sdk::Version>,
}

#[path = "authoring.rs"]
mod authoring;
pub use authoring::{
    AnimationDeletionPlan, AnimationLibraryPlan, AnimationRemoval, AuthoringAction,
    AuthoringCollection, AuthoringCollectionCapabilities, AuthoringPlanningError,
    AuthoringSessionReason, BusinessAction, BusinessDestination, BusinessPlan,
    ExpressionAuthoringCapabilities, ExpressionEntryEditToken, MotionCopyKind, MotionCopyPlan,
    ObjectAuthoringCapabilities, OperationResult, OrdinaryBindingAction, OrdinaryBindingCapture,
    OrdinaryBindingPlan, OrganizationAuthoringCapabilities, PaletteMove, PaletteMoveDestination,
    PaletteMovePlan, PaletteMovePlanningError, PalettePosition, PalettePreservation,
    PaletteProjection, PaletteReparentCapture, PaletteReparentPlan, PhysicsAuthoringCapabilities,
    PhysicsChannelAction, PoseAuthoringCapabilities, ReferenceAuthoringCapabilities,
    ResourceAuthoringCapabilities, SceneAuthoringCapabilities, SceneContentAuthoringCapabilities,
    SceneObjectCreationPlan, SceneObjectType, TextureInfo,
};
mod edits;
mod physics;
mod projection;
mod properties;
mod textures;
mod transfer;

impl Document {
    pub fn runtime_session_id(&self) -> zipzi_runtime::SessionId {
        self.session.id
    }
    pub fn runtime_host(&self) -> crate::RuntimeHost {
        self.session.host.clone()
    }
    pub(crate) fn sync_runtime_events(&mut self) -> Result<bool> {
        let receipts = match self.session.receipts() {
            Ok(receipts) => receipts,
            Err(zipzi_runtime::RuntimeError::ResyncRequired) => {
                self.refresh_textures()?;
                self.reconcile_parameter_preview()?;
                self.after_history(true);
                self.session.refresh_metadata()?;
                self.session.take_management_changed();
                return Ok(true);
            }
            Err(error) => return Err(error.into()),
        };
        let changed = receipts.iter().any(|r| r.changed);
        if changed {
            self.refresh_textures()?;
            self.reconcile_parameter_preview()?;
            self.pristine_blank = None;
            for receipt in receipts {
                self.consume_authoring_receipt(&receipt, true);
            }
        }
        self.session.refresh_metadata()?;
        let management_changed = self.session.take_management_changed();
        Ok(changed || management_changed)
    }
    pub(crate) fn set_import_name(&mut self, name: Option<String>) {
        self.import_name = name;
    }
    pub(crate) fn saved_by_runtime(&mut self) {
        self.pristine_blank = None;
    }
    /// Presentation indexes use the projection revision as their cache key.
    /// Topology also has an unversioned cache and must be cleared explicitly.
    fn invalidate_projection(&mut self, topology_changed: bool) {
        self.projection_revision += 1;
        self.animation_input_revision = self.session.version().revision;
        if topology_changed {
            self.canvas_topology.get_mut().unwrap().take();
        }
    }

    pub(crate) fn animation_generation(&self) -> u64 {
        self.animation_input_revision
    }

    pub fn model_generation(&self) -> u64 {
        // SDK generation identifies project replacement; revision advances on
        // every committed content change, including checkpoint Undo / Redo.
        // Document never replaces its session with another project in place.
        self.session.version().revision
    }
    pub(crate) fn animation_source(&self) -> crate::AnimationSource {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = zipzi_core::benchmark::span("editor.animation_source");
        crate::AnimationSource {
            snapshot: Arc::new(self.session.read_snapshot()),
            properties: self
                .has_inspector_edit()
                .then(|| Arc::new(self.model().fork_candidate())),
            textures: self.textures.snapshot(),
            candidate: None,
            materialized: Default::default(),
        }
    }
    pub fn new_blank() -> Result<Self> {
        let mut document = Self::from_session(Self::empty_session()?)?;
        document.pristine_blank = Some(document.session.version());
        document
            .session
            .host
            .mark_pristine_view(document.session.id, document.session.version());
        Ok(document)
    }

    fn empty_session() -> Result<AuthoringSession> {
        Ok(AuthoringSession::new(
            &Uuid::new_v4().to_string(),
            ModelSetup::default(),
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
        let entry = zipzi_project::resolve_model3_entry(path)
            .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
        let path = entry.as_path();
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

    pub fn import_psd(source: &Path) -> Result<(Self, Vec<String>)> {
        let mut session = Self::empty_session()?;
        let receipt = session.import_psd_in_memory(source, None)?;
        let mut notices = receipt.project.warnings;
        notices.extend(
            receipt
                .project
                .diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message)),
        );
        let mut document = Self::from_session(session)?;
        document.import_name = source
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned());
        Ok((document, notices))
    }

    pub(crate) fn prepare_runtime_view(session: &AuthoringSession) -> Result<PreparedDocument> {
        Ok(PreparedDocument {
            version: session.version(),
            textures: textures::DocumentTextures::new(session)?,
            projection: Arc::new(crate::ObjectIndex::build(session.document())),
        })
    }
    fn from_session(session: AuthoringSession) -> Result<Self> {
        Self::from_hosted(crate::runtime_host::HostedSession::from_session(session)?)
    }
    pub fn attach_runtime(host: crate::RuntimeHost, id: zipzi_runtime::SessionId) -> Result<Self> {
        Self::from_hosted(crate::runtime_host::HostedSession::attach(host, id)?)
    }
    fn from_hosted(session: crate::runtime_host::HostedSession) -> Result<Self> {
        let prepared = session.host.take_prepared_view(session.id)?;
        let prepared = match prepared {
            Some(prepared) if prepared.version == session.version() => prepared,
            _ => Self::prepare_runtime_view(&session.read())?,
        };
        let PreparedDocument {
            textures,
            projection,
            ..
        } = prepared;
        let pristine_blank = session
            .host
            .pristine_view(session.id)
            .filter(|version| *version == session.version());
        let animation_input_revision = session.version().revision;
        Ok(Self {
            animation_input_revision,
            session,
            import_name: None,
            context: crate::PanelContext::default(),
            projection_revision: 1,
            preview_revision: 0,
            object_state_index: std::sync::Mutex::new(None),
            object_index: std::sync::Mutex::new(Some((1, projection))),
            authoring_capability_cache: Default::default(),
            #[cfg(test)]
            object_index_builds: std::sync::atomic::AtomicUsize::new(1),
            animation_catalog: std::sync::Mutex::new(None),
            canvas_topology: std::sync::Mutex::new(None),
            selected: None,
            pending: None,
            next_property_edit: 0,
            textures,
            pristine_blank,
        })
    }

    fn refresh_textures(&mut self) -> Result<()> {
        self.textures.refresh(&self.session.read())
    }

    pub fn save(&mut self, path: &Path) -> Result<SaveReceipt> {
        self.finish_edit()?;
        let receipt = self.session.save_project(path, None)?;
        self.pristine_blank = None;
        Ok(receipt)
    }

    pub fn export_settings(&self) -> zipzi_core::ExportSettings {
        self.model().export_settings()
    }
    pub fn model_setup(&self) -> zipzi_core::ModelSetup {
        self.model().model_setup()
    }

    pub fn source_canvas(&self) -> Option<zipzi_core::Canvas> {
        self.model().source_canvas()
    }

    pub fn path(&self) -> Option<&Path> {
        self.session.project_path()
    }
    pub fn import_name(&self) -> Option<&str> {
        self.import_name.as_deref()
    }
    pub(crate) fn is_pristine_blank(&self) -> bool {
        self.pristine_blank
            .is_some_and(|version| version == self.session.version())
    }
    pub fn modified(&self) -> bool {
        (self.session.modified() && !self.is_pristine_blank()) || self.pending_changed()
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

    pub(crate) fn model(&self) -> crate::runtime_host::ModelRead<'_> {
        self.pending
            .as_ref()
            .map_or_else(|| self.session.document(), |p| p.edit.candidate_document())
    }

    fn layer(&self, id: &str) -> Option<Layer> {
        let mesh = self.mesh(id)?;
        Some(Layer {
            id: mesh.id,
            name: mesh.name,
            opacity: mesh.appearance.opacity,
            visible: !self.model().editor_state().get(id).hidden_in_editor,
            vertices: mesh.vertex_ids.len(),
            triangles: mesh.triangles.len(),
        })
    }

    pub fn layers(&self) -> Vec<Layer> {
        self.session
            .mesh_ids()
            .iter()
            .filter_map(|id| self.layer(id))
            .collect()
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

    /// Source-art metadata follows committed edits, never interactive pose previews.
    pub fn mesh_texture_revision(&self) -> u64 {
        self.session.document().revision()
    }

    /// Static source-art crop and UV footprint; never evaluates posed geometry.
    pub fn mesh_texture_region(&self, id: &str) -> Option<crate::MeshTextureRegion> {
        let model_read_guard_55 = self.session.document();
        let mesh = model_read_guard_55.get_mesh(id)?;
        let texture = self.textures.get(&mesh.texture_asset_id)?;
        crate::MeshTextureRegion::new(texture, mesh)
    }

    pub fn selected_layer(&self) -> Option<Layer> {
        self.selected.as_deref().and_then(|id| self.layer(id))
    }

    pub fn context(&self) -> &crate::PanelContext {
        &self.context
    }
    /// Changes when inspector-visible object data changes, including candidates.
    pub fn properties_revision(&self) -> u64 {
        self.projection_revision
    }

    pub fn object_index(&self) -> Arc<crate::ObjectIndex> {
        let model = self.model();
        let mut cache = self.object_index.lock().unwrap();
        if cache
            .as_ref()
            .is_none_or(|(revision, _)| *revision != self.projection_revision)
        {
            #[cfg(test)]
            self.object_index_builds
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            *cache = Some((
                self.projection_revision,
                Arc::new(crate::ObjectIndex::build(&model)),
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
        self.finish_context_edit()?;
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
        self.finish_context_edit()?;
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
            self.finish_context_edit()?;
            self.context.inspected = Some(object);
            Ok(())
        }
    }
    pub fn navigate_back(&mut self) -> Result<()> {
        self.finish_context_edit()?;
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
        ensure!(
            keys.len() == b.axes.iter().filter(|a| !a.parameter_id.is_empty()).count(),
            "Invalid stored keyform"
        );
        let values = b
            .axes
            .iter()
            .filter(|a| !a.parameter_id.is_empty())
            .zip(keys)
            .map(|(a, &v)| (a.parameter_id.clone(), v))
            .collect::<Vec<_>>();
        self.set_preview_parameters(&values)
    }

    pub fn selected_vertex_positions(&self) -> Vec<(zipzi_core::VertexId, crate::ModelPoint)> {
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
    pub fn geometry(&self, scene: &SceneSnapshot) -> Arc<crate::CanvasScene> {
        Arc::new(crate::CanvasScene::new(self, scene.clone()))
    }

    pub(crate) fn canvas_topology(&self) -> Arc<crate::canvas_topology::CanvasTopology> {
        self.canvas_topology
            .lock()
            .unwrap()
            .get_or_insert_with(|| {
                Arc::new(crate::canvas_topology::CanvasTopology::new(&self.model()))
            })
            .clone()
    }

    pub fn deformer_controls_visible(&self, id: &str) -> bool {
        self.object_state(&crate::ObjectRef::new(crate::ObjectKind::Deformer, id))
            .visible()
    }

    pub fn snapshot(&mut self) -> Result<SceneSnapshot> {
        let frame = if let Some(pending) = &mut self.pending {
            pending
                .preview
                .replace(
                    &pending.edit.candidate_document(),
                    0,
                    self.session.preview_values().clone(),
                )
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?;
            pending
                .preview
                .frame(&pending.edit.candidate_document(), 0)
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?
        } else {
            self.session.preview_frame()?
        };
        let viewport_frame = self.viewport_frame(&frame);
        Ok(SceneSnapshot {
            model_opacity: 1.,
            runtime_frame: frame,
            viewport_frame,
            textures: self.textures.snapshot(),
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
