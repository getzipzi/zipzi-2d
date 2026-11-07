use super::*;
impl Model<'_> {
    pub(crate) fn validate_pose(&self, pose: &PoseAsset) -> Status {
        if !valid_uuid(&pose.id) {
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
            if !valid_uuid(id) || !ids.insert(id) {
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
    pub(crate) fn validate_pose_ref(&self, target: &PosePartRef) -> Result<Option<String>, Status> {
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
}
