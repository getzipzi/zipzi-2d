//! Keep the Parameters panel's cache boundary at its visible rows.
use gpui_kit::base::ResizeHandleContext;
use gpui_kit::component::dock::*;
use gpui_kit::{
    AnyElement, AnyView, App, Axis, Div, Empty, EntityId, IntoElement, Stateful, Window, div,
    prelude::*,
};
use std::{rc::Rc, sync::Arc};

pub(super) struct EditorDockSkin {
    component: Rc<DockSkin>,
    row_cached_panel: EntityId,
}

impl EditorDockSkin {
    pub(super) fn new(component: Rc<DockSkin>, row_cached_panel: EntityId) -> Rc<Self> {
        Rc::new(Self {
            component,
            row_cached_panel,
        })
    }
}

impl DockAreaRenderer for EditorDockSkin {
    fn frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.component.frame(window, cx)
    }

    fn center_frame(&self, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.component.center_frame(window, cx)
    }

    fn split_frame(
        &self,
        node: NodeId,
        axis: Axis,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        self.component.split_frame(node, axis, window, cx)
    }

    fn render_split_handle(
        &self,
        handle: &ResizeHandleContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.component.render_split_handle(handle, window, cx)
    }

    fn render_dock(
        &self,
        dock: &DockContext,
        content: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.component.render_dock(dock, content, window, cx)
    }

    fn build_placeholder(
        &self,
        state: &PanelState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Arc<dyn BasePanelView>> {
        self.component.build_placeholder(state, window, cx)
    }

    fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
        Rc::new(EditorTabGroupSkin {
            component: self.component.tab_group_renderer(),
            row_cached_panel: self.row_cached_panel,
        })
    }
}

struct EditorTabGroupSkin {
    component: Rc<dyn TabGroupRenderer>,
    row_cached_panel: EntityId,
}

impl TabGroupRenderer for EditorTabGroupSkin {
    fn frame(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> Stateful<Div> {
        self.component.frame(group, window, cx)
    }

    fn content_frame(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Stateful<Div> {
        self.component.content_frame(group, window, cx)
    }

    fn render_tab_bar(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        self.component.render_tab_bar(group, window, cx)
    }

    fn render_active_panel(
        &self,
        panel: AnyView,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if panel.entity_id() != self.row_cached_panel {
            return self.component.render_active_panel(panel, group, window, cx);
        }
        if group.is_collapsed() {
            return Empty.into_any_element();
        }
        // An outer view cache would force every child cache to refresh whenever
        // one parameter row changes. Keep the component skin's sizing and scroll
        // region, and let the panel's virtualized rows own the view caches.
        div()
            .id("tab-content")
            .overflow_y_scroll()
            .overflow_x_hidden()
            .flex_1()
            .child(div().absolute().size_full().child(panel))
            .into_any_element()
    }

    fn render_drop_indicator(
        &self,
        indicator: DropIndicator,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.component.render_drop_indicator(indicator, window, cx)
    }

    fn render_empty(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.component.render_empty(group, window, cx)
    }
}

#[cfg(test)]
#[path = "../../headless_tests/view_cache.rs"]
mod tests;
