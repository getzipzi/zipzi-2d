use super::*;
impl DocumentBuilder {
    pub fn duplicate_asset(
        &mut self,
        source: &str,
        id: String,
        name: String,
    ) -> Result<(), Status> {
        self.write(|this| {
            this.authoring_unlocked(source)?;
            let mut asset = this
                .asset(source)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_ASSET", source))?;
            if name.contains('\0') {
                return Err(Status::error("INVALID_NAME", name));
            }
            let base = if name.is_empty() {
                asset.name.clone()
            } else {
                name
            };
            let mut name = base.clone();
            let mut n = 2;
            while this
                .content
                .assets
                .shared_map()
                .values()
                .any(|a| a.name == name)
            {
                name = format!("{base} ({n})");
                n += 1;
            }
            asset.id = id;
            asset.name = name;
            this.create_asset(asset)
        })
    }
    pub fn delete_asset(
        &mut self,
        id: &str,
        replacement: Option<crate::SceneReference>,
    ) -> Result<(), Status> {
        self.write(|this| {
            this.authoring_unlocked(id)?;
            if this.asset(id).is_none() {
                return Err(Status::error("MISSING_ASSET", id));
            }
            let users = this.references_to(id);
            if !users.is_empty() && replacement.is_none() {
                return Err(Status::error("OBJECT_REFERENCED", users.join(", ")));
            }
            if let Some(target) = replacement {
                if let Some(target_id) = target.resolved_id() {
                    if target_id == id {
                        return Err(Status::error("SELF_REFERENCE", id));
                    }
                    if this.asset(target_id).is_none() {
                        return Err(Status::error("MISSING_ASSET", target_id));
                    }
                }
                for user in users {
                    this.authoring_unlocked(&user)?;
                    let mut mesh = this
                        .mesh(&user)
                        .cloned()
                        .ok_or_else(|| Status::error("UNSUPPORTED_REFERENCE", &user))?;
                    mesh.texture_asset_id = target.clone();
                    this.replace_mesh(mesh)?;
                }
            }
            this.clear_owned_editor_objects(&[id.into()]);
            this.remove_asset(id)
        })
    }
    pub fn reorder_assets(&mut self, order: Vec<String>) -> Result<(), Status> {
        self.write(|this| {
            for (slot, id) in order.iter().enumerate() {
                if this.asset_order().get(slot) != Some(id) {
                    this.authoring_unlocked(id)?;
                }
            }
            this.reorder(RecordDomain::Asset, order)
        })
    }
}
