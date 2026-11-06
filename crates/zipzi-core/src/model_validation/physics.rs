use super::*;
use crate::document::physics::model::PhysicsAssetIdentity;
impl Model<'_> {
    pub(crate) fn validate_physics(&self, asset: &PhysicsAsset, trust_unchanged: bool) -> Status {
        if !valid_uuid(&asset.id) {
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
            if !valid_uuid(id) || !ids.insert(id) || (!old_ids.contains(id) && self.contains_id(id))
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
}
