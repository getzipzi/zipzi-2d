//! Public SDK value types and crate-internal history identities.
use crate::changes::ChangeSetExt;
use std::collections::HashSet;
use std::path::PathBuf;

use zipzi_core::document::DocumentCheckpoint;
use zipzi_core::{Appearance, BlendMode, ChangeKind, Mesh};
use zipzi_project::{ImportReport, ProjectResult, PsdImportReport};

pub use zipzi_contract::authoring::Version;

pub use zipzi_contract::authoring::{EditReceipt, SaveReceipt, SdkError};

pub trait EditReceiptSize {
    fn estimated_bytes(&self) -> usize;
}
impl EditReceiptSize for EditReceipt {
    /// Conservative owned payload estimate for bounded host event streams.
    fn estimated_bytes(&self) -> usize {
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

pub use zipzi_contract::authoring::{GeometrySnapshot, GeometrySpace, ObjectKind};

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
pub use zipzi_contract::authoring::TopologyReplacement;

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
