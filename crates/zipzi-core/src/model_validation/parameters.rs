use super::*;
impl Model<'_> {
    pub(crate) fn validate_parameter(&self, p: &Parameter) -> Status {
        if !valid_uuid(&p.id) {
            return Status::error(
                "INVALID_ID",
                format!("{}: parameter requires a canonical UUID", p.id),
            );
        }
        if p.runtime_id.is_empty() {
            return Status::error("INVALID_ID", format!("{}.runtime_id is empty", p.id));
        }
        for (id, other) in &self.content.parameters {
            if id != &p.id && other.runtime_id == p.runtime_id {
                return Status::error(
                    "DUPLICATE_RUNTIME_ID",
                    format!("{}.runtime_id duplicates {}", p.id, id),
                );
            }
        }
        if self.content.display_info.parameters.as_ref().is_some_and(|entries| entries.iter().any(|entry| {
            matches!(entry, CdiParameterEntry::Unresolved { runtime_id, .. } if runtime_id == &p.runtime_id)
        })) {
            return Status::error("DUPLICATE_RUNTIME_ID", format!("{}.runtime_id collides with unresolved CDI parameter", p.id));
        }
        if validate_parameter_range(p.minimum, p.maximum, p.default_value).is_err() {
            return Status::error(
                "INVALID_PARAMETER",
                format!(
                    "{}: require finite minimum < maximum and default in range",
                    p.id
                ),
            );
        }
        if p.decimal_places < 0 || p.decimal_places > 9 {
            return Status::error(
                "INVALID_PARAMETER",
                format!("{}.decimal_places must be 0..9", p.id),
            );
        }
        Status::ok()
    }
}
