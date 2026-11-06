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
        self.validation_model().validate_model3_settings(settings)
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
