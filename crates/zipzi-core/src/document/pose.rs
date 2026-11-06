//! Persistent pose3 authoring asset. Runtime fade state belongs to animation.
use std::collections::{BTreeMap, HashSet};

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};
mod authoring;
mod projection;
pub use authoring::*;
pub use projection::*;

pub use zipzi_contract::document::{PoseEntry, PoseGroup, PoseLink, PosePartRef};

pub use zipzi_contract::document::PoseAsset;

pub(super) fn same_execution(value: &PoseAsset, other: &PoseAsset) -> bool {
    value.id == other.id
        && value.file_type == other.file_type
        && value.fade_in == other.fade_in
        && value.extensions == other.extensions
        && value.opaque_source_ids == other.opaque_source_ids
        && value.opaque_source_content_hash == other.opaque_source_content_hash
        && value.groups.len() == other.groups.len()
        && value
            .groups
            .iter()
            .zip(&other.groups)
            .all(|(a, b)| a.id == b.id && a.entries == b.entries)
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
        self.validation_model().validate_pose(pose)
    }

    fn validate_pose_ref(&self, target: &PosePartRef) -> Result<Option<String>, Status> {
        self.validation_model().validate_pose_ref(target)
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
