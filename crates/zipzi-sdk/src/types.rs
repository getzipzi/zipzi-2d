//! Public SDK value types and crate-internal history identities.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use zipzi_core::document::DocumentCheckpoint;
use zipzi_core::{
    Appearance, BlendMode, BlendShapeBinding, ChangeKind, Glue, Mesh, MeshBinding, Status, Vec2,
    VertexId,
};
use zipzi_project::{ImportReport, ProjectResult, PsdImportReport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
pub struct Version {
    pub session_id: u64,
    pub generation: u64,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdkError {
    pub code: Box<str>,
    pub message: Box<str>,
    pub operation: &'static str,
    pub object_ids: Vec<String>,
    pub field_path: Option<Box<str>>,
    pub expected_version: Option<Box<Version>>,
    pub actual_version: Option<Box<Version>>,
    pub referrers: Box<[String]>,
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for SdkError {}

impl SdkError {
    pub(crate) fn new(code: &str, message: &str, operation: &'static str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            operation,
            object_ids: Vec::new(),
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        }
    }
    pub(crate) fn from_status(status: Status, operation: &'static str, ids: Vec<String>) -> Self {
        Self {
            code: status.code.into_boxed_str(),
            message: status.message.into_boxed_str(),
            operation,
            object_ids: ids,
            field_path: None,
            expected_version: None,
            actual_version: None,
            referrers: Box::new([]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditReceipt {
    pub label: String,
    pub before: Version,
    pub after: Version,
    pub kind: ChangeKind,
    pub object_ids: Vec<String>,
    pub changed: bool,
    pub changes: crate::ChangeSet,
}

impl EditReceipt {
    /// Conservative owned payload estimate for bounded host event streams.
    pub fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.label.capacity()
            + self.changes.estimated_bytes()
            + self
                .object_ids
                .iter()
                .map(|id| std::mem::size_of::<String>() + id.capacity())
                .sum::<usize>()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReceipt {
    pub before: Version,
    pub after: Version,
    pub manifest: PathBuf,
    pub warnings: Vec<String>,
    pub history_warnings: Vec<String>,
    pub durable: bool,
}

#[derive(Debug, Clone)]
pub struct ImportReceipt {
    pub before: Version,
    pub after: Version,
    pub project: ProjectResult,
    pub report: ImportReport,
}

#[derive(Debug, Clone)]
pub struct PsdImportReceipt {
    pub before: Version,
    pub after: Version,
    pub project: ProjectResult,
    pub report: PsdImportReport,
    /// Published manifest, or an empty path for an in-memory import.
    pub manifest: PathBuf,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum GeometrySpace {
    Model,
    RotationLocal(
        /// Stable rotation-transform ID defining the local position space.
        String,
    ),
    WarpParameter(
        /// Stable warp-transform ID defining the parameter/lattice position space.
        String,
    ),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct GeometrySnapshot {
    /// Exact SDK version captured for this value; not a best-effort latest-version marker.
    pub version: Version,
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Stable u32 vertex IDs in the same order as positions and UVs; IDs need not be contiguous.
    pub vertex_ids: Vec<VertexId>,
    /// Absolute XY vertices in the explicitly recorded GeometrySpace, aligned with vertex_ids.
    pub positions: Vec<Vec2>,
    /// Texture UV pairs ordered by vertex_ids; keep one UV for each position.
    pub uvs: Vec<Vec2>,
    /// Triangle triples of stable vertex IDs; preserve winding and do not substitute position-
    /// array offsets.
    pub triangles: Vec<[VertexId; 3]>,
    /// Coordinate space of positions; local-space variants carry the controlling transform ID.
    pub space: GeometrySpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape)]
pub enum ObjectKind {
    Asset,
    Mesh,
    Parameter,
    MeshBinding,
    BindingAxis,
    BindingSample,
    Part,
    Transform,
    SceneBinding,
    BlendKeyTable,
    BlendConstraint,
    BlendBinding,
    Glue,
    GlueVertexPair,
    Offscreen,
    DrawOrderGroup,
    CdiParameterGroup,
    CdiCombinedSet,
    CdiCombinedMember,
    ModelParameterGroup,
    ModelParameterMember,
    ModelHitArea,
    Expression,
    ExpressionEntry,
    Motion,
    MotionGroup,
    MotionRegistration,
    MotionTrack,
    MotionKey,
    MotionEvent,
    Attachment,
    Pose,
    PoseGroup,
    PoseEntry,
    PoseLink,
    Physics,
    PhysicsGroup,
    PhysicsInput,
    PhysicsOutput,
    PhysicsNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectHandle {
    pub(crate) session_id: u64,
    pub(crate) generation: u64,
    pub(crate) object_id: String,
    pub(crate) incarnation: u64,
    pub(crate) kind: ObjectKind,
}

impl ObjectHandle {
    pub fn id(&self) -> &str {
        &self.object_id
    }
    pub fn kind(&self) -> ObjectKind {
        self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ObjectKey {
    pub(crate) kind: ObjectKind,
    pub(crate) id: String,
}

pub(crate) fn object_key_bytes(keys: &HashSet<ObjectKey>) -> usize {
    keys.iter().map(|key| key.id.capacity()).fold(
        keys.capacity()
            .saturating_mul(std::mem::size_of::<ObjectKey>() + 1),
        usize::saturating_add,
    )
}

/// Editable display and drawing fields. Geometry, persistent ID and runtime ID
/// are preserved by `update_mesh_properties`.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshProperties {
    pub texture_asset_id: String,
    pub appearance: Appearance,
    pub draw_order: Option<f32>,
    pub blend_mode: BlendMode,
    pub enabled: bool,
    pub double_sided: bool,
    pub inverted_mask: bool,
    pub masks: Vec<String>,
}

impl From<&Mesh> for MeshProperties {
    fn from(mesh: &Mesh) -> Self {
        Self {
            texture_asset_id: mesh.texture_asset_id.as_str().to_owned(),
            appearance: mesh.appearance,
            draw_order: mesh.draw_order,
            blend_mode: mesh.blend_mode,
            enabled: mesh.enabled,
            double_sided: mesh.double_sided,
            inverted_mask: mesh.inverted_mask,
            masks: mesh.masks.clone(),
        }
    }
}

/// All geometry dependencies are explicit. For an unbound mesh, pass None and
/// empty vectors; mapping must mention every previous vertex ID exactly once.
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct TopologyReplacement {
    /// Complete replacement mesh geometry and metadata.
    pub mesh: Mesh,
    /// Optional replacement ordinary mesh binding; None supplies no ordinary binding.
    pub binding: Option<MeshBinding>,
    /// Complete replacement BlendShape bindings for the remeshed object.
    pub blend_bindings: Vec<BlendShapeBinding>,
    /// Complete replacement Glue records affected by the remesh.
    pub glues: Vec<Glue>,
    #[serde(deserialize_with = "zipzi_core::serde_maps::u32_map")]
    /// Old vertex ID to replacement ID map; Some(new_id) remaps an identity and None explicitly
    /// removes it.
    pub vertex_mapping: HashMap<VertexId, Option<VertexId>>,
}

pub(crate) struct HistoryEntry {
    // Checkpoints are immutable between exchange/relocation boundaries. Runtime
    // may query budgets after every publication without rescanning old models.
    pub(crate) estimated_bytes_cache: std::sync::OnceLock<usize>,
    pub(crate) label: String,
    pub(crate) changes: crate::ChangeSet,
    pub(crate) kind: ChangeKind,
    pub(crate) object_ids: Vec<String>,
    pub(crate) checkpoint: DocumentCheckpoint,
    pub(crate) identity_keys: HashSet<ObjectKey>,
    pub(crate) root: PathBuf,
}

impl HistoryEntry {
    pub(crate) fn estimated_bytes(&self) -> usize {
        *self
            .estimated_bytes_cache
            .get_or_init(|| self.measure_bytes())
    }

    fn measure_bytes(&self) -> usize {
        self.checkpoint.estimated_bytes() + self.metadata_bytes()
    }
    pub(crate) fn metadata_bytes(&self) -> usize {
        self.changes.estimated_bytes()
            + self
                .object_ids
                .iter()
                .map(|id| id.capacity() + std::mem::size_of::<String>())
                .sum::<usize>()
            + self.label.capacity()
            + std::mem::size_of::<String>()
            + self.identity_keys.capacity() * std::mem::size_of::<ObjectKey>()
            + self
                .identity_keys
                .iter()
                .map(|key| key.id.capacity())
                .sum::<usize>()
            + self.root.as_os_str().len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_steps: usize,
    pub max_bytes: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_steps: 50,
            max_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryState {
    pub undo_steps: usize,
    pub redo_steps: usize,
    pub estimated_bytes: usize,
    pub max_steps: usize,
    pub max_bytes: usize,
}
impl zipzi_core::api_shape::ApiShape for Version {
    fn api_shape() -> zipzi_core::api_shape::Shape {
        zipzi_core::api_shape::Shape::Opaque {
            name: concat!(module_path!(), "::Version").into(),
        }
    }
}
