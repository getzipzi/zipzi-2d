use super::*;
impl DocumentState {
    pub fn pose_fields(&self, id: &str) -> Option<PoseFields> {
        self.content.model().pose_fields(id)
    }
}
impl DocumentBuilder {
    pub fn pose_fields(&self, id: &str) -> Option<PoseFields> {
        self.content.model().pose_fields(id)
    }
    pub fn replace_pose(&mut self, asset: Option<PoseAsset>) -> Result<(), Status> {
        self.write(|this| {
            this.content
                .model()
                .validate_pose_replacement(asset.as_ref())?;
            let retained: std::collections::HashSet<_> =
                asset.iter().flat_map(|p| p.entity_ids()).collect();
            let removed: Vec<_> = this
                .pose()
                .iter()
                .flat_map(|p| p.entity_ids())
                .filter(|id| !retained.contains(id))
                .map(str::to_owned)
                .collect();
            this.clear_owned_editor_objects(&removed);
            this.set_pose(asset.map(Arc::new))
        })
    }
    pub fn edit_pose(&mut self, edit: PoseEdit) -> Result<(), Status> {
        self.write(|this| {
            let change = this.content.model().pose_changes(edit)?;
            if let Some((id, fields)) = change.patch {
                let mut asset = this.pose().as_ref().unwrap().as_ref().clone();
                crate::model_validation::pose_authoring::apply_fields(&mut asset, &id, &fields);
                this.replace_pose(Some(asset))
            } else {
                this.replace_pose(change.replacement)
            }
        })
    }
}
