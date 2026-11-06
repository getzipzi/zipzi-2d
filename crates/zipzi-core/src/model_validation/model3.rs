use super::*;
impl Model<'_> {
    pub(crate) fn validate_model3_settings(&self, settings: &Model3Settings) -> Status {
        if let Err(s) = self.validate_reference_storage(settings, self.display_info()) {
            return s;
        }
        let valid_target = |target: &ModelTargetRef, parameter: bool| match target {
            ModelTargetRef::Unspecified => true,
            ModelTargetRef::Resolved { object_id } => {
                if parameter {
                    self.get_parameter(object_id).is_some()
                } else {
                    self.get_mesh(object_id).is_some()
                }
            }
            ModelTargetRef::Unresolved { runtime_id } => {
                !runtime_id.is_empty() && !runtime_id.contains('\0')
            }
        };
        for group in settings.groups.iter().flatten() {
            let mut seen = std::collections::HashSet::new();
            for p in &group.parameters {
                let key = match &p.target {
                    ModelTargetRef::Unspecified => continue,
                    ModelTargetRef::Resolved { object_id } => self
                        .get_parameter(object_id)
                        .map(|p| p.runtime_id.as_str())
                        .unwrap_or(object_id),
                    ModelTargetRef::Unresolved { runtime_id } => runtime_id,
                };
                if !seen.insert(key) {
                    return Status::error("DUPLICATE_REFERENCE_TARGET", &p.id);
                }
            }
            if group.name.is_empty()
                || group.name.contains('\0')
                || group
                    .parameters
                    .iter()
                    .any(|target| !valid_target(&target.target, true))
            {
                return Status::error("INVALID_MODEL3_GROUPS", &group.name);
            }
        }
        for area in settings.hit_areas.iter().flatten() {
            // Cubism permits unnamed hit areas; the Mao sample uses them.
            if area.name.contains('\0') || !valid_target(&area.mesh, false) {
                return Status::error("INVALID_MODEL3_HIT_AREAS", &area.name);
            }
        }
        if settings
            .layout
            .iter()
            .flat_map(|layout| layout.iter())
            .any(|(key, value)| key.is_empty() || key.contains('\0') || !value.is_finite())
        {
            return Status::error(
                "INVALID_MODEL3_LAYOUT",
                "Layout requires named finite values",
            );
        }
        self.validate_model3_user_data(settings.user_data.as_ref())
    }
    pub(crate) fn validate_model3_user_data(&self, reference: Option<&AttachmentRef>) -> Status {
        if reference.is_some_and(|path| {
            self.attachment_path(path)
                .is_none_or(|p| !valid_attachment_path(p))
        }) {
            return Status::error(
                "INVALID_ATTACHMENT_PATH",
                "Invalid UserData attachment reference",
            );
        }
        Status::ok()
    }
}
