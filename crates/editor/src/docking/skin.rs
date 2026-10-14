//! Compact editor dock chrome and the Parameters panel's row cache boundary.
use super::header::PanelHeaders;
use crate::t;
use editor_core::PanelKind;
use gpui_kit::assets::IconName;
use gpui_kit::base::ResizeHandleContext;
use gpui_kit::component::dock::*;
use gpui_kit::component::{
    ActiveTheme, Icon, Selectable, Sizable,
    button::{Button, ButtonVariants},
    menu::{ContextMenuExt, DropdownMenu, PopupMenuItem},
};
use gpui_kit::{
    Anchor, AnyElement, AnyView, App, Axis, Context, Div, Empty, EntityId, IntoElement, Render,
    SharedString, Stateful, TestSupportExt as _, WeakEntity, Window, div, prelude::*, px,
};
use std::{rc::Rc, sync::Arc};

const TAB_BAR_HEIGHT: f32 = 28.;

/// Component's styled Button does not expose drag initiation. Keep Base's
/// accessible button behavior and drag handling behind the same menu trigger.
#[derive(gpui_kit::IntoElement)]
struct PanelSelectorButton {
    button: gpui_kit::base::Button,
    group: TabGroupContext,
    active: PanelId,
    closable: bool,
}

impl Selectable for PanelSelectorButton {
    fn selected(mut self, selected: bool) -> Self {
        self.button = self.button.selected(selected);
        self
    }
    fn is_selected(&self) -> bool {
        self.button.is_selected()
    }
}

impl gpui_kit::InteractiveElement for PanelSelectorButton {
    fn interactivity(&mut self) -> &mut gpui_kit::Interactivity {
        self.button.interactivity()
    }
}

impl gpui_kit::Styled for PanelSelectorButton {
    fn style(&mut self) -> &mut gpui_kit::StyleRefinement {
        self.button.style()
    }
}

impl DropdownMenu for PanelSelectorButton {}

impl gpui_kit::RenderOnce for PanelSelectorButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let active = self.active;
        let closable = self.closable;
        let group = self.group;
        self.button.context_menu(move |menu, _, _| {
            menu.item(
                PopupMenuItem::element(move |_, _| {
                    div()
                        .id(SharedString::from(format!("dock-close-{active:?}")))
                        .test_support()
                        .child(t!("dock.close"))
                })
                .icon(IconName::Close)
                .disabled(!closable)
                .on_click({
                    let group = group.clone();
                    move |_, window, cx| group.close(active, window, cx)
                }),
            )
        })
    }
}

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
    headers: Rc<PanelHeaders>,
    area: WeakEntity<DockArea>,
    owner: WeakEntity<crate::EditorView>,
}

impl EditorDockSkin {
    pub(super) fn new(
        component: Rc<DockSkin>,
        row_cached_panel: EntityId,
        headers: Rc<PanelHeaders>,
        area: WeakEntity<DockArea>,
        owner: WeakEntity<crate::EditorView>,
    ) -> Rc<Self> {
        Rc::new(Self {
            component,
            row_cached_panel,
            headers,
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
            headers: self.headers.clone(),
            area: self.area.clone(),
            owner: self.owner.clone(),
        })
    }
}

struct EditorTabGroupSkin {
    component: Rc<dyn TabGroupRenderer>,
    row_cached_panel: EntityId,
    headers: Rc<PanelHeaders>,
    area: WeakEntity<DockArea>,
    owner: WeakEntity<crate::EditorView>,
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

    fn selector(&self, group: &TabGroupContext, cx: &mut App) -> AnyElement {
        let Some(active) = group.active_panel() else {
            return Empty.into_any_element();
        };
        let active_id = active.panel_id(cx);
        let name = active.panel_name(cx);
        let label = panel_label(name);
        let ix = group
            .panels()
            .iter()
            .position(|panel| panel.panel_id(cx) == active_id)
            .unwrap();
        let entries: Vec<_> = group
            .panels()
            .iter()
            .filter(|panel| panel.visible(cx))
            .map(|panel| (panel.panel_id(cx), panel.panel_name(cx)))
            .collect();
        let area = self.area.clone();
        let collapsed = group.is_collapsed();
        let owner = self.owner.clone();
        let node = group.node();
        let button = gpui_kit::base::Button::new(SharedString::from(format!(
            "dock-selector-{}",
            node.as_u64()
        )))
        .h(px(22.))
        .px(px(4.))
        .gap(px(2.))
        .flex_shrink_0()
        .rounded(px(3.))
        .hover({
            let color = cx.theme().accent;
            move |style| style.bg(color)
        })
        .child(Icon::new(panel_icon(name)).xsmall())
        .child(Icon::new(IconName::ChevronDown).xsmall())
        .accessibility_label(format!("{}: {label}", t!("dock.select_panel")))
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)
        })
        .when_some(
            group
                .is_draggable()
                .then(|| group.drag_panel(ix, cx))
                .flatten(),
            |button, drag| {
                let panel = active.clone();
                button.on_drag(drag, move |drag, offset, _, cx| {
                    cx.stop_propagation();
                    drag.set_drag_offset(offset);
                    drag.set_preview_size(gpui_kit::size(px(96.), px(TAB_BAR_HEIGHT)));
                    cx.new(|_| EditorDragPreview {
                        panel: panel.clone(),
                    })
                })
            },
        )
        .when(group.is_droppable() && !collapsed, |button| {
            button
                .drag_over::<DragPanel>(|style, _, _, cx| {
                    style.bg(cx.theme().primary.opacity(0.12))
                })
                .on_drop({
                    let group = group.clone();
                    move |drag: &DragPanel, window, cx| {
                        cx.stop_propagation();
                        group.drop_panel(drag.clone(), Some(ix), true, window, cx)
                    }
                })
        });
        let button = PanelSelectorButton {
            button,
            group: group.clone(),
            active: active_id,
            closable: group.is_panel_closable(active_id, cx),
        }
        .dropdown_menu(move |mut menu, window, cx| {
            menu = menu.check_side(gpui_kit::component::Side::Right);
            for (id, name) in &entries {
                let area = area.clone();
                let id = *id;
                let label = panel_label(name);
                let item_id: SharedString = format!("dock-select-{id:?}").into();
                menu = menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div()
                            .id(item_id.clone())
                            .test_support()
                            .child(label.clone())
                    })
                    .icon(panel_icon(name))
                    .checked(id == active_id)
                    .on_click(move |_, window, cx| {
                        _ = area.update(cx, |area, cx| {
                            // Select after reopening: toggling reconciles groups
                            // from the tree and would overwrite an earlier selection.
                            if collapsed && !area.is_dock_open(DockPlacement::Bottom) {
                                area.toggle_dock(DockPlacement::Bottom, window, cx);
                            }
                            area.select_panel(id, window, cx);
                        });
                    }),
                );
            }
            super::add_panel::menu(menu.separator(), owner.clone(), node, true, window, cx)
        });
        div()
            .id(SharedString::from(format!("dock-tab-{active_id:?}")))
            .test_support()
            .flex()
            .items_center()
            .flex_shrink_0()
            .child(button)
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
        self.component.content_frame(group, window, cx).pt_0()
    }

    fn render_tab_bar(
        &self,
        group: &TabGroupContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let Some(active) = group.active_panel() else {
            return Empty.into_any_element();
        };
        let selector = self.selector(group, cx);
        let header = (!group.is_collapsed())
            .then(|| self.headers.get(&active.view().entity_id()).cloned())
            .flatten();
        let suffix = (!group.is_collapsed())
            .then(|| PanelHandle::of(active).and_then(|h| h.title_suffix(window, cx)))
            .flatten();
        let toolbar = (!group.is_collapsed()).then(|| self.toolbar(group, window, cx));
        div()
            .id(SharedString::from(format!(
                "dock-header-{}",
                group.node().as_u64()
            )))
            .test_support()
            .h(px(TAB_BAR_HEIGHT))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(2.))
            .bg(cx.theme().tab_bar)
            .text_color(cx.theme().foreground)
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.65))
            .child(selector)
            .child(
                div()
                    .id("dock-panel-toolbar")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .children(header)
                    .children(suffix),
            )
            .children(toolbar)
            .children(self.dock_toggle(DockPlacement::Left, group, cx))
            .children(self.dock_toggle(DockPlacement::Bottom, group, cx))
            .children(self.dock_toggle(DockPlacement::Right, group, cx))
            .when(group.is_droppable() && !group.is_collapsed(), |this| {
                this.drag_over::<DragPanel>(|style, _, _, cx| {
                    style.bg(cx.theme().primary.opacity(0.1))
                })
                .on_drop({
                    let group = group.clone();
                    move |drag: &DragPanel, window, cx| {
                        group.drop_panel(drag.clone(), Some(group.panels().len()), true, window, cx)
                    }
                })
                .on_drop({
                    let group = group.clone();
                    move |item: &AnyDrag, window, cx| {
                        group.drop_item(item.clone(), None, window, cx)
                    }
                })
            })
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

fn panel_label(name: &str) -> SharedString {
    PanelKind::from_name(name)
        .and_then(|kind| kind.spec().command)
        .map(|command| SharedString::from(t!(command.spec().label).into_owned()))
        .unwrap_or_else(|| {
            if name == "Canvas" {
                t!("dock.canvas").into_owned().into()
            } else {
                name.to_owned().into()
            }
        })
}

pub(super) fn panel_icon(name: &str) -> IconName {
    use PanelKind::*;
    match PanelKind::from_name(name) {
        Some(Canvas) => IconName::Image,
        Some(MeshTree) => IconName::Layers,
        Some(Deformers) => IconName::Network,
        Some(DrawOrder) => IconName::ListOrdered,
        Some(Properties | AnimationProperties) => IconName::Settings,
        Some(Relations) => IconName::GitBranch,
        Some(Parameters | AnimationParameters) => IconName::SlidersHorizontal,
        Some(Keyforms) => IconName::Grid2x2,
        Some(BlendShapes) => IconName::Blend,
        Some(Glue) => IconName::Link,
        Some(Motions | AnimationMotions) => IconName::Film,
        Some(Expressions | AnimationExpressions) => IconName::Shapes,
        Some(Physics | AnimationPhysics) => IconName::Activity,
        Some(Pose | AnimationPose) => IconName::Bone,
        Some(Resources) => IconName::Folder,
        Some(AnimationInfo) => IconName::Info,
        Some(AnimationTimeline) => IconName::ChartLine,
        None => IconName::LayoutGrid,
    }
}

#[cfg(test)]
#[path = "../../headless_tests/view_cache.rs"]
mod tests;
