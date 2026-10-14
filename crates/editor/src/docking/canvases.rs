//! Canvas instances are addressed by identity, separately from panel kinds.
//! Only one is created today. Additional instances will also need independent
//! core view state and render requests; the registry does not imply that yet.
use crate::canvas::CanvasView;
use gpui_kit::{Entity, EntityId};
use std::collections::HashMap;

pub(super) struct CanvasPanels {
    active: EntityId,
    views: HashMap<EntityId, Entity<CanvasView>>,
}

impl CanvasPanels {
    pub(super) fn new(canvas: Entity<CanvasView>) -> Self {
        Self {
            active: canvas.entity_id(),
            views: HashMap::from([(canvas.entity_id(), canvas)]),
        }
    }

    pub(super) fn active(&self) -> &Entity<CanvasView> {
        &self.views[&self.active]
    }

    pub(super) fn activate(&mut self, id: EntityId) {
        if self.views.contains_key(&id) {
            self.active = id;
        }
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &Entity<CanvasView>> {
        self.views.values()
    }
}
