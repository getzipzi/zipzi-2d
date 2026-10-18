use super::*;
use crate::draw_order::{validate_groups, DrawOrderGroup};
use crate::types::{Canvas, ChangeKind, EditResult, ModelSetup, ModelSpace, Status};

impl Document {
    /// Reinterpret source coordinates in a new model space. This does not move
    /// authored points to compensate for the change; callers must use an explicit
    /// coordinate conversion operation when preserving evaluated positions.
    pub fn replace_model_space(&mut self, space: ModelSpace) -> EditResult {
        self.replace_model_setup(ModelSetup {
            space,
            source_canvas: self.source_canvas(),
        })
    }

    /// Source framing is metadata and never affects geometry or viewport fitting.
    pub fn replace_source_canvas(&mut self, source_canvas: Option<Canvas>) -> EditResult {
        self.replace_model_setup(ModelSetup {
            space: self.model_space(),
            source_canvas,
        })
    }

    pub fn replace_model_setup(&mut self, setup: impl Into<ModelSetup>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &self.content.id));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", &self.content.id));
        }
        let mut candidate = Document::new();
        let setup = setup.into();
        let s = candidate.initialize(self.content.id.clone(), setup);
        if !s.is_ok() {
            return self.failed(s);
        }
        if self.model_setup() == setup {
            return self.failed(Status::ok());
        }
        let geometry_changed = self.content.model_space != setup.space;
        self.content.model_space = setup.space;
        self.content.source_canvas = setup.source_canvas;
        let doc_id = self.content.id.clone();
        if geometry_changed {
            self.changed(
                ChangeKind::Structure,
                self.content.mesh_order.clone(),
                vec![doc_id],
            )
        } else {
            self.changed(ChangeKind::Metadata, Vec::new(), vec![doc_id])
        }
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
        self.content.draw_order_groups = Some(groups);
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            Vec::new(),
        )
    }
}
