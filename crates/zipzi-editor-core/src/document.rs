use crate::{SceneSnapshot, SceneTexture, TreeNode, TreeNodeKind, tree::nest_nodes};
use anyhow::{Result, anyhow, ensure};
use std::collections::HashMap;
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};
use uuid::Uuid;
use zipzi_core::{Mesh, ModelSetup, ParameterKind, preview::PreviewState};
use zipzi_sdk::Publication;
use zipzi_sdk::{AuthoringSession, Commands, SaveReceipt};

#[path = "object_state.rs"]
mod object_state;
pub use object_state::{ObjectState, ObjectStateIndex};

#[path = "mesh_generation.rs"]
mod mesh_generation;
pub use mesh_generation::{
    MeshGenerationOptions, MeshGenerationPreview, MeshGenerationSource, MeshGridOptions,
    MeshSettings,
};

#[path = "document/manual_mesh.rs"]
mod manual_mesh;
pub use manual_mesh::ManualMeshTarget;

#[path = "document/warp_grid.rs"]
mod warp_grid;
pub use warp_grid::WarpGridPreviewMetrics;

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
    GlueWeights,
    Properties(u64),
    ExpressionEntry(Uuid),
}

enum PendingOriginal {
    RotationLength(String, Option<f32>),
    GlueWeights(zipzi_core::Glue),
    ExpressionEntry(crate::FieldCapture<zipzi_contract::document::ExpressionEntryFields>),
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
            Self::RotationLength(id, _) => id,
            Self::GlueWeights(glue) => &glue.id,
            Self::ExpressionEntry(address) => &address.id,
            Self::Controls(original) => original.id(),
            Self::Properties(object, _, _) => &object.id,
        }
    }
    fn unchanged(&self, model: &zipzi_core::state::DocumentState) -> bool {
        match self {
            Self::RotationLength(id, value) => {
                model.editor_state().rotation_bone_lengths.get(id).copied() == *value
            }
            Self::GlueWeights(glue) => model.glue(&glue.id) == Some(glue),
            Self::ExpressionEntry(address) => {
                model
                    .expression_entry(&address.id)
                    .is_some_and(|(_, entry)| {
                        zipzi_contract::document::ExpressionEntryFields::from(entry)
                            == *&address.value
                    })
            }
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
    edit: crate::runtime_host::HostedDraft,
    preview: PreviewState,
    changed: OnceLock<bool>,
    topology_changed: bool,
    draw_groups_changed: bool,
    property_write: Option<crate::property::PropertyWrite>,
}

struct ManualPending {
    edit: crate::runtime_host::HostedDraft,
    preview: PreviewState,
    changed: bool,
}

struct GenerationPending {
    edit: crate::EditPreview<zipzi_sdk::ManualMeshOutcome>,
    preview: PreviewState,
}

/// CPU presentation prepared on a load worker; contains no writable SDK state.
pub(crate) struct PreparedDocument {
    content: zipzi_sdk::ContentId,
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
    tree_cache: std::sync::Mutex<[Option<crate::tree::TreeProjection>; 2]>,
    authoring_capability_cache: std::sync::Mutex<authoring::AuthoringCapabilityCache>,
    #[cfg(test)]
    object_index_builds: std::sync::atomic::AtomicUsize,
    pub(crate) animation_catalog: std::sync::Mutex<Option<(u64, Arc<crate::AnimationCatalog>)>>,
    canvas_topology: std::sync::Mutex<Option<Arc<crate::canvas_topology::CanvasTopology>>>,
    canvas_topology_source:
        std::sync::Mutex<Option<(zipzi_sdk::QuerySource, zipzi_core::state::ContentId)>>,
    previous_canvas_topology: std::sync::Mutex<Option<Arc<crate::canvas_topology::CanvasTopology>>>,
    previous_canvas_source: std::sync::Mutex<Option<zipzi_sdk::QuerySource>>,
    changed_canvas_meshes: std::sync::Mutex<Option<std::collections::HashSet<String>>>,
    pending: Option<PendingEdit>,
    manual_pending: Option<ManualPending>,
    manual_vertex_ids: std::cell::RefCell<
        std::collections::HashMap<
            String,
            std::sync::Arc<std::sync::Mutex<zipzi_sdk::ManualVertexIdAllocator>>,
        >,
    >,
    generation_pending: Option<GenerationPending>,
    generation_texture: Option<SceneTexture>,
    next_property_edit: u64,
    textures: textures::DocumentTextures,
    pristine_blank: Option<zipzi_sdk::Version>,
}

#[path = "authoring.rs"]
mod authoring;
pub use authoring::{
    AnimationDeletionPlan, AnimationLibraryPlan, AnimationRemoval, AuthoringAction,
    AuthoringCollectionCapabilities, AuthoringPlanningError, AuthoringSessionReason,
    BusinessAction, BusinessDestination, BusinessPlan, CollectionTarget, EditPreview,
    ExpressionAuthoringCapabilities, ExpressionEntryEditToken, FieldCapture, MotionCopyKind,
    MotionCopyPlan, ObjectAuthoringCapabilities, OperationResult, OrdinaryBindingAction,
    OrdinaryBindingCapture, OrdinaryBindingPlan, OrganizationAuthoringCapabilities, PaletteMove,
    PaletteMoveDestination, PaletteMovePlan, PaletteMovePlanningError, PalettePosition,
    PalettePreservation, PaletteProjection, PaletteReparentCapture, PaletteReparentPlan,
    ParameterKeyformAction, PhysicsAuthoringCapabilities, PhysicsChannelAction,
    PoseAuthoringCapabilities, ReferenceAuthoringCapabilities, ResourceAuthoringCapabilities,
    SceneAuthoringCapabilities, SceneContentAuthoringCapabilities, SceneObjectCreationPlan,
    SceneObjectType, TextureInfo, TreeDeletionCancellation, TreeDeletionCompletion,
    TreeDeletionPlan, TreeDeletionTask,
};
mod edits;
mod glue;
mod physics;
mod projection;
mod properties;
mod textures;
mod transfer;

impl Document {
    pub fn runtime_session_id(&self) -> zipzi_sdk::SessionKey {
        self.session.id
    }
    pub fn runtime_host(&self) -> crate::RuntimeHost {
        self.session.host.clone()
    }
    pub(crate) fn sync_runtime_events(&mut self) -> Result<bool> {
        let receipts = match self.session.receipts() {
            Ok(receipts) => receipts,
            Err(error) if error.code == "EVENT_GAP" => {
                self.cancel_edit();
                self.refresh_textures()?;
                self.reconcile_parameter_preview()?;
                self.after_history(true);
                self.session.refresh_metadata()?;
                self.session.take_management_changed();
                return Ok(true);
            }
            Err(error) => return Err(error.into()),
        };
        let changed = receipts
            .iter()
            .any(|r| r.kind != zipzi_sdk::PublicationKind::Unchanged);
        if changed {
            self.cancel_edit();
            self.refresh_textures()?;
            self.reconcile_parameter_preview()?;
            self.pristine_blank = None;
            for receipt in receipts {
                self.consume_authoring_receipt(&receipt, None);
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
            self.invalidate_canvas_topology(None);
        }
    }

    pub(crate) fn invalidate_canvas_topology(
        &mut self,
        changed: Option<std::collections::HashSet<String>>,
    ) {
        let old = self.canvas_topology.get_mut().unwrap().take();
        let old_source = self.canvas_topology_source.get_mut().unwrap().take();
        let previous = self.previous_canvas_topology.get_mut().unwrap();
        let known_changes = self.changed_canvas_meshes.get_mut().unwrap();
        if let Some(old) = old {
            *previous = Some(old);
            *self.previous_canvas_source.get_mut().unwrap() = old_source.map(|(source, _)| source);
            *known_changes = changed;
        } else if previous.is_some() {
            *known_changes = match (known_changes.take(), changed) {
                (Some(mut a), Some(b)) => {
                    a.extend(b);
                    Some(a)
                }
                _ => None,
            };
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
        let snapshot = if self.has_inspector_edit() {
            self.pending
                .as_ref()
                .expect("inspector gesture")
                .edit
                .read_snapshot()
                .expect("active inspector Draft")
        } else {
            self.session.sourced_snapshot()
        };
        crate::AnimationSource {
            snapshot: Arc::new(snapshot),
            textures: self.textures.snapshot(),
            candidate: None,
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
        Ok(AuthoringSession::new(zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                Uuid::new_v4().to_string(),
                ModelSetup::default(),
            )?,
            Default::default(),
        )?))
    }

    /// Decode into a separate resident host; invalid content cannot replace a live view.
    pub fn open(path: &Path) -> Result<Self> {
        Self::load_document(path, None).map(|(document, _)| document)
    }

    pub fn import_model3(path: &Path) -> Result<(Self, Vec<String>)> {
        let entry = zipzi_project::resolve_model3_entry(path)?;
        let (mut document, notices) = Self::load_document(
            &entry,
            Some(zipzi_contract::kernel_wire::ImportFormat::Model3),
        )?;
        document.import_name = model_name_from_path(&entry);
        Ok((document, notices))
    }

    pub fn import_psd(path: &Path) -> Result<(Self, Vec<String>)> {
        let (mut document, notices) =
            Self::load_document(path, Some(zipzi_contract::kernel_wire::ImportFormat::Psd))?;
        document.import_name = path
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned());
        Ok((document, notices))
    }

    fn load_document(
        path: &Path,
        format: Option<zipzi_contract::kernel_wire::ImportFormat>,
    ) -> Result<(Self, Vec<String>)> {
        let host = crate::RuntimeHost::new();
        let client = host.client_id();
        let id = host.write().create_session(
            client,
            &Uuid::new_v4().to_string(),
            ModelSetup::default(),
        )?;
        let version = host.read().session_read(client, id)?.version();
        let job = if let Some(format) = format {
            host.write()
                .request_import(client, version, path, format, Default::default())?
        } else {
            let io = host.read().project_io().clone();
            host.write()
                .request_open(client, version, &io, path, Default::default())?
        };
        let completed = host
            .dispatch_job(job)?
            .run_with_stack("zipzi-document-load", 8 << 20);
        let prepared = completed
            .prepared_snapshot()
            .map(Self::prepare_runtime_view);
        host.write()
            .complete(completed)
            .expect("completion belongs to native load host");
        let result = host
            .read()
            .job(client, job)?
            .result
            .clone()
            .expect("completed load");
        host.write().release_job(client, job)?;
        let (mut notices, diagnostics) = match result? {
            zipzi_runtime::JobValue::Imported(receipt) => (receipt.warnings, receipt.diagnostics),
            zipzi_runtime::JobValue::Opened(receipt) => (vec![], receipt.diagnostics),
            _ => unreachable!("load returned another job value"),
        };
        notices.extend(
            diagnostics
                .into_iter()
                .map(|d| format!("{}: {}", d.code, d.message)),
        );
        if let Some(prepared) = prepared {
            host.prepare_view(id, prepared);
        }
        Ok((Self::attach_runtime(host, id)?, notices))
    }

    pub(crate) fn prepare_runtime_view(snapshot: &zipzi_sdk::Snapshot) -> Result<PreparedDocument> {
        Ok(PreparedDocument {
            content: snapshot.content_id(),
            textures: textures::DocumentTextures::new(snapshot)?,
            projection: Arc::new(crate::ObjectIndex::build(snapshot.document())),
        })
    }
    fn from_session(session: AuthoringSession) -> Result<Self> {
        Self::from_hosted(crate::runtime_host::HostedSession::from_session(session)?)
    }
    pub fn attach_runtime(host: crate::RuntimeHost, id: zipzi_sdk::SessionKey) -> Result<Self> {
        Self::from_hosted(crate::runtime_host::HostedSession::attach(host, id)?)
    }
    fn from_hosted(session: crate::runtime_host::HostedSession) -> Result<Self> {
        let prepared = session.host.take_prepared_view(session.id)?;
        let prepared = match prepared {
            Some(prepared) if prepared.content == session.read().snapshot().content_id() => {
                prepared
            }
            _ => Self::prepare_runtime_view(session.read().snapshot())?,
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
            tree_cache: Default::default(),
            authoring_capability_cache: Default::default(),
            #[cfg(test)]
            object_index_builds: std::sync::atomic::AtomicUsize::new(1),
            animation_catalog: std::sync::Mutex::new(None),
            canvas_topology: std::sync::Mutex::new(None),
            canvas_topology_source: std::sync::Mutex::new(None),
            previous_canvas_topology: std::sync::Mutex::new(None),
            previous_canvas_source: std::sync::Mutex::new(None),
            changed_canvas_meshes: std::sync::Mutex::new(None),
            selected: None,
            pending: None,
            manual_pending: None,
            manual_vertex_ids: Default::default(),
            generation_pending: None,
            generation_texture: None,
            next_property_edit: 0,
            textures,
            pristine_blank,
        })
    }

    fn refresh_textures(&mut self) -> Result<()> {
        self.textures.refresh(self.session.read().snapshot())
    }

    pub fn save(&mut self, path: &Path) -> Result<SaveReceipt> {
        self.finish_edit()?;
        let receipt = self.session.save_project(path, None)?;
        self.pristine_blank = None;
        Ok(receipt)
    }

    pub fn export_settings(&self) -> zipzi_core::ExportSettings {
        self.model().setup().export
    }
    pub fn model_setup(&self) -> zipzi_core::ModelSetup {
        self.model().setup()
    }

    pub fn source_canvas(&self) -> Option<zipzi_core::Canvas> {
        self.model().setup().source_canvas
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
        self.pending.is_some() || self.manual_pending.is_some() || self.generation_pending.is_some()
    }
    pub fn selected_id(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    fn mesh(&self, id: &str) -> Option<Mesh> {
        if let Some(pending) = &self.generation_pending {
            return pending
                .edit
                .snapshot()
                .snapshot
                .document()
                .mesh(id)
                .cloned();
        }
        if let Some(pending) = &self.manual_pending {
            return pending.edit.candidate_document().mesh(id).cloned();
        }
        match &self.pending {
            Some(pending) => pending.edit.candidate_document().mesh(id).cloned(),
            None => self.session.mesh(id),
        }
    }

    pub(crate) fn model(&self) -> crate::runtime_host::ModelRead {
        if let Some(pending) = &self.generation_pending {
            return crate::runtime_host::ModelRead(pending.edit.snapshot().snapshot.clone());
        }
        if let Some(pending) = &self.manual_pending {
            return pending.edit.candidate_document();
        }
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
        self.set_preview_parameters_at(values, None)
    }

    pub(crate) fn set_preview_parameters_at(
        &mut self,
        values: &[(String, f32)],
        model_revision: Option<u64>,
    ) -> Result<()> {
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
        if model_revision.is_none() {
            self.finish_edit()?;
        }
        if changed {
            self.session.set_preview_values_at(next, model_revision)?;
            self.preview_revision += 1;
        }
        Ok(())
    }

    /// Source-art metadata follows committed edits, never interactive pose previews.
    pub fn mesh_texture_revision(&self) -> u64 {
        self.session.version().revision
    }

    /// Static source-art crop and UV footprint; never evaluates posed geometry.
    pub fn mesh_texture_region(&self, id: &str) -> Option<crate::MeshTextureRegion> {
        let model_read_guard_55 = self.session.document();
        let mesh = model_read_guard_55.mesh(id)?;
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
    /// Structural scene queries can share results across pose and appearance candidates.
    pub fn evaluation_layout_id(&self) -> zipzi_core::state::ContentId {
        self.model().evaluation_layout_id()
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
        ensure!(self.model().parameter(id).is_some(), "Parameter not found");
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
        self.context.parameter_focus = None;
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
        let snapshot = if let Some(pending) = &self.generation_pending {
            pending.edit.snapshot().clone()
        } else if let Some(pending) = &self.manual_pending {
            pending.edit.read_snapshot().expect("active canvas Draft")
        } else if let Some(pending) = &self.pending {
            pending.edit.read_snapshot().expect("active canvas Draft")
        } else {
            self.session.sourced_snapshot()
        };
        let source = snapshot.source;
        let model = snapshot.snapshot.document();
        let layout = model.evaluation_layout_id();
        let mut current = self.canvas_topology.lock().unwrap();
        let mut cached_source = self.canvas_topology_source.lock().unwrap();
        if let Some(value) = current.as_mut() {
            // A source includes the candidate sequence as well as its base.
            // Recheck structural equality before sharing geometry from another
            // source; a local invalidation hint alone cannot prove equivalence.
            if !cached_source
                .is_some_and(|(cached, cached_layout)| cached == source || cached_layout == layout)
            {
                let next =
                    crate::canvas_topology::CanvasTopology::with_previous(model, Some(value), None);
                if !next.same_geometry(value) || !Arc::ptr_eq(&next.index, &value.index) {
                    *value = Arc::new(next);
                }
            }
            *cached_source = Some((source, layout));
            return value.clone();
        }
        let previous = self.previous_canvas_topology.lock().unwrap().take();
        let changed = self.changed_canvas_meshes.lock().unwrap().take();
        let previous_source = self.previous_canvas_source.lock().unwrap().take();
        let different_candidate = match (previous_source, source) {
            (
                Some(zipzi_sdk::QuerySource::Draft(previous)),
                zipzi_sdk::QuerySource::Draft(current),
            ) => previous.base != current.base || previous.draft != current.draft,
            _ => previous_source != Some(source),
        };
        let built = if previous.is_some() {
            crate::canvas_topology::CanvasTopology::with_previous(
                model,
                previous.as_deref(),
                if different_candidate {
                    None
                } else {
                    changed.as_ref()
                },
            )
        } else {
            crate::canvas_topology::CanvasTopology::new(model)
        };
        let built = Arc::new(built);
        *current = Some(built.clone());
        *cached_source = Some((source, layout));
        built
    }

    pub fn deformer_controls_visible(&self, id: &str) -> bool {
        self.object_state(&crate::ObjectRef::new(crate::ObjectKind::Deformer, id))
            .visible()
    }

    pub fn snapshot(&mut self) -> Result<SceneSnapshot> {
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("editor.snapshot");
        let frame = if let Some(pending) = &mut self.generation_pending {
            let document = pending.edit.snapshot().snapshot.document();
            pending
                .preview
                .replace(document, 0, self.session.preview_values().clone())?;
            pending.preview.frame(document, 0)?
        } else if let Some(pending) = &mut self.manual_pending {
            let document = pending.edit.candidate_document();
            pending
                .preview
                .replace(&document, 0, self.session.preview_values().clone())
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?;
            pending
                .preview
                .frame(&document, 0)
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?
        } else if let Some(pending) = &mut self.pending {
            let document = pending.edit.candidate_document();
            pending
                .preview
                .replace(&document, 0, self.session.preview_values().clone())
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?;
            pending
                .preview
                .frame(&document, 0)
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?
        } else {
            self.session.preview_frame()?
        };
        let viewport_frame = self.viewport_frame(&frame);
        let mut textures = self.textures.snapshot();
        if let Some(texture) = &self.generation_texture {
            let items = Arc::make_mut(&mut textures);
            if let Some(item) = items
                .iter_mut()
                .find(|item| item.asset_id == texture.asset_id)
            {
                *item = texture.clone();
            }
        }
        Ok(SceneSnapshot {
            model_opacity: 1.,
            runtime_frame: frame,
            viewport_frame,
            textures,
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
