//! Compact editor dock chrome and the Parameters panel's row cache boundary.
use crate::t;
use gpui_kit::base::{ResizeHandleContext, Tab, Tabs};
use gpui_kit::component::dock::*;
use gpui_kit::component::{
    ActiveTheme, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
};
use gpui_kit::{
    Anchor, AnyElement, AnyView, App, Axis, Context, Div, Empty, EntityId, FontWeight, IntoElement,
    Render, ScrollHandle, SharedString, Stateful, TestSupportExt as _, WeakEntity, Window, div,
    prelude::*, px,
};
use std::{cell::Cell, rc::Rc, sync::Arc};

const TAB_BAR_HEIGHT: f32 = 28.;

struct EditorDragPreview {
    panel: Arc<dyn BasePanelView>,
}

impl Render for EditorDragPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = PanelHandle::of(&self.panel)
            .map(|handle| handle.title(window, cx))
            .unwrap_or_else(|| SharedString::from(self.panel.panel_name(cx)).into_any_element());
        div()
            .h(px(TAB_BAR_HEIGHT))
            .w(px(96.))
            .px(px(8.))
            .flex()
            .items_center()
            .text_xs()
            .overflow_hidden()
            .whitespace_nowrap()
            .bg(cx.theme().tab_active)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(4.))
            .opacity(0.9)
            .child(title)
    }
}

pub(super) struct EditorDockSkin {
    component: Rc<DockSkin>,
    row_cached_panel: EntityId,
    area: WeakEntity<DockArea>,
    owner: WeakEntity<crate::EditorView>,
}

impl EditorDockSkin {
    pub(super) fn new(
        component: Rc<DockSkin>,
        row_cached_panel: EntityId,
        area: WeakEntity<DockArea>,
        owner: WeakEntity<crate::EditorView>,
    ) -> Rc<Self> {
        Rc::new(Self {
            component,
            row_cached_panel,
            area,
            owner,
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
            area: self.area.clone(),
            owner: self.owner.clone(),
            scroll_handle: ScrollHandle::default(),
            last_displayed: Cell::new(None),
        })
    }
}

struct EditorTabGroupSkin {
    component: Rc<dyn TabGroupRenderer>,
    row_cached_panel: EntityId,
    area: WeakEntity<DockArea>,
    owner: WeakEntity<crate::EditorView>,
    scroll_handle: ScrollHandle,
    last_displayed: Cell<Option<PanelId>>,
}

impl EditorTabGroupSkin {
    fn dock_toggle(
        &self,
        placement: DockPlacement,
        group: &TabGroupContext,
        cx: &App,
    ) -> Option<Button> {
        if group.is_zoomed() {
            return None;
        }
        let area = self.area.upgrade()?;
        let area = area.read(cx);
        if !area.is_dock_collapsible(placement) {
            return None;
        }
        fn top_group(node: &PaneNode, right: bool) -> Option<NodeId> {
            match node.kind() {
                PaneRef::Tabs { .. } => Some(node.id()),
                PaneRef::Split { axis, children, .. } => {
                    let child = if right && axis == Axis::Horizontal {
                        children.last()
                    } else {
                        children.first()
                    };
                    child.and_then(|child| top_group(child, right))
                }
            }
        }
        let root = if placement == DockPlacement::Bottom {
            placement
        } else {
            DockPlacement::Center
        };
        let designated = area
            .layout(root)
            .and_then(|tree| top_group(tree.root(), placement == DockPlacement::Right));
        if designated != Some(group.node()) {
            return None;
        }
        let open = area.is_dock_open(placement);
        let icon = match (placement, open) {
            (DockPlacement::Left, true) => IconName::PanelLeft,
            (DockPlacement::Left, false) => IconName::PanelLeftOpen,
            (DockPlacement::Right, true) => IconName::PanelRight,
            (DockPlacement::Right, false) => IconName::PanelRightOpen,
            (DockPlacement::Bottom, true) => IconName::PanelBottom,
            (DockPlacement::Bottom, false) => IconName::PanelBottomOpen,
            _ => return None,
        };
        let area = self.area.clone();
        Some(
            Button::new(SharedString::from(format!("dock-toggle-{placement:?}")))
                .icon(icon)
                .xsmall()
                .ghost()
                .tab_stop(false)
                .tooltip(if open {
                    t!("dock.collapse")
                } else {
                    t!("dock.expand")
                })
                .on_click(move |_, window, cx| {
                    _ = area.update(cx, |area, cx| area.toggle_dock(placement, window, cx));
                }),
        )
    }

    fn toolbar(&self, group: &TabGroupContext, window: &mut Window, cx: &mut App) -> AnyElement {
        let handle = group.active_panel().and_then(PanelHandle::of);
        let control = group
            .active_panel()
            .filter(|panel| panel.zoomable(cx))
            .and_then(|_| handle.and_then(|handle| handle.zoom_control(cx)));
        let zoomed = group.is_zoomed();
        let panel = handle.map(PanelHandle::panel);
        let closable = group.is_closable();
        let owner = self.owner.clone();
        let node = group.node();
        let zoom_title = if zoomed {
            t!("dock.restore")
        } else {
            t!("dock.maximize")
        };
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .flex_shrink_0()
            .occlude()
            .children(
                handle
                    .and_then(|handle| handle.toolbar_buttons(window, cx))
                    .into_iter()
                    .flatten()
                    .map(|button| button.xsmall().ghost().tab_stop(false)),
            )
            .when(
                zoomed || control.is_some_and(|control| control.toolbar_visible()),
                |this| {
                    this.child(
                        Button::new("dock-zoom")
                            .icon(if zoomed {
                                IconName::Minimize
                            } else {
                                IconName::Maximize
                            })
                            .xsmall()
                            .ghost()
                            .tab_stop(false)
                            .selected(zoomed)
                            .tooltip(zoom_title.clone())
                            .on_click({
                                let group = group.clone();
                                move |_, window, cx| group.toggle_zoom(window, cx)
                            }),
                    )
                },
            )
            .child(
                Button::new(SharedString::from(format!("dock-menu-{}", node.as_u64())))
                    .icon(IconName::Ellipsis)
                    .xsmall()
                    .ghost()
                    .tab_stop(false)
                    .tooltip(t!("dock.panel_actions"))
                    .dropdown_menu(move |menu, window, cx| {
                        let menu = menu.when_some(panel.clone(), |menu, panel| {
                            panel.dropdown_menu(menu, window, cx)
                        });
                        super::add_panel::menu(menu, owner.clone(), node, true, window, cx)
                            .separator()
                            .menu_with_disabled(
                                zoom_title.clone(),
                                Box::new(ToggleZoom),
                                !zoomed && !control.is_some_and(|control| control.menu_visible()),
                            )
                            .when(closable, |menu| {
                                menu.separator()
                                    .menu(t!("dock.close"), Box::new(ClosePanel))
                            })
                    })
                    .anchor(Anchor::TopRight),
            )
            .into_any_element()
    }

    fn tab(
        &self,
        group: &TabGroupContext,
        ix: usize,
        position: usize,
        count: usize,
        single: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let panel = &group.panels()[ix];
        let handle = PanelHandle::of(panel);
        let panel_id = panel.panel_id(cx);
        let closable = !group.is_collapsed() && group.is_panel_closable(panel_id, cx);
        let active = !group.is_collapsed()
            && group
                .active_panel()
                .is_some_and(|panel| panel.panel_id(cx) == panel_id);
        let title = match handle {
            Some(handle) => handle
                .tab_name(cx)
                .map(IntoElement::into_any_element)
                .unwrap_or_else(|| handle.title(window, cx)),
            None => SharedString::from(panel.panel_name(cx)).into_any_element(),
        };
        let theme = cx.theme();
        let tab_id: SharedString = format!("dock-tab-{panel_id:?}").into();
        Tab::new(tab_id)
            .selected(active)
            .set_position(position + 1, count)
            .h_full()
            .min_w_0()
            .flex_shrink_0()
            .gap(px(4.))
            .px(px(8.))
            .relative()
            .text_xs()
            .cursor_pointer()
            .text_color(if active {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .hover(move |style| style.text_color(theme.foreground))
            .child(
                div()
                    .min_w_0()
                    .max_w(px(144.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .font_weight(if active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .child(title),
            )
            .when(active && !single, |this| {
                this.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left(px(8.))
                        .right(px(8.))
                        .h(px(2.))
                        .rounded_full()
                        .bg(theme.primary),
                )
            })
            .on_click({
                let group = group.clone();
                let area = self.area.clone();
                move |_, window, cx| {
                    group.select_tab(ix, window, cx);
                    if group.is_collapsed() {
                        _ = area.update(cx, |area, cx| {
                            if !area.is_dock_open(DockPlacement::Bottom) {
                                area.toggle_dock(DockPlacement::Bottom, window, cx);
                            }
                        });
                    }
                }
            })
            .when(!group.is_collapsed(), |this| {
                this.when_some(
                    group
                        .is_draggable()
                        .then(|| group.drag_panel(ix, cx))
                        .flatten(),
                    |this, drag| {
                        this.on_drag(drag, {
                            let panel = panel.clone();
                            move |drag, offset, _, cx| {
                                cx.stop_propagation();
                                drag.set_drag_offset(offset);
                                drag.set_preview_size(gpui_kit::size(px(96.), px(TAB_BAR_HEIGHT)));
                                cx.new(|_| EditorDragPreview {
                                    panel: panel.clone(),
                                })
                            }
                        })
                    },
                )
                .when(group.is_droppable(), |this| {
                    this.drag_over::<DragPanel>(|style, _, _, cx| {
                        style
                            .bg(cx.theme().primary.opacity(0.12))
                            .border_l_2()
                            .border_color(cx.theme().drag_border)
                    })
                    .on_drop({
                        let group = group.clone();
                        move |drag: &DragPanel, window, cx| {
                            group.drop_panel(drag.clone(), Some(ix), true, window, cx)
                        }
                    })
                    .drag_over::<AnyDrag>(|style, _, _, cx| {
                        style.bg(cx.theme().primary.opacity(0.12))
                    })
                    .on_drop({
                        let group = group.clone();
                        move |item: &AnyDrag, window, cx| {
                            group.drop_item(item.clone(), None, window, cx)
                        }
                    })
                })
            })
            .context_menu({
                let group = group.clone();
                move |menu, _, _| {
                    menu.item(
                        PopupMenuItem::new(t!("dock.close"))
                            .icon(IconName::Close)
                            .disabled(!closable)
                            .on_click({
                                let group = group.clone();
                                move |_, window, cx| group.close(panel_id, window, cx)
                            }),
                    )
                }
            })
            .into_any_element()
    }
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
        let visible: Vec<_> = group
            .panels()
            .iter()
            .enumerate()
            .filter(|(_, panel)| panel.visible(cx))
            .map(|(ix, _)| ix)
            .collect();
        if visible.is_empty() {
            return Empty.into_any_element();
        }
        let single = visible.len() == 1;
        if single
            && group
                .active_panel()
                .and_then(PanelHandle::of)
                .is_some_and(|handle| !handle.title_bar(cx))
        {
            return Empty.into_any_element();
        }
        let displayed = group.active_panel().map(|panel| panel.panel_id(cx));
        if self.last_displayed.replace(displayed) != displayed {
            if let Some(position) = visible
                .iter()
                .position(|ix| Some(group.panels()[*ix].panel_id(cx)) == displayed)
            {
                self.scroll_handle.scroll_to_item(position);
            }
        }
        let left = self.dock_toggle(DockPlacement::Left, group, cx);
        let bottom = self.dock_toggle(DockPlacement::Bottom, group, cx);
        let right = self.dock_toggle(DockPlacement::Right, group, cx);
        let tabs: Vec<_> = visible
            .iter()
            .enumerate()
            .map(|(position, &ix)| self.tab(group, ix, position, visible.len(), single, window, cx))
            .collect();
        let suffix = (!group.is_collapsed())
            .then(|| {
                group
                    .active_panel()
                    .and_then(PanelHandle::of)
                    .and_then(|handle| handle.title_suffix(window, cx))
            })
            .flatten();
        let toolbar = (!group.is_collapsed()).then(|| self.toolbar(group, window, cx));
        let droppable = group.is_droppable() && !group.is_collapsed();
        let count = group.panels().len();
        div()
            .id("editor-dock-tab-bar")
            .test_support()
            .h(px(TAB_BAR_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(cx.theme().tab_bar)
            .text_color(cx.theme().foreground)
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.65))
            .children(left)
            .children(bottom)
            .child(
                Tabs::new("editor-dock-tabs")
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .overflow_x_hidden()
                    .child(
                        div()
                            .id("editor-dock-tabs-scroll")
                            .test_support()
                            .h_full()
                            .w_full()
                            .flex()
                            .overflow_x_scroll()
                            .track_scroll(&self.scroll_handle)
                            .children(tabs)
                            .child(
                                div()
                                    .id("dock-tab-empty-space")
                                    .h_full()
                                    .flex_1()
                                    .min_w(px(12.))
                                    .when(droppable, |this| {
                                        this.drag_over::<DragPanel>(|style, _, _, cx| {
                                            style.bg(cx.theme().primary.opacity(0.1))
                                        })
                                        .on_drop({
                                            let group = group.clone();
                                            move |drag: &DragPanel, window, cx| {
                                                let ix = (drag.source() == group.node())
                                                    .then_some(count - 1);
                                                group.drop_panel(
                                                    drag.clone(),
                                                    ix,
                                                    false,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        })
                                        .drag_over::<AnyDrag>(|style, _, _, cx| {
                                            style.bg(cx.theme().primary.opacity(0.1))
                                        })
                                        .on_drop({
                                            let group = group.clone();
                                            move |item: &AnyDrag, window, cx| {
                                                group.drop_item(item.clone(), None, window, cx)
                                            }
                                        })
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .gap(px(2.))
                    .px(px(4.))
                    .children(suffix)
                    .children(toolbar)
                    .children(right),
            )
            .into_any_element()
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
