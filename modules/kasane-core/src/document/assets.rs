use super::*;
use crate::types::{ChangeKind, EditResult, ImageAsset, Status};

impl Document {
    pub fn asset_order(&self) -> &[String] {
        &self.content.asset_order
    }

    pub fn asset_count(&self) -> usize {
        self.content.assets.len()
    }

    pub fn get_asset(&self, id: &str) -> Option<&ImageAsset> {
        self.content.assets.get(id)
    }

    pub fn add_asset(&mut self, asset: ImageAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel the active transaction first.",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error(
                "NOT_INITIALIZED",
                "Initialize Document first.",
            ));
        }
        if !valid_uuid(&asset.id) {
            return self.failed(Status::error(
                "INVALID_ID",
                "Asset ID must be a canonical UUID.",
            ));
        }
        if self.contains_id(&asset.id) {
            return self.failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
        }
        if asset
            .unresolved_source
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.contains('\0'))
            || (asset.content.is_some() && asset.unresolved_source.is_some())
        {
            return self.failed(Status::error("INVALID_ASSET", &asset.id));
        }
        if asset
            .content
            .as_ref()
            .is_some_and(|content| !content.is_valid())
        {
            return self.failed(Status::error(
                "INVALID_ASSET",
                "Asset requires source and positive pixel dimensions.",
            ));
        }
        let key = asset.id.clone();
        if let Err(status) = self.authoring_unlocked(&key) {
            return self.failed(status);
        }
        if self.get_asset(&key) == Some(&asset) {
            return self.failed(Status::ok());
        }
        self.content.assets.insert(key.clone(), asset);
        self.content.asset_order.push(key.clone());
        self.changed(ChangeKind::Metadata, Vec::new(), vec![key])
    }

    pub fn replace_asset(&mut self, asset: ImageAsset) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &asset.id));
        }
        if !self.content.assets.contains_key(&asset.id) {
            return self.failed(Status::error("MISSING_ASSET", &asset.id));
        }
        if asset
            .unresolved_source
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.contains('\0'))
            || (asset.content.is_some() && asset.unresolved_source.is_some())
        {
            return self.failed(Status::error("INVALID_ASSET", &asset.id));
        }
        if asset
            .content
            .as_ref()
            .is_some_and(|content| !content.is_valid())
        {
            return self.failed(Status::error("INVALID_ASSET", &asset.id));
        }
        let key = asset.id.clone();
        if let Err(status) = self.authoring_unlocked(&key) {
            return self.failed(status);
        }
        if self.get_asset(&key) == Some(&asset) {
            return self.failed(Status::ok());
        }
        let metadata = self.get_asset(&key).unwrap().content == asset.content;
        self.content.assets.insert(key.clone(), asset);
        let meshes = if metadata {
            vec![]
        } else {
            self.content.mesh_order.clone()
        };
        self.changed(
            if metadata {
                ChangeKind::Metadata
            } else {
                ChangeKind::Resources
            },
            meshes,
            vec![key],
        )
    }
}
