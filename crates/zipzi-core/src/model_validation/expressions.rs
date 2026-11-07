use super::*;
impl Model<'_> {
    pub(crate) fn validate_expression(&self, expression: &ExpressionAsset) -> Status {
        if !valid_uuid(&expression.id) {
            return Status::error("INVALID_EXPRESSION_ID", &expression.id);
        }
        if expression.extensions.contains_key("ZipziSourceScene") {
            let Some(scene) = expression.source_scene() else {
                return Status::error("INVALID_EXPRESSION_SOURCE_SCENE", &expression.id);
            };
            let status = self.validate_motion(&scene);
            if !status.is_ok() {
                return status;
            }
            if scene.id != expression.id
                || !scene.events.is_empty()
                || scene
                    .tracks
                    .iter()
                    .any(|track| track.initial.time != 0. || !track.segments.is_empty())
            {
                return Status::error("INVALID_EXPRESSION_SOURCE_SCENE", &expression.id);
            }
        }
        if expression.name.is_empty() || expression.name.contains('\0') {
            return Status::error("INVALID_EXPRESSION_NAME", &expression.id);
        }
        if self
            .content
            .expressions
            .values()
            .any(|other| other.id != expression.id && other.name == expression.name)
        {
            return Status::error("DUPLICATE_EXPRESSION_NAME", &expression.name);
        }
        if expression
            .file_type
            .as_ref()
            .is_some_and(|value| value.contains('\0'))
        {
            return Status::error("INVALID_EXPRESSION_TYPE", &expression.id);
        }
        for (path, fade) in [
            ("fade_in", expression.fade_in),
            ("fade_out", expression.fade_out),
        ] {
            if fade.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Status::error(
                    "INVALID_EXPRESSION_FADE",
                    format!("{}.{}", expression.id, path),
                );
            }
        }
        let mut ids = HashSet::new();
        for (index, entry) in expression.entries.iter().enumerate() {
            if !valid_uuid(&entry.id) || entry.id == expression.id || !ids.insert(&entry.id) {
                return Status::error("INVALID_EXPRESSION_ENTRY_ID", &entry.id);
            }
            if !entry.value.is_finite() {
                return Status::error(
                    "INVALID_EXPRESSION_VALUE",
                    format!("{}.entries[{index}]", expression.id),
                );
            }
            match &entry.target {
                ExpressionTarget::Resolved { parameter_id }
                    if self.get_parameter(parameter_id).is_none() =>
                {
                    return Status::error(
                        "MISSING_PARAMETER",
                        format!("{}.entries[{index}]", expression.id),
                    );
                }
                ExpressionTarget::Unresolved { runtime_id }
                    if runtime_id.is_empty() || runtime_id.contains('\0') =>
                {
                    return Status::error(
                        "INVALID_EXPRESSION_TARGET",
                        format!("{}.entries[{index}]", expression.id),
                    );
                }
                _ => {}
            }
        }
        Status::ok()
    }
}
