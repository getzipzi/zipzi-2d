//! Persistent physics rig data with references bound to document parameter IDs.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

mod authoring;
mod model;
mod projection;
pub use authoring::*;
pub use model::*;
pub use projection::*;
mod patch;
use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};
pub use patch::{PhysicsAssetProperty, PhysicsPatch, PhysicsProperty, PhysicsRigPatch};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsAsset {
    pub id: String,
    pub data: PhysicsDefinition,
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_source_content_hash: Option<String>,
}

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
        if !super::valid_uuid(&asset.id) {
            return Status::error("INVALID_PHYSICS_ID", &asset.id);
        }
        if self
            .content
            .physics
            .as_ref()
            .is_none_or(|old| old.id != asset.id)
            && self.contains_id(&asset.id)
        {
            return Status::error("DUPLICATE_ID", &asset.id);
        }
        let mut ids = HashSet::from([asset.id.as_str()]);
        let old_ids: HashSet<_> = self
            .physics()
            .map(|a| a.entity_ids().collect())
            .unwrap_or_default();
        for id in asset.entity_ids().skip(1) {
            if !super::valid_uuid(id)
                || !ids.insert(id)
                || (!old_ids.contains(id) && self.contains_id(id))
            {
                return Status::error("DUPLICATE_OR_INVALID_PHYSICS_ID", id);
            }
        }
        if let Some(old) = self.physics() {
            let roles = old.identity_roles();
            if asset
                .identity_roles()
                .iter()
                .any(|(id, role)| roles.get(id).is_some_and(|old| old != role))
            {
                return Status::error(
                    "PHYSICS_IDENTITY_KIND_CHANGED",
                    "an existing ID cannot change entity type",
                );
            }
        }
        let mut external = HashSet::new();
        for rig in &asset.data.settings {
            if rig.external_id.is_empty()
                || rig.external_id.contains('\0')
                || !external.insert(&rig.external_id)
            {
                return Status::error("DUPLICATE_PHYSICS_EXTERNAL_ID", &rig.external_id);
            }
            let unchanged = trust_unchanged
                && self.physics().is_some_and(|a| {
                    a.data
                        .settings
                        .iter()
                        .any(|g| std::sync::Arc::ptr_eq(g, rig))
                });
            if unchanged {
                continue;
            }
            if let Err(status) = self.validate_native_physics_rig(rig) {
                return status;
            }
        }
        let dictionary: HashSet<_> = asset.data.meta.dictionary.iter().map(|d| &d.id).collect();
        if dictionary.len() != asset.data.meta.dictionary.len()
            || dictionary != asset.data.settings.iter().map(|g| &g.id).collect()
        {
            return Status::error(
                "INVALID_PHYSICS_DICTIONARY",
                "dictionary must match native groups",
            );
        }
        let mut meta = asset.data.meta.clone();
        meta.dictionary.clear();
        let wire = crate::physics::PhysicsDefinition {
            version: asset.data.version,
            meta,
            settings: vec![],
            extensions: asset.data.extensions.clone(),
        };
        if let Err(error) = crate::physics::validate_physics_definition(&wire) {
            return Status::error(error.code, format!("{}: {}", error.path, error.message));
        }
        Status::ok()
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
