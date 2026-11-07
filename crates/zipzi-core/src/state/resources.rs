use super::*;
impl DocumentBuilder {
    pub fn edit_resource(&mut self, intent: ResourceEdit) -> Result<(), Status> {
        self.write(|this| match intent {
            ResourceEdit::Create { id, name, before } => {
                this.create_asset(AssetRecord {
                    id: id.clone(),
                    name,
                    content: AssetContent::Empty,
                })?;
                this.place_assets(vec![id], before)
            }
            ResourceEdit::PatchName { id, expected, name } => {
                this.authoring_unlocked(&id)?;
                let mut asset = this
                    .asset(&id)
                    .cloned()
                    .ok_or_else(|| Status::error("MISSING_ASSET", &id))?;
                if asset.name != expected {
                    return Err(Status::error("STALE_FIELD", &id));
                }
                if name.contains('\0') {
                    return Err(Status::error("INVALID_NAME", &id));
                }
                asset.name = name;
                this.replace_asset(asset)
            }
            ResourceEdit::Reorder { ids, before } => this.place_assets(ids, before),
            ResourceEdit::Duplicate {
                source,
                id,
                name,
                before,
            } => {
                this.duplicate_asset(&source, id.clone(), name)?;
                this.place_assets(vec![id], before)
            }
            ResourceEdit::Delete { id, replacement } => this.delete_asset(&id, replacement),
        })
    }
    fn place_assets(&mut self, ids: Vec<String>, before: Option<String>) -> Result<(), Status> {
        let selected: std::collections::HashSet<_> = ids.iter().collect();
        if selected.len() != ids.len() {
            return Err(Status::error(
                "DUPLICATE_ID",
                "Resource selection contains duplicates",
            ));
        }
        for id in &ids {
            if self.asset(id).is_none() {
                return Err(Status::error("MISSING_ASSET", id));
            }
            self.authoring_unlocked(id)?;
        }
        let mut order: Vec<_> = self
            .asset_order()
            .iter()
            .filter(|id| !selected.contains(id))
            .cloned()
            .collect();
        let index = match before {
            Some(anchor) => order
                .iter()
                .position(|id| id == &anchor)
                .ok_or_else(|| Status::error("INVALID_ANCHOR", anchor))?,
            None => order.len(),
        };
        order.splice(index..index, ids);
        self.reorder_assets(order)
    }
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
