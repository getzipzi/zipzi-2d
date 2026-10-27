//! Persistent pose3 authoring asset. Runtime fade state belongs to animation.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};
mod authoring;
mod projection;
pub use authoring::*;
pub use projection::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PosePartRef {
    Unspecified,
    Resolved {
        /// Stable document Part UUID for this opacity or visibility reference.
        part_id: String,
    },
    Unresolved {
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PoseEntry {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed resolved/unresolved Part reference retained by the Pose entry or link.
    pub part: PosePartRef,
    /// Optional ordered Pose links; None retains absence rather than an explicit empty list.
    pub links: Option<crate::SharedVec<PoseLink>>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PoseLink {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: PosePartRef,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PoseGroup {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Human-readable display or registration name; distinct from persistent object identity.
    pub name: String,
    /// Ordered owned registrations or expression/Pose entries, with stable identities.
    pub entries: crate::SharedVec<PoseEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct PoseAsset {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Optional format type label retained from the imported expression or Pose asset.
    pub file_type: Option<String>,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Ordered owned Pose groups; each owns its entries and link rows.
    pub groups: crate::SharedVec<PoseGroup>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
    /// Original runtime-ID namespace used to validate preservation of opaque imported fields.
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Original imported-content hash used to detect unsupported changes to opaque fields.
    pub opaque_source_content_hash: Option<String>,
}

impl PoseAsset {
    pub(super) fn same_execution(&self, other: &Self) -> bool {
        self.id == other.id
            && self.file_type == other.file_type
            && self.fade_in == other.fade_in
            && self.extensions == other.extensions
            && self.opaque_source_ids == other.opaque_source_ids
            && self.opaque_source_content_hash == other.opaque_source_content_hash
            && self.groups.len() == other.groups.len()
            && self
                .groups
                .iter()
                .zip(&other.groups)
                .all(|(a, b)| a.id == b.id && a.entries == b.entries)
    }

    pub fn entity_ids(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.id.as_str()).chain(self.groups.iter().flat_map(|g| {
            std::iter::once(g.id.as_str()).chain(g.entries.iter().flat_map(|e| {
                std::iter::once(e.id.as_str())
                    .chain(e.links.iter().flatten().map(|l| l.id.as_str()))
            }))
        }))
    }

    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self
                .groups
                .iter()
                .flat_map(|group| &group.entries)
                .any(|entry| !entry.extensions.is_empty())
    }
}

impl Document {
    /// Share an immutable pose asset without copying its authored content.
    pub fn shared_pose(&self) -> Option<std::sync::Arc<PoseAsset>> {
        self.content.pose.clone()
    }

    pub fn pose(&self) -> Option<&PoseAsset> {
        self.content.pose.as_deref()
    }

    pub fn set_pose(&mut self, pose: PoseAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Err(status) = self.validate_pose_replacement(Some(&pose)) {
            return self.failed(status);
        }
        if self.content.pose.as_deref() == Some(&pose) {
            return self.failed(Status::ok());
        }
        let id = pose.id.clone();
        self.install_pose(Some(pose));
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    fn install_pose(&mut self, pose: Option<PoseAsset>) {
        let retained: HashSet<_> = pose.iter().flat_map(|p| p.entity_ids()).collect();
        let removed: Vec<_> = self
            .pose()
            .into_iter()
            .flat_map(|p| p.entity_ids())
            .filter(|id| !retained.contains(id))
            .map(str::to_owned)
            .collect();
        for id in removed {
            self.content.editor_state.objects.remove(&id);
        }
        self.content.pose = pose.map(std::sync::Arc::new);
    }

    pub(super) fn validate_pose(&self, pose: &PoseAsset) -> Status {
        if !super::valid_uuid(&pose.id) {
            return Status::error("INVALID_POSE_ID", &pose.id);
        }
        if pose
            .file_type
            .as_deref()
            .is_some_and(|value| value != "Live2D Pose")
        {
            return Status::error("INVALID_POSE_TYPE", &pose.id);
        }
        if pose
            .fade_in
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Status::error("INVALID_POSE_FADE", &pose.id);
        }
        let owned: HashSet<_> = self
            .pose()
            .into_iter()
            .flat_map(|p| p.entity_ids())
            .collect();
        let mut ids = HashSet::new();
        for id in pose.entity_ids() {
            if !super::valid_uuid(id) || !ids.insert(id) {
                return Status::error("INVALID_POSE_IDENTITY", id);
            }
            if !owned.contains(id) && self.contains_id(id) {
                return Status::error("DUPLICATE_ID", id);
            }
        }
        for (group_index, group) in pose.groups.iter().enumerate() {
            if group.name.contains('\0') {
                return Status::error("INVALID_POSE_GROUP_NAME", &group.id);
            }
            let mut seen = HashSet::new();
            for (entry_index, entry) in group.entries.iter().enumerate() {
                let path = format!("{}.groups[{group_index}][{entry_index}]", pose.id);
                let runtime = match self.validate_pose_ref(&entry.part) {
                    Ok(id) => id,
                    Err(status) => return status,
                };
                if runtime.is_some_and(|runtime| !seen.insert(runtime)) {
                    return Status::error("DUPLICATE_POSE_PART", path);
                }
                if let Some(links) = &entry.links {
                    let mut linked = HashSet::new();
                    for target in links {
                        let runtime = match self.validate_pose_ref(&target.target) {
                            Ok(id) => id,
                            Err(status) => return status,
                        };
                        if runtime.is_some_and(|runtime| !linked.insert(runtime)) {
                            return Status::error("DUPLICATE_POSE_LINK", path);
                        }
                    }
                }
            }
        }
        Status::ok()
    }

    fn validate_pose_ref(&self, target: &PosePartRef) -> Result<Option<String>, Status> {
        match target {
            PosePartRef::Unspecified => Ok(None),
            PosePartRef::Resolved { part_id } => self
                .get_part(part_id)
                .map(|part| Some(part.runtime_id.clone()))
                .ok_or_else(|| Status::error("MISSING_PART", part_id)),
            PosePartRef::Unresolved { runtime_id }
                if runtime_id.is_empty() || runtime_id.contains('\0') =>
            {
                Err(Status::error("INVALID_POSE_PART", runtime_id))
            }
            PosePartRef::Unresolved { runtime_id } => Ok(Some(runtime_id.clone())),
        }
    }

    pub(super) fn validate_pose_asset(&self) -> Vec<StructureIssue> {
        self.content
            .pose
            .as_ref()
            .and_then(|pose| {
                let status = self.validate_pose(pose);
                (!status.is_ok()).then(|| StructureIssue {
                    object_id: pose.id.clone(),
                    status,
                })
            })
            .into_iter()
            .collect()
    }
}
