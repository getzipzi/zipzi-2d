use super::*;
use crate::draw_order::{validate_groups, DrawOrderGroup};
use crate::types::{Canvas, ChangeKind, EditResult, ExportSettings, ModelSetup, Status};

impl Document {
    /// Source framing is provenance and never affects geometry or output policy.
    pub fn replace_source_canvas(&mut self, source_canvas: Option<Canvas>) -> EditResult {
        self.replace_model_setup(ModelSetup {
            source_canvas,
            export: self.export_settings(),
        })
    }

    pub fn replace_export_settings(&mut self, export: ExportSettings) -> EditResult {
        self.replace_model_setup(ModelSetup {
            source_canvas: self.source_canvas(),
            export,
        })
    }

    pub fn replace_model_setup(&mut self, setup: impl Into<ModelSetup>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &self.content.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &self.content.id));
        }
        let setup = setup.into();
        if !setup.is_valid() {
            return self.failed(Status::error(
                "INVALID_MODEL_SETUP",
                "Invalid source metadata or export settings",
            ));
        }
        if self.model_setup() == setup {
            return self.failed(Status::ok());
        }
        self.content.source_canvas = setup.source_canvas;
        self.content.export_settings = setup.export;
        self.changed(
            ChangeKind::Metadata,
            Vec::new(),
            vec![self.content.id.clone()],
        )
    }

    pub fn draw_order_groups(&self) -> Option<&[DrawOrderGroup]> {
        self.content.draw_order_groups.as_deref()
    }

    pub fn replace_draw_order_groups(&mut self, groups: Vec<DrawOrderGroup>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let groups = match validate_groups(self, &groups) {
            Ok(groups) => groups,
            Err(status) => return self.failed(status),
        };
        if self.content.draw_order_groups.as_ref() == Some(&groups) {
            return self.failed(Status::ok());
        }
        let previous = crate::draw_order::resolved_groups(self);
        for group in previous.iter().chain(&groups) {
            let a = previous.iter().find(|g| g.owner == group.owner);
            let b = groups.iter().find(|g| g.owner == group.owner);
            if a != b {
                for id in std::iter::once(&group.owner).chain(group.items.iter()) {
                    if let Err(status) = self.authoring_unlocked(id) {
                        return self.failed(status);
                    }
                }
            }
        }
        self.content.draw_order_groups = Some(groups);
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            Vec::new(),
        )
    }
}
