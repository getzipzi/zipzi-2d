mod assets;
mod attachments;
mod bindings;
mod blendshapes;
mod canvas;
mod checkpoint;
mod display_info;
mod editor_state;
mod expressions;
mod glue;
mod hierarchy_order;
mod meshes;
mod model3;
mod motions;
mod offscreen;
mod parameters;
mod parts;
mod physics;
mod pose;
mod references;
mod structure;
mod transactions;
mod transforms;
mod validation;
mod vertices;

pub use attachments::{valid_attachment_path, PackageAttachment};
pub use checkpoint::DocumentCheckpoint;
pub use display_info::{
    CdiCombinedSet, CdiNamespaceIds, CdiParameterEntry, CdiParameterGroup, CdiParameterRef,
    CdiPartEntry, DisplayInfo, DisplayInfoOrigin,
};
pub use editor_state::{EditorState, ObjectEditorState};
pub use expressions::{ExpressionAsset, ExpressionBlend, ExpressionEntry, ExpressionTarget};
pub use hierarchy_order::HierarchyOrder;
pub use model3::{Model3Settings, ModelHitArea, ModelParameterGroup, ModelTargetRef};
pub use motions::{
    MotionClip, MotionEvent, MotionGroup, MotionKey, MotionPoint, MotionRegistration,
    MotionSegment, MotionTrack, MotionTrackTarget,
};
pub use physics::{PhysicsAsset, PhysicsPatch, PhysicsProperty};
pub use pose::{PoseAsset, PoseEntry, PosePartRef};
pub use structure::StructureIssue;
pub use validation::*;

use crate::draw_order::DrawOrderGroup;
use crate::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, Canvas, ChangeKind, ChangeSet,
    DocumentEdit, EditResult, ExportSettings, Glue, ImageAsset, Mesh, MeshBinding, ModelSetup,
    Offscreen, Parameter, Part, SceneBinding, Status, Transform, VertexId,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub(super) trait EmptyOr {
    fn empty_or(&self) -> bool;
}

impl<T> EmptyOr for Vec<T> {
    fn empty_or(&self) -> bool {
        self.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    content: DocumentContent,

    revision: u64,
    evaluation_revision: u64,
    batch_build_active: bool,
    transaction_active: bool,
    staged_edits: Vec<DocumentEdit>,

    mesh_runtime_ids: HashMap<String, String>,
    vertex_slots: HashMap<String, HashMap<VertexId, usize>>,

    saved_content: Option<Arc<DocumentContent>>,
    // Valid until persistent content or its saved baseline changes. OnceLock
    // keeps read-only queries thread-safe without repeating deep comparisons.
    modified_cache: OnceLock<bool>,
    lookup: OnceLock<DocumentLookup>,
    prepared: OnceLock<Result<crate::evaluation::PreparedEvaluation, Status>>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct DocumentLookup {
    pub(super) mesh_bindings: HashMap<String, String>,
    pub(super) scene_bindings: HashMap<String, String>,
    pub(super) blend_bindings: HashMap<String, Vec<String>>,
}

/// The single owning representation of persistent state, shared by live
/// documents, saved baselines, and checkpoints. Runtime indexes stay on Document.
#[derive(Debug, Clone, Default, PartialEq)]
struct DocumentContent {
    id: String,
    export_settings: ExportSettings,
    source_canvas: Option<Canvas>,
    assets: HashMap<String, ImageAsset>,
    asset_order: Vec<String>,
    parts: HashMap<String, Part>,
    part_order: Vec<String>,
    transforms: HashMap<String, Transform>,
    transform_order: Vec<String>,
    meshes: HashMap<String, Mesh>,
    mesh_order: Vec<String>,
    parameters: HashMap<String, Parameter>,
    parameter_order: Vec<String>,
    bindings: HashMap<String, MeshBinding>,
    binding_order: Vec<String>,
    scene_bindings: HashMap<String, SceneBinding>,
    scene_binding_order: Vec<String>,
    draw_order_groups: Option<Vec<DrawOrderGroup>>,
    blend_key_tables: HashMap<String, BlendShapeKeyTable>,
    blend_key_table_order: Vec<String>,
    blend_constraints: HashMap<String, BlendShapeConstraint>,
    blend_constraint_order: Vec<String>,
    blend_bindings: HashMap<String, BlendShapeBinding>,
    blend_binding_order: Vec<String>,
    glues: HashMap<String, Glue>,
    glue_order: Vec<String>,
    offscreens: HashMap<String, Offscreen>,
    offscreen_order: Vec<String>,
    display_info: DisplayInfo,
    hierarchy_order: HierarchyOrder,
    editor_state: EditorState,
    expressions: HashMap<String, ExpressionAsset>,
    expression_order: Vec<String>,
    motions: HashMap<String, std::sync::Arc<MotionClip>>,
    motion_order: Vec<String>,
    motion_groups: Vec<MotionGroup>,
    pose: Option<PoseAsset>,
    physics: Option<Arc<PhysicsAsset>>,
    missing_attachments: Vec<String>,
    model3_settings: Model3Settings,
    package_attachments: Vec<PackageAttachment>,
}

impl Document {
    pub const SCHEMA_VERSION: u32 = 6;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn initialize(&mut self, id: impl Into<String>, setup: impl Into<ModelSetup>) -> Status {
        if self.initialized() {
            return Status::error(
                "ALREADY_INITIALIZED",
                "Create a new Document to open another model.",
            );
        }
        let id_str = id.into();
        if !valid_uuid(&id_str) {
            return Status::error("INVALID_ID", "Use a nonzero canonical lowercase UUID.");
        }
        let setup = setup.into();
        if !setup.is_valid() {
            return Status::error(
                "INVALID_MODEL_SETUP",
                "Source metadata and export settings must have finite valid dimensions and scale.",
            );
        }
        self.content.id = id_str;
        self.content.export_settings = setup.export;
        self.content.source_canvas = setup.source_canvas;
        self.modified_cache.take();
        Status::ok()
    }

    pub fn initialized(&self) -> bool {
        !self.content.id.is_empty()
    }

    pub fn id(&self) -> &str {
        &self.content.id
    }

    pub fn export_settings(&self) -> ExportSettings {
        self.content.export_settings
    }

    pub fn coordinate_space(&self, parent: &str) -> Result<crate::CoordinateSpace, Status> {
        if parent.is_empty() {
            return Ok(crate::CoordinateSpace::Model);
        }
        let t = self
            .get_transform(parent)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", parent))?;
        Ok(if t.warp().is_some() {
            crate::CoordinateSpace::WarpParameter
        } else {
            crate::CoordinateSpace::RotationLocal
        })
    }

    pub fn source_canvas(&self) -> Option<Canvas> {
        self.content.source_canvas
    }

    pub fn model_setup(&self) -> ModelSetup {
        ModelSetup {
            export: self.export_settings(),
            source_canvas: self.source_canvas(),
        }
    }

    pub fn missing_attachments(&self) -> &[String] {
        &self.content.missing_attachments
    }

    /// Record unresolved model3 files in persistent project state. An editor
    /// must explicitly clear each entry after repairing or discarding it.
    pub fn set_missing_attachments(&mut self, mut paths: Vec<String>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        paths.sort();
        paths.dedup();
        if paths.iter().any(|path| path.is_empty()) {
            return self.failed(Status::error(
                "INVALID_ATTACHMENT",
                "Attachment path is empty",
            ));
        }
        if self.content.missing_attachments == paths {
            return self.failed(Status::ok());
        }
        self.content.missing_attachments = paths;
        self.changed(ChangeKind::Metadata, Vec::new(), Vec::new())
    }

    /// Last document revision that changed visual inputs. Names do not affect frames.
    pub fn evaluation_revision(&self) -> u64 {
        self.evaluation_revision
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn same_content(&self, other: &Document) -> bool {
        self.content == other.content
    }

    pub fn modified(&self) -> bool {
        *self
            .modified_cache
            .get_or_init(|| match &self.saved_content {
                Some(saved) => &self.content != saved.as_ref(),
                None => self.initialized(),
            })
    }

    pub fn mark_saved(&mut self) {
        self.saved_content = Some(Arc::new(self.content.clone()));
        self.modified_cache = OnceLock::from(false);
    }

    pub fn restore_from(&mut self, source: &Document) {
        let next_rev = self.revision + 1;
        let saved = self.saved_content.clone();
        *self = source.clone();
        self.saved_content = saved;
        // The source's cached result refers to its own saved baseline.
        self.modified_cache.take();
        self.revision = next_rev;
        self.evaluation_revision = next_rev;
        self.transaction_active = false;
        self.batch_build_active = false;
        self.staged_edits.clear();
    }

    pub fn transaction_active(&self) -> bool {
        self.transaction_active
    }

    /// Begin construction of a new document without publishing a revision and
    /// invalidating derived caches after every inserted object.
    pub fn begin_batch_build(&mut self) -> Status {
        if self.initialized() || self.batch_build_active || self.transaction_active {
            return Status::error(
                "INVALID_BUILD_STATE",
                "Batch construction requires a new document",
            );
        }
        self.batch_build_active = true;
        Status::ok()
    }

    /// Publish a successfully constructed candidate as one document revision.
    pub fn finish_batch_build(&mut self) -> Status {
        if !self.batch_build_active {
            return Status::error("NO_BATCH_BUILD", "Call begin_batch_build first");
        }
        if self.transaction_active || !self.staged_edits.is_empty() {
            return Status::error(
                "INVALID_BUILD_STATE",
                "Commit or cancel the active transaction before finishing the batch",
            );
        }
        if !self.initialized() {
            return Status::error("NOT_INITIALIZED", "Initialize the document first");
        }
        self.batch_build_active = false;
        self.lookup.take();
        self.prepared.take();
        self.revision = 1;
        self.evaluation_revision = 1;
        Status::ok()
    }

    pub(super) fn mutation_blocked(&self) -> bool {
        self.transaction_active
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.content.id == id
            || self.content.assets.contains_key(id)
            || self.content.parts.contains_key(id)
            || self.content.transforms.contains_key(id)
            || self.content.meshes.contains_key(id)
            || self.content.parameters.contains_key(id)
            || self.content.bindings.contains_key(id)
            || self.content.scene_bindings.contains_key(id)
            || self.content.blend_key_tables.contains_key(id)
            || self.content.blend_constraints.contains_key(id)
            || self.content.blend_bindings.contains_key(id)
            || self.content.glues.contains_key(id)
            || self.content.offscreens.contains_key(id)
            || self
                .content
                .display_info
                .parameter_groups
                .as_ref()
                .is_some_and(|groups| groups.iter().any(|group| group.id == id))
            || self
                .content
                .display_info
                .combined_parameters
                .as_ref()
                .is_some_and(|sets| sets.iter().any(|set| set.id == id))
            || self.content.expressions.contains_key(id)
            || self.content.motions.contains_key(id)
            || self.content.pose.as_ref().is_some_and(|pose| pose.id == id)
            || self
                .content
                .physics
                .as_ref()
                .is_some_and(|physics| physics.id == id)
    }

    pub(super) fn failed(&self, status: Status) -> EditResult {
        EditResult {
            status,
            changes: ChangeSet {
                kind: ChangeKind::None,
                mesh_ids: Vec::new(),
                revision: self.revision,
                object_ids: Vec::new(),
            },
            referrers: Vec::new(),
        }
    }

    pub(crate) fn prepared_evaluation(
        &self,
    ) -> Result<&crate::evaluation::PreparedEvaluation, &Status> {
        self.prepared
            .get_or_init(|| crate::evaluation::PreparedEvaluation::new(self))
            .as_ref()
    }

    pub(super) fn changed(
        &mut self,
        kind: ChangeKind,
        mesh_ids: Vec<String>,
        mut object_ids: Vec<String>,
    ) -> EditResult {
        // Batch construction mutates content without advancing revision too.
        self.modified_cache.take();
        if self.batch_build_active {
            if object_ids.is_empty() {
                object_ids = mesh_ids.clone();
            }
            if kind == ChangeKind::Structure {
                self.lookup.take();
                self.prepared.take();
            }
            return EditResult {
                status: Status::ok(),
                changes: ChangeSet {
                    kind,
                    mesh_ids,
                    revision: self.revision,
                    object_ids,
                },
                referrers: Vec::new(),
            };
        }
        if object_ids.is_empty() {
            object_ids = mesh_ids.clone();
        }
        // References only change with structure edits; vertex drags retain the index.
        if kind == ChangeKind::Structure {
            self.lookup.take();
            self.prepared.take();
        }
        self.revision += 1;
        if kind != ChangeKind::Metadata {
            self.evaluation_revision = self.revision;
        }
        EditResult {
            status: Status::ok(),
            changes: ChangeSet {
                kind,
                mesh_ids,
                revision: self.revision,
                object_ids,
            },
            referrers: Vec::new(),
        }
    }

    pub(super) fn lookup(&self) -> &DocumentLookup {
        self.lookup.get_or_init(|| {
            let mut lookup = DocumentLookup::default();
            for id in &self.content.binding_order {
                lookup
                    .mesh_bindings
                    .insert(self.content.bindings[id].mesh_id.clone(), id.clone());
            }
            for id in &self.content.scene_binding_order {
                lookup.scene_bindings.insert(
                    self.content.scene_bindings[id].target_id().to_owned(),
                    id.clone(),
                );
            }
            for id in &self.content.blend_binding_order {
                if let Some(binding) = self.content.blend_bindings.get(id) {
                    lookup
                        .blend_bindings
                        .entry(binding.target_id.clone())
                        .or_default()
                        .push(id.clone());
                }
            }
            lookup
        })
    }
}
