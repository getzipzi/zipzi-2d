//! Typed supplemental model metadata. Known references use stable document IDs.
use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};

pub use zipzi_contract::document::{
    Model3Settings, ModelHitArea, ModelParameterGroup, ModelTargetRef,
};

impl Document {
    pub fn model3_settings(&self) -> &Model3Settings {
        &self.content.model3_settings
    }

    pub fn set_model3_settings(&mut self, settings: Model3Settings) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Err(status) =
            self.validate_reference_replacement_unlocked(&settings, self.display_info())
        {
            return self.failed(status);
        }
        let status = self.validate_model3_settings(&settings);
        if !status.is_ok() {
            return self.failed(status);
        }
        if self.content.model3_settings == settings {
            return self.failed(Status::ok());
        }
        let previous = self
            .reference_identities()
            .iter()
            .map(|i| i.id.clone())
            .collect();
        self.content.model3_settings = settings;
        self.clean_retired_reference_flags(previous);
        self.changed(ChangeKind::Metadata, Vec::new(), Vec::new())
    }

    pub(super) fn validate_model3_settings(&self, settings: &Model3Settings) -> Status {
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

    pub(super) fn validate_model3_user_data(
        &self,
        reference: Option<&super::AttachmentRef>,
    ) -> Status {
        if reference.is_some_and(|path| {
            self.attachment_path(path)
                .is_none_or(|p| !super::valid_attachment_path(p))
        }) {
            return Status::error(
                "INVALID_ATTACHMENT_PATH",
                "Invalid UserData attachment reference",
            );
        }
        Status::ok()
    }
    pub(super) fn validate_model3(&self) -> Vec<StructureIssue> {
        let status = self.validate_model3_settings(&self.content.model3_settings);
        if status.is_ok() {
            Vec::new()
        } else {
            vec![StructureIssue {
                object_id: self.content.id.clone(),
                status,
            }]
        }
    }
}
pub trait AllocateModelMetadata: Sized {
    fn allocate() -> Self;
}
impl AllocateModelMetadata for ModelParameterGroup {
    fn allocate() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Parameter Group".into(),
            parameters: vec![],
            extensions: Default::default(),
        }
    }
}
impl AllocateModelMetadata for ModelHitArea {
    fn allocate() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Hit Area".into(),
            mesh: ModelTargetRef::Unspecified,
            extensions: Default::default(),
        }
    }
}
