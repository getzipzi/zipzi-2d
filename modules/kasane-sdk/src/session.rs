//! Authoring session lifecycle, project IO, reads, history and preview.
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::edit::EditSession;
use crate::types::*;
use kasane_animation::{ExpressionPreview, MotionPreview, PhysicsPreview};
use kasane_core::document::{
    DisplayInfo, ExpressionAsset, Model3Settings, MotionClip, MotionGroup, PackageAttachment,
    PhysicsAsset, PoseAsset, StructureIssue,
};
use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::preview::PreviewState;
use kasane_core::{
    evaluate_frame, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, Canvas,
    ChangeKind, Document, DrawableFrame, Glue, ImageAsset, Mesh, MeshBinding, Offscreen, Parameter,
    Part, PreviewValues, SceneBinding, Transform,
};
use kasane_project::{
    export_cdi3, export_expression3, export_motion3, export_physics3, export_pose3,
    CdiProjectError, ExpressionProjectError, MotionProjectError, PhysicsProjectError,
    PoseProjectError,
};
use kasane_project::{DocumentSession, FileSystem, NativeFileSystem};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

pub struct AuthoringSession {
    pub(crate) project: DocumentSession,
    pub(crate) preview: PreviewState,
    pub(crate) session_id: u64,
    pub(crate) generation: u64,
    pub(crate) done: VecDeque<HistoryEntry>,
    pub(crate) redo: Vec<HistoryEntry>,
    pub(crate) events: Vec<EditReceipt>,
    pub(crate) history_limits: HistoryLimits,
    pub(crate) binding_contexts: HashMap<String, (String, u64)>,
    pub(crate) authoring_limits: kasane_core::document::AuthoringLimits,
    pub(crate) incarnations: HashMap<ObjectKey, u64>,
    pub(crate) next_incarnation: u64,
    // Owner-dependent field addresses must not revive after a move and undo.
    pub(crate) motion_contexts: HashMap<String, (Option<String>, u64)>,
    pub(crate) document_epochs: HashMap<kasane_core::document::DocumentField, u64>,
    pub(crate) resource_loads:
        std::sync::Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>,
    pub(crate) resource_names: HashMap<String, (String, u64)>,
    pub(crate) attachment_content_epochs: HashMap<String, u64>,
    pub(crate) reference_epochs: HashMap<String, u64>,
    pub(crate) selection_epochs:
        HashMap<(String, kasane_core::document::ReferenceSelectionField), u64>,
    pub(crate) content_epochs: HashMap<(String, kasane_core::document::SceneContentDomain), u64>,
    pub(crate) scene_field_epochs: HashMap<(String, kasane_core::document::SceneField), u64>,
    pub(crate) organization_contexts: HashMap<String, (Option<String>, u64)>,
    pub(crate) pose_contexts: HashMap<String, (String, u64)>,
    pub(crate) physics_contexts: HashMap<String, (String, u64)>,
    pub(crate) expression_entry_contexts: HashMap<String, (String, u64)>,
}

/// Read-only document snapshot for bounded work outside an authoring lock.
/// The clone cost belongs to the capture step; later evaluation cannot observe edits.
#[derive(Debug, Clone)]
pub struct AuthoringSnapshot {
    document: Document,
    version: Version,
    project_path: Option<PathBuf>,
    clone_elapsed_ns: u64,
    memory_images: HashMap<String, kasane_project::PreparedImageContent>,
}

impl AuthoringSnapshot {
    /// Capture an independent curve candidate without publishing it or changing
    /// authoring preview values. The version remains the committed base version;
    /// hosts must pair it with their candidate edit ID/revision when accepting
    /// asynchronous results. Old snapshots retain their original curves.
    pub fn with_motion_candidate(
        &self,
        candidate: &crate::MotionClipCandidate,
    ) -> Result<Self, SdkError> {
        let started = std::time::Instant::now();
        crate::MotionClipCandidate::new(&self.document, candidate.clip().clone())?;
        let mut document = self.document.fork_candidate();
        let result = document.replace_motion(candidate.clip().clone());
        if !result.status.is_ok() {
            return Err(SdkError::from_status(
                result.status,
                "with_motion_candidate",
                vec![candidate.clip().id.clone()],
            ));
        }
        Ok(Self {
            document,
            version: self.version,
            memory_images: self.memory_images.clone(),
            project_path: self.project_path.clone(),
            clone_elapsed_ns: started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        })
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn memory_image(&self, id: &str) -> Option<&kasane_project::PreparedImageContent> {
        self.memory_images.get(id)
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn project_path(&self) -> Option<&std::path::Path> {
        self.project_path.as_deref()
    }

    pub fn clone_elapsed_ns(&self) -> u64 {
        self.clone_elapsed_ns
    }

    pub fn evaluate(&self, values: &PreviewValues) -> Result<DrawableFrame, SdkError> {
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(&self.document, values, &mut frame);
        if status.is_ok() {
            Ok(frame)
        } else {
            Err(SdkError::from_status(status, "evaluate", Vec::new()))
        }
    }
}

impl AuthoringSession {
    /// Clone the committed document and source path under the caller's lock.
    /// History, events, and mutable preview state are not copied or changed.
    pub fn read_snapshot(&self) -> AuthoringSnapshot {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("sdk.read_snapshot");
        let started = std::time::Instant::now();
        let document = self.project.document().clone();
        let memory_images = document
            .asset_order()
            .iter()
            .filter_map(|id| {
                self.project
                    .memory_image(document.get_asset(id)?)
                    .map(|image| (id.clone(), image))
            })
            .collect();
        AuthoringSnapshot {
            memory_images,
            document,
            version: self.version(),
            project_path: self.project_path().map(std::path::Path::to_path_buf),
            clone_elapsed_ns: started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64,
        }
    }
    /// The caller supplies a canonical, nonzero, lowercase document UUID.
    pub fn new(
        document_id: &str,
        canvas: impl Into<kasane_core::ModelSetup>,
    ) -> Result<Self, SdkError> {
        Self::with_history_limits(document_id, canvas, HistoryLimits::default())
    }

    pub fn with_history_limits(
        document_id: &str,
        canvas: impl Into<kasane_core::ModelSetup>,
        history_limits: HistoryLimits,
    ) -> Result<Self, SdkError> {
        Self::with_options(
            document_id,
            canvas,
            history_limits,
            Arc::new(NativeFileSystem),
        )
    }

    /// Use a custom publication backend, primarily for deterministic IO tests.
    pub fn with_filesystem(
        document_id: &str,
        canvas: impl Into<kasane_core::ModelSetup>,
        filesystem: Arc<dyn FileSystem>,
    ) -> Result<Self, SdkError> {
        Self::with_options(document_id, canvas, HistoryLimits::default(), filesystem)
    }

    fn with_options(
        document_id: &str,
        canvas: impl Into<kasane_core::ModelSetup>,
        history_limits: HistoryLimits,
        filesystem: Arc<dyn FileSystem>,
    ) -> Result<Self, SdkError> {
        let mut document = Document::new();
        let status = document.initialize(document_id, canvas);
        if !status.is_ok() {
            return Err(SdkError::from_status(
                status,
                "new",
                vec![document_id.into()],
            ));
        }
        let incarnations: HashMap<_, _> = keys_in(&document)
            .into_iter()
            .enumerate()
            .map(|(slot, key)| (key, slot as u64 + 1))
            .collect();
        let next_incarnation = incarnations.len() as u64 + 1;
        Ok(Self {
            project: DocumentSession::from_authoring_document_with_filesystem(document, filesystem),
            preview: PreviewState::default(),
            session_id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            generation: 1,
            done: VecDeque::new(),
            redo: Vec::new(),
            events: Vec::new(),
            history_limits,
            authoring_limits: Default::default(),
            binding_contexts: HashMap::new(),
            incarnations,
            physics_contexts: HashMap::new(),
            pose_contexts: HashMap::new(),
            reference_epochs: HashMap::new(),
            selection_epochs: HashMap::new(),
            content_epochs: HashMap::new(),
            scene_field_epochs: HashMap::new(),
            organization_contexts: HashMap::new(),
            expression_entry_contexts: HashMap::new(),
            motion_contexts: HashMap::new(),
            attachment_content_epochs: HashMap::new(),
            document_epochs: HashMap::new(),
            resource_loads: Default::default(),
            resource_names: HashMap::new(),
            next_incarnation,
        })
    }

    pub fn version(&self) -> Version {
        Version {
            session_id: self.session_id,
            generation: self.generation,
            revision: self.project.document().revision(),
        }
    }
    pub fn evaluation_revision(&self) -> u64 {
        self.project.document().evaluation_revision()
    }
    pub fn export_settings(&self) -> kasane_core::ExportSettings {
        self.project.document().export_settings()
    }
    pub fn source_canvas(&self) -> Option<Canvas> {
        self.project.document().source_canvas()
    }
    pub fn model_setup(&self) -> kasane_core::ModelSetup {
        self.project.document().model_setup()
    }
    pub fn draw_order_groups(&self) -> Option<Vec<DrawOrderGroup>> {
        self.project
            .document()
            .draw_order_groups()
            .map(<[_]>::to_vec)
    }
    pub fn modified(&self) -> bool {
        self.project.document().modified()
    }
    pub fn asset_ids(&self) -> &[String] {
        self.project.document().asset_order()
    }
    pub fn mesh_ids(&self) -> &[String] {
        self.project.document().mesh_order()
    }
    pub fn parameter_ids(&self) -> &[String] {
        self.project.document().parameter_order()
    }
    pub fn binding_ids(&self) -> &[String] {
        self.project.document().binding_order()
    }
    pub fn part_ids(&self) -> &[String] {
        self.project.document().part_order()
    }
    pub fn transform_ids(&self) -> &[String] {
        self.project.document().transform_order()
    }
    pub fn scene_binding_ids(&self) -> &[String] {
        self.project.document().scene_binding_order()
    }
    pub fn blend_key_table_ids(&self) -> &[String] {
        self.project.document().blend_key_table_order()
    }
    pub fn blend_constraint_ids(&self) -> &[String] {
        self.project.document().blend_constraint_order()
    }
    pub fn blend_binding_ids(&self) -> &[String] {
        self.project.document().blend_binding_order()
    }
    pub fn glue_ids(&self) -> &[String] {
        self.project.document().glue_order()
    }
    pub fn offscreen_ids(&self) -> &[String] {
        self.project.document().offscreen_order()
    }
    pub fn display_info(&self) -> DisplayInfo {
        self.project.document().display_info().clone()
    }
    pub fn expression_ids(&self) -> &[String] {
        self.project.document().expression_order()
    }
    pub fn expression(&self, id: &str) -> Option<ExpressionAsset> {
        self.project.document().get_expression(id).cloned()
    }
    pub fn motion_ids(&self) -> &[String] {
        self.project.document().motion_order()
    }
    pub fn motion(&self, id: &str) -> Option<MotionClip> {
        self.project.document().get_motion(id).cloned()
    }
    pub fn motion_groups(&self) -> Vec<MotionGroup> {
        self.project.document().motion_groups().to_vec()
    }
    pub fn export_motion3(&self, id: &str) -> Result<String, SdkError> {
        export_motion3(self.project.document(), id).map_err(|error: MotionProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_motion3");
            result.field_path = Some(error.path.into());
            result.object_ids.push(id.into());
            result
        })
    }
    /// Capture an isolated expression runtime with deterministic seek/replay.
    pub fn expression_preview(&self) -> ExpressionPreview {
        ExpressionPreview::new(self.project.document())
    }
    pub fn motion_preview(&self) -> MotionPreview {
        let mut preview = MotionPreview::new(self.project.document());
        preview.bind_session_identity(self.session_id, self.generation);
        preview
    }
    pub fn physics_preview(&self) -> PhysicsPreview {
        PhysicsPreview::new(self.project.document())
    }
    pub fn pose(&self) -> Option<PoseAsset> {
        self.project.document().pose().cloned()
    }
    pub fn export_pose3(&self) -> Result<Option<String>, SdkError> {
        export_pose3(self.project.document()).map_err(|error: PoseProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_pose3");
            result.field_path = Some(error.path.into());
            result
        })
    }
    pub fn physics(&self) -> Option<PhysicsAsset> {
        self.project.document().physics().cloned()
    }
    pub fn missing_attachments(&self) -> Vec<String> {
        self.project.document().missing_attachments().to_vec()
    }
    pub fn model3_settings(&self) -> Model3Settings {
        self.project.document().model3_settings().clone()
    }
    pub fn package_attachments(&self) -> Vec<PackageAttachment> {
        self.project.document().package_attachments().to_vec()
    }
    pub fn export_physics3(&self) -> Result<Option<String>, SdkError> {
        export_physics3(self.project.document()).map_err(|error: PhysicsProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_physics3");
            result.field_path = Some(error.path.into());
            result
        })
    }
    pub fn export_expression3(&self, id: &str) -> Result<String, SdkError> {
        export_expression3(self.project.document(), id).map_err(|error: ExpressionProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_expression3");
            result.field_path = Some(error.path.into());
            result.object_ids.push(id.into());
            result
        })
    }
    pub fn export_cdi3(&self) -> Result<String, SdkError> {
        export_cdi3(self.project.document()).map_err(|error: CdiProjectError| {
            let mut result = SdkError::new(&error.code, &error.message, "export_cdi3");
            result.field_path = Some(error.path.into());
            result
        })
    }
    pub fn asset(&self, id: &str) -> Option<ImageAsset> {
        self.project.document().get_asset(id).cloned()
    }
    pub fn mesh(&self, id: &str) -> Option<Mesh> {
        self.project.document().get_mesh(id).cloned()
    }

    /// Borrow model data for synchronous inspection without cloning geometry or
    /// keyforms. Mutation continues to go through an EditSession.
    pub fn document(&self) -> &Document {
        self.project.document()
    }
    pub fn parameter(&self, id: &str) -> Option<Parameter> {
        self.project.document().get_parameter(id).cloned()
    }
    pub fn binding(&self, id: &str) -> Option<MeshBinding> {
        self.project.document().get_binding(id).cloned()
    }
    pub fn binding_for_mesh(&self, mesh_id: &str) -> Option<MeshBinding> {
        self.project.document().binding_for_mesh(mesh_id).cloned()
    }
    pub fn part(&self, id: &str) -> Option<Part> {
        self.project.document().get_part(id).cloned()
    }
    pub fn transform(&self, id: &str) -> Option<Transform> {
        self.project.document().get_transform(id).cloned()
    }
    pub fn scene_binding(&self, id: &str) -> Option<SceneBinding> {
        self.project.document().get_scene_binding(id).cloned()
    }
    pub fn binding_for_scene(&self, target_id: &str) -> Option<SceneBinding> {
        self.project
            .document()
            .binding_for_scene(target_id)
            .cloned()
    }
    pub fn references_to(&self, id: &str) -> Vec<String> {
        self.project.document().references_to(id)
    }
    pub fn validate_structure(&self) -> Vec<StructureIssue> {
        self.project.document().validate_structure()
    }
    pub fn blend_key_table(&self, id: &str) -> Option<BlendShapeKeyTable> {
        self.project.document().get_blend_key_table(id).cloned()
    }
    pub fn blend_constraint(&self, id: &str) -> Option<BlendShapeConstraint> {
        self.project.document().get_blend_constraint(id).cloned()
    }
    pub fn blend_binding(&self, id: &str) -> Option<BlendShapeBinding> {
        self.project.document().get_blend_binding(id).cloned()
    }
    pub fn glue(&self, id: &str) -> Option<Glue> {
        self.project.document().get_glue(id).cloned()
    }
    pub fn offscreen(&self, id: &str) -> Option<Offscreen> {
        self.project.document().get_offscreen(id).cloned()
    }
    pub fn handle(&self, kind: ObjectKind, id: &str) -> Result<ObjectHandle, SdkError> {
        let key = ObjectKey {
            kind,
            id: id.into(),
        };
        if !self.object_exists(kind, id) {
            let mut error = SdkError::new("NOT_FOUND", "Object does not exist", "handle");
            error.object_ids.push(id.into());
            return Err(error);
        }
        let incarnation = self
            .incarnations
            .get(&key)
            .copied()
            .expect("committed object has incarnation");
        Ok(ObjectHandle {
            session_id: self.session_id,
            generation: self.generation,
            object_id: id.into(),
            incarnation,
            kind,
        })
    }
    /// Resolve captured identities before inspecting domain-specific epochs or values.
    /// A reused UUID must report a stale handle even if its final fields match.
    pub(crate) fn validate_field_address<'a>(
        &self,
        handles: impl IntoIterator<Item = &'a ObjectHandle>,
        is_stale: impl FnOnce() -> bool,
        message: &str,
        operation: &'static str,
    ) -> Result<(), SdkError> {
        for handle in handles {
            self.resolve_handle(handle)?;
        }
        if is_stale() {
            return Err(SdkError::new("STALE_FIELD_ADDRESS", message, operation));
        }
        Ok(())
    }

    pub fn resolve_handle(&self, handle: &ObjectHandle) -> Result<(), SdkError> {
        let key = ObjectKey {
            kind: handle.kind,
            id: handle.object_id.clone(),
        };
        if handle.session_id != self.session_id
            || handle.generation != self.generation
            || self.incarnations.get(&key) != Some(&handle.incarnation)
            || !self.object_exists(handle.kind, handle.id())
        {
            let mut error = SdkError::new(
                "STALE_HANDLE",
                "Object handle is no longer valid",
                "resolve_handle",
            );
            error.object_ids.push(handle.object_id.clone());
            return Err(error);
        }
        Ok(())
    }
    pub fn mesh_by_handle(&self, handle: &ObjectHandle) -> Result<Mesh, SdkError> {
        self.resolve_handle(handle)?;
        if handle.kind != ObjectKind::Mesh {
            return Err(SdkError::new(
                "WRONG_OBJECT_KIND",
                "Handle is not a mesh",
                "mesh_by_handle",
            ));
        }
        Ok(self
            .mesh(handle.id())
            .expect("validated mesh handle resolves"))
    }
    pub fn find_meshes_by_name(&self, name: &str) -> Vec<Mesh> {
        self.mesh_ids()
            .iter()
            .filter_map(|id| self.project.document().get_mesh(id))
            .filter(|mesh| mesh.name == name)
            .cloned()
            .collect()
    }
    pub fn require_unique_mesh(&self, name: &str) -> Result<Mesh, SdkError> {
        let mut matches = self
            .mesh_ids()
            .iter()
            .filter_map(|id| self.project.document().get_mesh(id))
            .filter(|mesh| mesh.name == name);
        let Some(first) = matches.next() else {
            return Err(SdkError::new(
                "NOT_FOUND",
                "No mesh has this name",
                "require_unique_mesh",
            ));
        };
        if matches.next().is_some() {
            Err(SdkError::new(
                "AMBIGUOUS_NAME",
                "Several meshes have this name",
                "require_unique_mesh",
            ))
        } else {
            Ok(first.clone())
        }
    }
    pub fn geometry(&self, id: &str) -> Option<GeometrySnapshot> {
        let mesh = self.project.document().get_mesh(id)?;
        let space = if mesh.deformer_id.is_empty() {
            GeometrySpace::Model
        } else if self
            .project
            .document()
            .get_transform(&mesh.deformer_id)?
            .warp()
            .is_some()
        {
            GeometrySpace::WarpParameter(mesh.deformer_id.clone())
        } else {
            GeometrySpace::RotationLocal(mesh.deformer_id.clone())
        };
        Some(GeometrySnapshot {
            version: self.version(),
            mesh_id: id.into(),
            vertex_ids: mesh.vertex_ids.clone(),
            positions: mesh.base_positions.clone(),
            uvs: mesh.uvs.clone(),
            triangles: mesh.triangles.clone(),
            space,
        })
    }
    pub fn history_lengths(&self) -> (usize, usize) {
        (self.done.len(), self.redo.len())
    }
    pub fn history_state(&self) -> HistoryState {
        HistoryState {
            undo_steps: self.done.len(),
            redo_steps: self.redo.len(),
            estimated_bytes: self
                .done
                .iter()
                .chain(self.redo.iter())
                .map(HistoryEntry::estimated_bytes)
                .sum(),
            max_steps: self.history_limits.max_steps,
            max_bytes: self.history_limits.max_bytes,
        }
    }
    pub fn estimated_content_bytes(&self) -> usize {
        self.project.document().estimated_content_bytes()
    }
    pub fn drain_events(&mut self) -> Vec<EditReceipt> {
        std::mem::take(&mut self.events)
    }

    pub fn begin_edit(
        &mut self,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> Result<EditSession<'_>, SdkError> {
        let mut edit = self.begin_owned_edit(label, expected)?;
        edit.session = Some(self);
        Ok(edit)
    }

    /// Create a persistent candidate workspace. `commit_to` checks the source
    /// version again before publishing, so callers can hold it across API calls.
    pub fn begin_owned_edit(
        &self,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> Result<EditSession<'static>, SdkError> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("sdk.begin_owned_edit");
        let version = self.version();
        if let Some(value) = expected.filter(|value| *value != version) {
            let mut error =
                SdkError::new("STALE_VERSION", "Document version changed", "begin_edit");
            error.expected_version = Some(Box::new(value));
            error.actual_version = Some(Box::new(version));
            return Err(error);
        }
        let candidate = self.project.document().fork_candidate();
        if candidate.estimated_content_bytes() > self.history_limits.max_bytes {
            return Err(SdkError::new(
                "HISTORY_LIMIT_EXCEEDED",
                "Candidate exceeds the history memory budget",
                "begin_edit",
            ));
        }
        Ok(EditSession {
            session: None,
            base_keys: self.object_keys(),
            candidate: Some(candidate),
            label: label.into(),
            before: version,
            aborted: false,
            kind: ChangeKind::None,
            categories: Default::default(),
            object_ids: Vec::new(),
            erased_keys: HashSet::new(),
            authoring_limits: self.authoring_limits,
            resource_load_controls: vec![],
            binding_controls: vec![],
            binding_cost: None,
        })
    }

    pub fn edit<T>(
        &mut self,
        label: impl Into<String>,
        expected: Option<Version>,
        operation: impl FnOnce(&mut EditSession<'_>) -> Result<T, SdkError>,
    ) -> Result<(T, EditReceipt), SdkError> {
        let mut edit = self.begin_edit(label, expected)?;
        let value = operation(&mut edit)?;
        let receipt = edit.commit()?;
        Ok((value, receipt))
    }

    pub fn undo(&mut self) -> Result<EditReceipt, SdkError> {
        let before = self.version();
        let before_keys = self.object_keys();
        let entry = self
            .done
            .back_mut()
            .ok_or_else(|| SdkError::new("NOTHING_TO_UNDO", "History is empty", "undo"))?;
        let evaluation_before = self.project.document().evaluation_revision();
        self.project
            .swap_authoring_checkpoint(&mut entry.checkpoint)
            .map_err(|status| SdkError::from_status(status, "undo", Vec::new()))?;
        self.preview.retain_parameters(self.project.document());
        if self.project.document().evaluation_revision() != evaluation_before {
            self.preview.invalidate();
        }
        let entry = self.done.pop_back().expect("history entry still exists");
        self.refresh_incarnations(&before_keys, &entry.identity_keys);
        let receipt = EditReceipt {
            label: entry.label.clone(),
            before,
            after: self.version(),
            kind: entry.kind,
            object_ids: entry.object_ids.clone(),
            changed: true,
            changes: entry.changes.inverted(),
        };
        self.redo.push(entry);
        self.publish_receipt(receipt.clone());
        Ok(receipt)
    }

    pub fn redo(&mut self) -> Result<EditReceipt, SdkError> {
        let before = self.version();
        let before_keys = self.object_keys();
        let entry = self
            .redo
            .last_mut()
            .ok_or_else(|| SdkError::new("NOTHING_TO_REDO", "History is empty", "redo"))?;
        let evaluation_before = self.project.document().evaluation_revision();
        self.project
            .swap_authoring_checkpoint(&mut entry.checkpoint)
            .map_err(|status| SdkError::from_status(status, "redo", Vec::new()))?;
        self.preview.retain_parameters(self.project.document());
        if self.project.document().evaluation_revision() != evaluation_before {
            self.preview.invalidate();
        }
        let entry = self.redo.pop().expect("history entry still exists");
        self.refresh_incarnations(&before_keys, &entry.identity_keys);
        let receipt = EditReceipt {
            label: entry.label.clone(),
            before,
            after: self.version(),
            kind: entry.kind,
            object_ids: entry.object_ids.clone(),
            changed: true,
            changes: entry.changes.clone(),
        };
        self.done.push_back(entry);
        self.publish_receipt(receipt.clone());
        Ok(receipt)
    }

    /// Evaluates explicit values without changing session preview state.
    pub fn evaluate(&self, values: &PreviewValues) -> Result<DrawableFrame, SdkError> {
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(self.project.document(), values, &mut frame);
        if status.is_ok() {
            Ok(frame)
        } else {
            Err(SdkError::from_status(status, "evaluate", Vec::new()))
        }
    }

    pub fn preview_values(&self) -> &PreviewValues {
        self.preview.values()
    }
    pub fn preview_revision(&self) -> u64 {
        self.preview.revision()
    }
    pub fn preview_evaluation_count(&self) -> u64 {
        self.preview.evaluation_count()
    }
    pub fn preview_frame(&mut self) -> Result<Arc<DrawableFrame>, SdkError> {
        self.preview
            .frame(self.project.document(), self.generation)
            .map_err(|status| SdkError::from_status(status, "preview_frame", Vec::new()))
    }
    pub fn set_preview_values(&mut self, values: PreviewValues) -> Result<bool, SdkError> {
        self.preview
            .replace(self.project.document(), self.generation, values)
            .map_err(|status| SdkError::from_status(status, "set_preview_values", Vec::new()))
    }
    pub fn set_preview_parameter(&mut self, id: &str, value: f32) -> Result<bool, SdkError> {
        let mut values = self.preview.values().clone();
        values.insert(id.into(), value);
        self.preview
            .replace(self.project.document(), self.generation, values)
            .map_err(|status| {
                SdkError::from_status(status, "set_preview_parameter", vec![id.into()])
            })
    }
    pub fn reset_preview_values(&mut self) -> Result<bool, SdkError> {
        self.set_preview_values(PreviewValues::new())
    }

    pub(crate) fn object_keys(&self) -> HashSet<ObjectKey> {
        keys_in(self.document())
    }

    fn object_exists(&self, kind: ObjectKind, id: &str) -> bool {
        object_exists_in(self.project.document(), kind, id)
    }

    /// Every publication, including history and replacement, advances operation
    /// validity from the same authoritative delta consumed by external clients.
    pub(crate) fn publish_receipt(&mut self, receipt: EditReceipt) {
        let document = self.project.document();
        let changes = &receipt.changes;
        if changes.full_invalidation {
            for (_, flag) in self.resource_loads.lock().unwrap().drain() {
                flag.store(true, Ordering::Release);
            }
            self.resource_names.clear();
            self.document_epochs.clear();
            self.attachment_content_epochs.clear();
            self.motion_contexts.clear();
            self.physics_contexts.clear();
            self.pose_contexts.clear();
            self.organization_contexts.clear();
            self.scene_field_epochs.clear();
            self.content_epochs.clear();
            self.reference_epochs.clear();
            self.selection_epochs.clear();
            if let Some(pose) = document.pose() {
                for id in pose.entity_ids() {
                    if let Some(owner) = document.pose_owner(id) {
                        self.pose_contexts
                            .insert(id.into(), (owner.into(), document.revision()));
                    }
                }
            }
            if let Some(asset) = document.physics() {
                for group in &asset.data.settings {
                    self.physics_contexts
                        .insert(group.id.clone(), (asset.id.clone(), document.revision()));
                    for id in group.entity_ids().skip(1) {
                        self.physics_contexts
                            .insert(id.into(), (group.id.clone(), document.revision()));
                    }
                }
            }
            self.expression_entry_contexts.clear();
            for owner in document
                .expression_order()
                .iter()
                .filter_map(|id| document.get_expression(id))
            {
                for entry in &owner.entries {
                    self.expression_entry_contexts
                        .insert(entry.id.clone(), (owner.id.clone(), document.revision()));
                }
            }
        } else {
            let ids: std::collections::BTreeSet<_> = changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .filter(|entity| entity.kind == ObjectKind::ExpressionEntry)
                .map(|entity| &entity.id)
                .collect();
            for id in ids {
                if let Some((owner, _)) = document.get_expression_entry(id) {
                    if self
                        .expression_entry_contexts
                        .get(id)
                        .is_none_or(|context| context.0 != owner.id)
                    {
                        self.expression_entry_contexts
                            .insert(id.clone(), (owner.id.clone(), document.revision()));
                    }
                } else {
                    self.expression_entry_contexts.remove(id);
                }
            }
        }
        for entity in changes
            .created
            .iter()
            .chain(&changes.deleted)
            .chain(&changes.modified)
        {
            let owner =
                match entity.kind {
                    ObjectKind::MotionRegistration => document
                        .get_motion_registration(&entity.id)
                        .map(|(owner, _)| match owner {
                            kasane_core::document::MotionContainer::Root => None,
                            kasane_core::document::MotionContainer::Group(id) => Some(id),
                        }),
                    ObjectKind::MotionTrack => document
                        .get_motion_track(&entity.id)
                        .map(|(c, _)| Some(c.id.clone())),
                    ObjectKind::MotionKey => document
                        .get_motion_key(&entity.id)
                        .map(|(_, t, _)| Some(t.id.clone())),
                    ObjectKind::MotionEvent => document
                        .get_motion_event(&entity.id)
                        .map(|(c, _)| Some(c.id.clone())),
                    _ => continue,
                };
            if let Some(owner) = owner {
                if self
                    .motion_contexts
                    .get(&entity.id)
                    .is_none_or(|c| c.0 != owner)
                {
                    self.motion_contexts
                        .insert(entity.id.clone(), (owner, document.revision()));
                }
            } else {
                self.motion_contexts.remove(&entity.id);
            }
        }
        for entity in changes
            .created
            .iter()
            .chain(&changes.deleted)
            .chain(&changes.modified)
        {
            if !matches!(
                entity.kind,
                ObjectKind::PhysicsGroup
                    | ObjectKind::PhysicsInput
                    | ObjectKind::PhysicsOutput
                    | ObjectKind::PhysicsNode
            ) {
                continue;
            }
            if let Some(owner) = document.physics_owner(&entity.id) {
                if self
                    .physics_contexts
                    .get(&entity.id)
                    .is_none_or(|v| v.0 != owner)
                {
                    self.physics_contexts
                        .insert(entity.id.clone(), (owner.into(), document.revision()));
                }
            } else {
                self.physics_contexts.remove(&entity.id);
            }
        }
        for entity in changes
            .created
            .iter()
            .chain(&changes.deleted)
            .chain(&changes.modified)
        {
            if !matches!(
                entity.kind,
                ObjectKind::PoseGroup | ObjectKind::PoseEntry | ObjectKind::PoseLink
            ) {
                continue;
            }
            if let Some(owner) = document.pose_owner(&entity.id) {
                if self
                    .pose_contexts
                    .get(&entity.id)
                    .is_none_or(|v| v.0 != owner)
                {
                    self.pose_contexts
                        .insert(entity.id.clone(), (owner.into(), document.revision()));
                }
            } else {
                self.pose_contexts.remove(&entity.id);
            }
        }
        for entity in changes
            .created
            .iter()
            .chain(&changes.deleted)
            .chain(&changes.modified)
        {
            if !matches!(
                entity.kind,
                ObjectKind::Parameter
                    | ObjectKind::CdiParameterGroup
                    | ObjectKind::Part
                    | ObjectKind::Mesh
                    | ObjectKind::Transform
                    | ObjectKind::Offscreen
            ) {
                continue;
            }
            if crate::organization_authoring::kind(document, &entity.id).is_some() {
                let owner = document.organization_owner(&entity.id).map(str::to_owned);
                if self
                    .organization_contexts
                    .get(&entity.id)
                    .is_none_or(|v| v.0 != owner)
                {
                    self.organization_contexts
                        .insert(entity.id.clone(), (owner, document.revision()));
                }
            } else {
                self.organization_contexts.remove(&entity.id);
            }
        }
        for field in &changes.document_fields {
            self.document_epochs.insert(*field, document.revision());
        }
        for change in &changes.reference_fields {
            self.reference_epochs
                .insert(change.entity.id.clone(), document.revision());
        }
        for change in &changes.reference_selections {
            self.selection_epochs.insert(
                (change.entity.id.clone(), change.field),
                document.revision(),
            );
        }
        for change in &changes.content_changes {
            self.content_epochs.insert(
                (change.entity.id.clone(), change.domain.clone()),
                document.revision(),
            );
        }
        for field in &changes.scene_fields {
            self.scene_field_epochs
                .insert((field.entity.id.clone(), field.field), document.revision());
        }
        for entity in changes.created.iter().chain(&changes.modified) {
            let name = match entity.kind {
                ObjectKind::Asset => document.get_asset(&entity.id).map(|a| a.name.as_str()),
                ObjectKind::Attachment => {
                    document.get_attachment(&entity.id).map(|a| a.path.as_str())
                }
                _ => None,
            };
            if let Some(name) = name {
                if self
                    .resource_names
                    .get(&entity.id)
                    .is_none_or(|v| v.0 != name)
                {
                    self.resource_names
                        .insert(entity.id.clone(), (name.into(), document.revision()));
                }
            }
        }
        for entity in &changes.deleted {
            if matches!(entity.kind, ObjectKind::Asset | ObjectKind::Attachment) {
                self.resource_names.remove(&entity.id);
                if let Some(flag) = self.resource_loads.lock().unwrap().remove(&entity.id) {
                    flag.store(true, Ordering::Release);
                }
            }
        }
        for entity in &changes.resource_content {
            let epoch = self
                .attachment_content_epochs
                .entry(entity.id.clone())
                .or_default();
            *epoch = epoch.checked_add(1).expect("content epoch exhausted");
        }
        if changes.full_invalidation {
            self.binding_contexts.clear();
        }
        let mut owners: std::collections::BTreeSet<String> =
            receipt.object_ids.iter().cloned().collect();
        if changes.full_invalidation
            || changes
                .modified
                .iter()
                .any(|e| e.kind == ObjectKind::Parameter)
                && changes
                    .categories
                    .contains(&crate::ChangeCategory::Topology)
        {
            owners.extend(document.binding_identities().into_iter().map(|i| i.owner));
        }
        for owner in owners {
            if let Some(signature) = document.binding_context_signature(&owner) {
                if self
                    .binding_contexts
                    .get(&owner)
                    .is_none_or(|c| c.0 != signature)
                {
                    self.binding_contexts
                        .insert(owner, (signature, document.revision()));
                }
            } else {
                self.binding_contexts.remove(&owner);
            }
        }
        self.events.push(receipt);
    }

    pub(crate) fn refresh_incarnations(
        &mut self,
        before: &HashSet<ObjectKey>,
        identity_keys: &HashSet<ObjectKey>,
    ) {
        let after = self.object_keys();
        self.refresh_incarnations_from_keys(before, &after, identity_keys);
    }

    pub(crate) fn refresh_incarnations_from_keys(
        &mut self,
        before: &HashSet<ObjectKey>,
        after: &HashSet<ObjectKey>,
        identity_keys: &HashSet<ObjectKey>,
    ) {
        let mut changed: HashSet<_> = before.symmetric_difference(after).cloned().collect();
        changed.extend(
            identity_keys
                .iter()
                .filter(|key| after.contains(*key))
                .cloned(),
        );
        for key in changed {
            self.incarnations.insert(key, self.next_incarnation);
            self.next_incarnation = self
                .next_incarnation
                .checked_add(1)
                .expect("incarnation counter exhausted");
        }
    }
}

pub(crate) fn object_exists_in(doc: &Document, kind: ObjectKind, id: &str) -> bool {
    match kind {
        ObjectKind::BindingAxis => doc
            .binding_identity(id)
            .is_some_and(|i| i.kind == kasane_core::document::BindingIdentityKind::Axis),
        ObjectKind::BindingSample => doc
            .binding_identity(id)
            .is_some_and(|i| i.kind == kasane_core::document::BindingIdentityKind::Sample),
        ObjectKind::Asset => doc.get_asset(id).is_some(),
        ObjectKind::Mesh => doc.get_mesh(id).is_some(),
        ObjectKind::Parameter => doc.get_parameter(id).is_some(),
        ObjectKind::MeshBinding => doc.get_binding(id).is_some(),
        ObjectKind::Part => doc.get_part(id).is_some(),
        ObjectKind::Transform => doc.get_transform(id).is_some(),
        ObjectKind::SceneBinding => doc.get_scene_binding(id).is_some(),
        ObjectKind::BlendKeyTable => doc.get_blend_key_table(id).is_some(),
        ObjectKind::BlendConstraint => doc.get_blend_constraint(id).is_some(),
        ObjectKind::BlendBinding => doc.get_blend_binding(id).is_some(),
        ObjectKind::Glue => doc.get_glue(id).is_some(),
        ObjectKind::GlueVertexPair => doc.get_glue_pair(id).is_some(),
        ObjectKind::Offscreen => doc.get_offscreen(id).is_some(),
        ObjectKind::DrawOrderGroup => doc.drawing_group_owners().iter().any(|owner| {
            if owner.is_empty() {
                doc.id() == id
            } else {
                owner == id
            }
        }),
        ObjectKind::CdiParameterGroup => doc
            .display_info()
            .parameter_groups
            .as_ref()
            .is_some_and(|groups| groups.iter().any(|group| group.id == id)),
        ObjectKind::ModelParameterGroup
        | ObjectKind::ModelParameterMember
        | ObjectKind::ModelHitArea
        | ObjectKind::CdiCombinedMember => crate::reference_authoring::kind(doc, id) == Some(kind),
        ObjectKind::CdiCombinedSet => doc
            .display_info()
            .combined_parameters
            .as_ref()
            .is_some_and(|sets| sets.iter().any(|set| set.id == id)),
        ObjectKind::Expression => doc.get_expression(id).is_some(),
        ObjectKind::ExpressionEntry => doc.get_expression_entry(id).is_some(),
        ObjectKind::Motion => doc.get_motion(id).is_some(),
        ObjectKind::MotionGroup => doc.get_motion_group(id).is_some(),
        ObjectKind::MotionRegistration => doc.get_motion_registration(id).is_some(),
        ObjectKind::MotionTrack => doc.get_motion_track(id).is_some(),
        ObjectKind::MotionKey => doc.get_motion_key(id).is_some(),
        ObjectKind::MotionEvent => doc.get_motion_event(id).is_some(),
        ObjectKind::Attachment => doc.get_attachment(id).is_some(),
        ObjectKind::Pose => doc.pose().is_some_and(|pose| pose.id == id),
        ObjectKind::PoseGroup => doc.get_pose_group(id).is_some(),
        ObjectKind::PoseEntry => doc.get_pose_entry(id).is_some(),
        ObjectKind::PoseLink => doc.get_pose_link(id).is_some(),
        ObjectKind::PhysicsGroup => doc.get_physics_group(id).is_some(),
        ObjectKind::PhysicsInput => doc.get_physics_input(id).is_some(),
        ObjectKind::PhysicsOutput => doc.get_physics_output(id).is_some(),
        ObjectKind::PhysicsNode => doc.get_physics_node(id).is_some(),
        ObjectKind::Physics => doc.physics().is_some_and(|physics| physics.id == id),
    }
}

pub(crate) fn keys_in(doc: &Document) -> HashSet<ObjectKey> {
    let mut keys: HashSet<_> = [
        (ObjectKind::Asset, doc.asset_order()),
        (ObjectKind::Mesh, doc.mesh_order()),
        (ObjectKind::Parameter, doc.parameter_order()),
        (ObjectKind::MeshBinding, doc.binding_order()),
        (ObjectKind::Part, doc.part_order()),
        (ObjectKind::Transform, doc.transform_order()),
        (ObjectKind::SceneBinding, doc.scene_binding_order()),
        (ObjectKind::BlendKeyTable, doc.blend_key_table_order()),
        (ObjectKind::BlendConstraint, doc.blend_constraint_order()),
        (ObjectKind::BlendBinding, doc.blend_binding_order()),
        (ObjectKind::Glue, doc.glue_order()),
        (ObjectKind::Offscreen, doc.offscreen_order()),
    ]
    .into_iter()
    .flat_map(|(kind, ids)| {
        ids.iter().map(move |id| ObjectKey {
            kind,
            id: id.clone(),
        })
    })
    .collect();
    keys.extend(
        doc.glue_order()
            .iter()
            .filter_map(|id| doc.get_glue(id))
            .flat_map(|g| {
                g.pairs.iter().map(|p| ObjectKey {
                    kind: ObjectKind::GlueVertexPair,
                    id: p.id.clone(),
                })
            }),
    );
    keys.extend(doc.reference_identities().iter().map(|i| ObjectKey {
        kind: crate::reference_authoring::kind(doc, &i.id).unwrap(),
        id: i.id.clone(),
    }));
    keys.extend(
        doc.drawing_group_owners()
            .into_iter()
            .map(|owner| ObjectKey {
                kind: ObjectKind::DrawOrderGroup,
                id: if owner.is_empty() {
                    doc.id().into()
                } else {
                    owner
                },
            }),
    );
    if let Some(groups) = &doc.display_info().parameter_groups {
        keys.extend(groups.iter().map(|group| ObjectKey {
            kind: ObjectKind::CdiParameterGroup,
            id: group.id.clone(),
        }));
    }
    if let Some(sets) = &doc.display_info().combined_parameters {
        keys.extend(sets.iter().map(|set| ObjectKey {
            kind: ObjectKind::CdiCombinedSet,
            id: set.id.clone(),
        }));
    }
    keys.extend(doc.expression_order().iter().map(|id| ObjectKey {
        kind: ObjectKind::Expression,
        id: id.clone(),
    }));
    keys.extend(
        doc.expression_order()
            .iter()
            .flat_map(|id| &doc.get_expression(id).unwrap().entries)
            .map(|entry| ObjectKey {
                kind: ObjectKind::ExpressionEntry,
                id: entry.id.clone(),
            }),
    );
    keys.extend(doc.motion_order().iter().map(|id| ObjectKey {
        kind: ObjectKind::Motion,
        id: id.clone(),
    }));
    keys.extend(doc.motion_groups().iter().map(|g| ObjectKey {
        kind: ObjectKind::MotionGroup,
        id: g.id.clone(),
    }));
    keys.extend(doc.motion_registrations().map(|e| ObjectKey {
        kind: ObjectKind::MotionRegistration,
        id: e.id.clone(),
    }));
    keys.extend(doc.package_attachments().iter().map(|a| ObjectKey {
        kind: ObjectKind::Attachment,
        id: a.id.clone(),
    }));
    for clip in doc
        .motion_order()
        .iter()
        .filter_map(|id| doc.get_motion(id))
    {
        for t in &clip.tracks {
            keys.insert(ObjectKey {
                kind: ObjectKind::MotionTrack,
                id: t.id.clone(),
            });
            keys.extend(t.keys().map(|k| ObjectKey {
                kind: ObjectKind::MotionKey,
                id: k.id.clone(),
            }));
        }
        keys.extend(clip.events.iter().map(|e| ObjectKey {
            kind: ObjectKind::MotionEvent,
            id: e.id.clone(),
        }));
    }
    if let Some(pose) = doc.pose() {
        keys.insert(ObjectKey {
            kind: ObjectKind::Pose,
            id: pose.id.clone(),
        });
        for group in &pose.groups {
            keys.insert(ObjectKey {
                kind: ObjectKind::PoseGroup,
                id: group.id.clone(),
            });
            for entry in &group.entries {
                keys.insert(ObjectKey {
                    kind: ObjectKind::PoseEntry,
                    id: entry.id.clone(),
                });
                keys.extend(entry.links.iter().flatten().map(|l| ObjectKey {
                    kind: ObjectKind::PoseLink,
                    id: l.id.clone(),
                }));
            }
        }
    }
    if let Some(physics) = doc.physics() {
        keys.insert(ObjectKey {
            kind: ObjectKind::Physics,
            id: physics.id.clone(),
        });
    }
    if let Some(asset) = doc.physics() {
        for g in &asset.data.settings {
            keys.insert(ObjectKey {
                kind: ObjectKind::PhysicsGroup,
                id: g.id.clone(),
            });
            keys.extend(g.inputs.iter().map(|v| ObjectKey {
                kind: ObjectKind::PhysicsInput,
                id: v.id.clone(),
            }));
            keys.extend(g.outputs.iter().map(|v| ObjectKey {
                kind: ObjectKind::PhysicsOutput,
                id: v.id.clone(),
            }));
            keys.extend(g.vertices.iter().map(|v| ObjectKey {
                kind: ObjectKind::PhysicsNode,
                id: v.id.clone(),
            }));
        }
    }
    keys.extend(doc.binding_identities_iter().map(|i| ObjectKey {
        kind: match i.kind {
            kasane_core::document::BindingIdentityKind::Axis => ObjectKind::BindingAxis,
            kasane_core::document::BindingIdentityKind::Sample => ObjectKind::BindingSample,
        },
        id: i.id.clone(),
    }));
    keys
}
