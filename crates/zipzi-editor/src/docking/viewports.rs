//! Concrete panel identity and shared input surface identity are distinct.
use crate::viewports::{AnimationViewport, ModelingViewport, ViewportSurface};
use gpui_kit::component::dock::{BasePanelView, PanelId, panel_handle};
use gpui_kit::{App, Entity, EntityId};
use std::{collections::HashMap, sync::Arc};
use zipzi_editor_core::{ViewportId, ViewportKind, WorkspaceKind};

pub(super) struct ViewportPanel {
    pub id: ViewportId,
    pub panel_id: PanelId,
    pub kind: ViewportKind,
    pub surface: Entity<ViewportSurface>,
    pub panel: Arc<dyn BasePanelView>,
}
pub(super) struct ViewportPanels {
    active: EntityId,
    views: HashMap<EntityId, ViewportPanel>,
}
impl ViewportPanels {
    pub fn new(
        modeling: Entity<ModelingViewport>,
        animation: Entity<AnimationViewport>,
        cx: &App,
    ) -> Self {
        let modeling_surface = modeling.read(cx).surface.clone();
        let animation_surface = animation.read(cx).surface.clone();
        let active = modeling_surface.entity_id();
        let views = [
            ViewportPanel {
                id: modeling_surface.read(cx).viewport,
                panel_id: PanelId::from(modeling.entity_id()),
                kind: ViewportKind::Modeling,
                surface: modeling_surface,
                panel: panel_handle(modeling),
            },
            ViewportPanel {
                id: animation_surface.read(cx).viewport,
                panel_id: PanelId::from(animation.entity_id()),
                kind: ViewportKind::Animation,
                surface: animation_surface,
                panel: panel_handle(animation),
            },
        ]
        .into_iter()
        .map(|view| (view.surface.entity_id(), view))
        .collect();
        Self { active, views }
    }
    pub fn active(&self, workspace: WorkspaceKind) -> &ViewportPanel {
        self.views
            .get(&self.active)
            .filter(|view| view.kind == ViewportKind::from(workspace))
            .unwrap_or_else(|| {
                self.views
                    .values()
                    .find(|view| view.kind == ViewportKind::from(workspace))
                    .unwrap()
            })
    }
    pub fn activate(&mut self, id: EntityId) {
        if self.views.contains_key(&id) {
            self.active = id;
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &ViewportPanel> {
        self.views.values()
    }
    pub fn panel_id(&self, workspace: WorkspaceKind) -> PanelId {
        self.active(workspace).panel_id
    }
}
