//! Persistent physics rig data with references bound to document parameter IDs.
use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

mod authoring;
pub(crate) mod model;
mod projection;
pub use authoring::*;
pub use model::*;
pub use projection::*;
mod patch;
use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};
pub use patch::{PhysicsAssetProperty, PhysicsPatch, PhysicsProperty, PhysicsRigPatch};

pub use zipzi_contract::physics_document::PhysicsAsset;

impl Document {
    /// Share an immutable physics asset without copying its authored content.
    pub fn shared_physics(&self) -> Option<std::sync::Arc<PhysicsAsset>> {
        self.content.physics.clone()
    }

    pub fn physics(&self) -> Option<&PhysicsAsset> {
        self.content.physics.as_deref()
    }

    pub fn set_physics(&mut self, mut asset: PhysicsAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        asset.reconcile_counts();
        if let Err(status) = self.validate_physics_replacement_unlocked(&asset) {
            return self.failed(status);
        }
        let status = self.validate_physics(&asset, true);
        if !status.is_ok() {
            return self.failed(status);
        }
        if self.content.physics.as_deref() == Some(&asset) {
            return self.failed(Status::ok());
        }
        let evaluation_changed = self.physics().is_none_or(|old| {
            old.data.settings != asset.data.settings
                || old.data.meta.fps != asset.data.meta.fps
                || old.data.meta.forces != asset.data.meta.forces
        });
        let id = asset.id.clone();
        if let Some(old) = self.physics() {
            let retained: HashSet<_> = asset.entity_ids().collect();
            let removed: Vec<_> = old
                .entity_ids()
                .filter(|id| !retained.contains(id))
                .map(str::to_owned)
                .collect();
            for id in removed {
                self.content.editor_state.objects.remove(&id);
            }
        }
        self.content.physics = Some(std::sync::Arc::new(asset));
        let result = self.changed(ChangeKind::Metadata, Vec::new(), vec![id]);
        if evaluation_changed {
            self.evaluation_revision = self.revision;
        }
        result
    }

    fn validate_physics(&self, asset: &PhysicsAsset, trust_unchanged: bool) -> Status {
        self.validation_model()
            .validate_physics(asset, trust_unchanged)
    }

    pub(super) fn validate_physics_asset(&self) -> Vec<StructureIssue> {
        self.content
            .physics
            .as_ref()
            .and_then(|asset| {
                let status = self.validate_physics(asset, false);
                (!status.is_ok()).then(|| StructureIssue {
                    object_id: asset.id.clone(),
                    status,
                })
            })
            .into_iter()
            .collect()
    }
}
