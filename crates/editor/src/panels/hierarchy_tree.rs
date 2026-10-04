mod filter;

use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, TreeNode, TreeNodeKind};
use filter::{TreeFilter, VisibleRow, highlight_ranges, node_key, visible_rows};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    dock::{BasePanel, Panel, PanelEvent},
    input::{Input, InputEvent, InputState},
    popover::Popover,
};
use gpui_kit::{prelude::*, *};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

#[derive(Clone, Default)]
struct TreeViewState {
    collapsed: HashSet<String>,
    filter: TreeFilter,
}

pub struct HierarchyTreePanel<const DEFORMER: bool> {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
    _thumbnails_observe: Subscription,
    view: TreeViewState,
    document_views: HashMap<DocumentId, TreeViewState>,
    filtered_collapsed: HashSet<String>,
    filter_open: bool,
    search: Entity<InputState>,
    search_placeholder: String,
    _search_subscription: Subscription,
    document_id: Option<DocumentId>,
    revision: Option<u64>,
    last_selection: Option<editor_core::ObjectRef>,
    nodes: Vec<TreeNode>,
    rows: Rc<Vec<VisibleRow>>,
    rows_dirty: bool,
    scroll: UniformListScrollHandle,
}

impl<const DEFORMER: bool> HierarchyTreePanel<DEFORMER> {
    pub fn new(
        owner: WeakEntity<EditorView>,
        thumbnails: Entity<crate::mesh_thumbnails::MeshThumbnails>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            let observe = cx.observe(&owner.upgrade().unwrap(), |_, _, cx| {
                cx.notify();
            });
            let thumbnails_observe = cx.observe(&thumbnails, |_, _, cx| cx.notify());
            let search_placeholder = t!("editor.tree_search").into_owned();
            let search =
                cx.new(|cx| InputState::new(window, cx).placeholder(search_placeholder.clone()));
            let search_subscription =
                cx.subscribe(&search, |this: &mut Self, search, event, cx| {
                    if let InputEvent::Change = event {
                        if this.owner.upgrade().is_none_or(|owner| {
                            owner.read(cx).editor.active_id() != this.document_id
                        }) {
                            return;
                        }
                        let query = search.read(cx).value().to_string();
                        if this.view.filter.query != query {
                            this.view.filter.query = query;
                            this.filter_changed(cx);
                        }
                    }
                });
            Self {
                owner,
                focus,
                _observe: observe,
                _thumbnails_observe: thumbnails_observe,
                view: TreeViewState::default(),
                document_views: HashMap::new(),
                filtered_collapsed: HashSet::new(),
                filter_open: false,
                search,
                search_placeholder,
                _search_subscription: search_subscription,
                document_id: None,
                revision: None,
                last_selection: None,
                nodes: Vec::new(),
                rows: Rc::new(Vec::new()),
                rows_dirty: true,
                scroll: UniformListScrollHandle::new(),
            }
        })
    }

    fn prefix() -> &'static str {
        if DEFORMER { "deformer" } else { "mesh" }
    }

    fn object_types() -> &'static [TreeNodeKind] {
        use TreeNodeKind::*;
        if DEFORMER {
            &[Mesh, WarpDeformer, RotationDeformer]
        } else {
            &[Part, Mesh, WarpDeformer, RotationDeformer, Offscreen]
        }
    }

    fn collapsed(&self) -> &HashSet<String> {
        if self.view.filter.is_active() {
            &self.filtered_collapsed
        } else {
            &self.view.collapsed
        }
    }

    fn filter_changed(&mut self, cx: &mut Context<Self>) {
        self.filtered_collapsed.clear();
        self.rows_dirty = true;
        self.scroll = UniformListScrollHandle::new();
        cx.notify();
    }

    fn clear_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.view.filter = TreeFilter::default();
        self.search
            .update(cx, |search, cx| search.set_value("", window, cx));
        self.filter_changed(cx);
    }

    fn toggle_expanded(&mut self, key: &str, cx: &mut Context<Self>) {
        let collapsed = if self.view.filter.is_active() {
            &mut self.filtered_collapsed
        } else {
            &mut self.view.collapsed
        };
        if !collapsed.insert(key.to_owned()) {
            collapsed.remove(key);
        }
        self.rows_dirty = true;
        cx.notify();
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let prefix = Self::prefix();
        let active = self.view.filter.is_active();
        let type_active = !self.view.filter.excluded.is_empty();
        let target = self.document_id;
        let panel = cx.weak_entity();
        div()
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .flex_shrink_0()
            .child(
                Input::new(&self.search)
                    .id(SharedString::from(format!("{prefix}-tree-search")))
                    .aria_label(t!("editor.tree_search"))
                    .small()
                    .h(px(26.))
                    .flex_1()
                    .min_w_0()
                    .prefix(
                        Icon::new(IconName::Search)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .when(active, |input| {
                        input.suffix(
                            Button::new(SharedString::from(format!("{prefix}-tree-clear")))
                                .icon(IconName::Close)
                                .ghost()
                                .xsmall()
                                .tooltip(t!("editor.tree_clear"))
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.clear_filter(window, cx)
                                    }),
                                ),
                        )
                    }),
            )
            .child(
                Popover::new(SharedString::from(format!("{prefix}-tree-filter-popover")))
                    .p_2()
                    .open(self.filter_open)
                    .on_open_change(cx.listener(|this, open, _, cx| {
                        this.filter_open = *open;
                        cx.notify();
                    }))
                    .trigger(
                        Button::new(SharedString::from(format!("{prefix}-tree-filter")))
                            .icon(IconName::ListFilter)
                            .ghost()
                            .small()
                            .h(px(26.))
                            .selected(type_active)
                            .when(type_active, |button| {
                                button.label(
                                    (Self::object_types().len() - self.view.filter.excluded.len())
                                        .to_string(),
                                )
                            })
                            .tooltip(t!("editor.tree_filter_types")),
                    )
                    .content(move |_, _, cx| {
                        let Some(panel) = panel.upgrade() else {
                            return div();
                        };
                        let filter = panel.read(cx).view.filter.clone();
                        let popover = cx.weak_entity();
                        let reset_panel = panel.downgrade();
                        let reset_popover = popover.clone();
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .w(px(172.))
                            .text_xs()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(t!("editor.tree_filter_types"))
                                    .child(
                                        Button::new("tree-all-types")
                                            .label(t!("editor.tree_all_types"))
                                            .ghost()
                                            .xsmall()
                                            .disabled(filter.excluded.is_empty())
                                            .on_click(move |_, _, cx| {
                                                _ = reset_panel.update(cx, |this, cx| {
                                                    if this.document_id != target {
                                                        return;
                                                    }
                                                    this.view.filter.excluded.clear();
                                                    this.filter_changed(cx);
                                                });
                                                _ = reset_popover.update(cx, |_, cx| cx.notify());
                                            }),
                                    ),
                            )
                            .children(Self::object_types().iter().map(|&kind| {
                                let panel = panel.downgrade();
                                let popover = popover.clone();
                                let label = match kind {
                                    TreeNodeKind::Part => t!("editor.tree_type_part"),
                                    TreeNodeKind::Mesh => t!("editor.tree_type_mesh"),
                                    TreeNodeKind::WarpDeformer => t!("editor.tree_type_warp"),
                                    TreeNodeKind::RotationDeformer => {
                                        t!("editor.tree_type_rotation")
                                    }
                                    TreeNodeKind::Offscreen => t!("editor.tree_type_offscreen"),
                                    TreeNodeKind::Group => unreachable!(),
                                };
                                Checkbox::new(SharedString::from(format!("tree-kind-{kind:?}")))
                                    .label(label.into_owned())
                                    .small()
                                    .h(px(24.))
                                    .w_full()
                                    .checked(filter.includes(kind))
                                    .on_change(move |checked, _, cx| {
                                        _ = panel.update(cx, |this, cx| {
                                            if this.document_id != target {
                                                return;
                                            }
                                            this.view.filter.set_included(kind, *checked);
                                            this.filter_changed(cx);
                                        });
                                        _ = popover.update(cx, |_, cx| cx.notify());
                                    })
                            }))
                    }),
            )
    }
}

impl<const DEFORMER: bool> EventEmitter<PanelEvent> for HierarchyTreePanel<DEFORMER> {}
impl<const DEFORMER: bool> Focusable for HierarchyTreePanel<DEFORMER> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl<const DEFORMER: bool> BasePanel for HierarchyTreePanel<DEFORMER> {
    fn panel_name(&self) -> &'static str {
        if DEFORMER { "Deformers" } else { "MeshTree" }
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl<const DEFORMER: bool> Panel for HierarchyTreePanel<DEFORMER> {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if DEFORMER {
            t!("editor.deformer_tree")
        } else {
            t!("editor.mesh_tree")
        }
    }
}

impl<const DEFORMER: bool> Render for HierarchyTreePanel<DEFORMER> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let (target, revision, selected, selections, disabled, thumbnails) = {
            let state = owner.read(cx);
            (
                state.editor.active_id(),
                state.editor.tree_revision(),
                state
                    .editor
                    .document()
                    .and_then(|d| d.context().selection.clone()),
                state
                    .editor
                    .document()
                    .map(|d| d.context().selections.clone())
                    .unwrap_or_default(),
                state.modal || state.editor.is_busy(),
                state.mesh_thumbnails.clone(),
            )
        };
        let document_changed = self.document_id != target;
        if document_changed {
            if let Some(previous) = self.document_id {
                self.document_views.insert(previous, self.view.clone());
            }
            self.view = target
                .and_then(|id| self.document_views.get(&id).cloned())
                .unwrap_or_default();
            self.document_id = target;
            self.filtered_collapsed.clear();
            self.filter_open = false;
            self.search.update(cx, |search, cx| {
                search.set_value(self.view.filter.query.clone(), window, cx);
            });
            self.scroll = UniformListScrollHandle::new();
        }
        let placeholder = t!("editor.tree_search");
        if self.search_placeholder != placeholder {
            self.search_placeholder = placeholder.to_string();
            self.search.update(cx, |search, cx| {
                search.set_placeholder(placeholder, window, cx)
            });
        }
        if document_changed || self.revision != revision {
            self.revision = revision;
            self.nodes = if DEFORMER {
                owner.read(cx).editor.deformer_tree()
            } else {
                owner.read(cx).editor.mesh_tree()
            };
            self.rows_dirty = true;
        }
        let selection_changed = self.last_selection != selected || document_changed;
        if selection_changed {
            fn reveal(
                nodes: &[TreeNode],
                target: &editor_core::ObjectRef,
                collapsed: &mut HashSet<String>,
            ) -> bool {
                for node in nodes {
                    if node.reference().as_ref() == Some(target) {
                        return true;
                    }
                    if reveal(&node.children, target, collapsed) {
                        collapsed.remove(&node_key(node));
                        return true;
                    }
                }
                false
            }
            if let Some(r) = &selected {
                let collapsed = if self.view.filter.is_active() {
                    &mut self.filtered_collapsed
                } else {
                    &mut self.view.collapsed
                };
                reveal(&self.nodes, r, collapsed);
            }
            self.last_selection = selected.clone();
            self.rows_dirty = true;
        }
        if self.rows_dirty {
            self.rows = Rc::new(visible_rows(
                &self.nodes,
                self.collapsed(),
                &self.view.filter,
            ));
            self.rows_dirty = false;
        }
        if selection_changed {
            if let Some(i) = self.rows.iter().position(|r| {
                r.node.reference().as_ref() == selected.as_ref() && selected.is_some()
            }) {
                self.scroll.scroll_to_item(i, ScrollStrategy::Center);
            }
        }
        let rows = self.rows.clone();
        let panel = cx.entity();
        let query = self.view.filter.query();
        let filtered = self.view.filter.is_active();
        thumbnails.update(cx, |cache, cx| cache.sync_document(target, window, cx));
        let theme = cx.theme().clone();

        div()
            .id(if DEFORMER {
                "deformer-tree-panel"
            } else {
                "mesh-tree-panel"
            })
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .track_focus(&self.focus)
            .child(self.toolbar(cx))
            .child(if rows.is_empty() {
                div()
                    .id(SharedString::from(format!("{}-tree-empty", Self::prefix())))
                    .test_support()
                    .p_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if filtered {
                        t!("editor.tree_no_matches")
                    } else if DEFORMER {
                        t!("editor.no_deformers")
                    } else {
                        t!("editor.no_meshes")
                    })
                    .into_any_element()
            } else {
                uniform_list(
                    if DEFORMER { "deformer-tree-scroll-area" } else { "mesh-tree-scroll-area" },
                    rows.len(),
                    move |range, window, cx| panel.update(cx, |this, cx| {
                        let mesh_ids: Vec<_> = rows[range.clone()].iter()
                            .filter(|row| row.node.kind == TreeNodeKind::Mesh)
                            .map(|row| row.node.id.as_str()).collect();
                        let mut images = thumbnails.update(cx, |cache, cx| cache.request(&owner, &mesh_ids, window, cx)).into_iter();
                        let theme = cx.theme().clone();
                        range.map(|index| {
                        let row = rows[index].clone();
                        let node = row.node;
                        let is_mesh = node.kind == TreeNodeKind::Mesh;
                        let thumbnail = if is_mesh { images.next().flatten() } else { None };
                        let reference = node.reference();
                        let is_selected = reference.as_ref().is_some_and(|r| selections.contains(r));
                        let key = node_key(&node);
                        let is_collapsed = this.collapsed().contains(&key);
                        let id = node.id.clone();
                        let owner = this.owner.clone();
                        let owner_for_vis = owner.clone();
                        let id_for_vis = id.clone();
                        let icon = match node.kind {
                            TreeNodeKind::Group | TreeNodeKind::Part => IconName::Folder,
                            TreeNodeKind::Offscreen => IconName::Layers,
                            TreeNodeKind::Mesh => IconName::Grid2x2,
                            TreeNodeKind::WarpDeformer => IconName::Grid3x3,
                            TreeNodeKind::RotationDeformer => IconName::RotateCw,
                        };
                        let label = if node.name.is_empty() {
                            if is_mesh {
                                t!("editor.untitled_layer").into_owned()
                            } else {
                                node.id.clone()
                            }
                        } else {
                            node.name
                        };
                        let label = if !query.is_empty() && highlight_ranges(&label, &query).is_empty() && !highlight_ranges(&id, &query).is_empty() {
                            format!("{label} · {id}")
                        } else {
                            label
                        };

                        div()
                            .id(SharedString::from(format!(
                                "{}-item-{key}",
                                if DEFORMER { "deformer" } else { "mesh" }
                            )))
                            .flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .h(px(26.))
                            .pl(px(4. + row.depth as f32 * 10.))
                            .pr_2()
                            .rounded_md()
                            .bg(if is_selected {
                                theme.primary.opacity(0.15)
                            } else {
                                gpui::transparent_black()
                            })
                            .hover(|style| style.bg(if is_selected {
                                theme.primary.opacity(0.22)
                            } else {
                                theme.muted.opacity(0.5)
                            }))
                            .cursor_pointer()
                            .test_support()
                            .on_click(cx.listener(move |_, event: &ClickEvent, window, cx| {
                                if disabled { return; }
                                let (Some(owner), Some(target), Some(reference)) = (owner.upgrade(), target, reference.clone()) else { return; };
                                owner.update(cx, |owner, cx| {
                                    if owner.modal || window.has_active_dialog(cx) || owner.editor.is_busy() || owner.editor.active_id() != Some(target) { return; }
                                    let command = if event.modifiers().shift {
                                        let mut objects = owner.editor.document().map(|d|d.context().selections.clone()).unwrap_or_default();
                                        if objects.contains(&reference) {objects.retain(|r|r!=&reference);} else {objects.push(reference);}
                                        EditorCommand::SelectObjects(objects)
                                    } else {EditorCommand::SelectObject(reference)};
                                    let result = owner.editor.execute_for(target, command);
                                    owner.complete_edit(result, cx);
                                });
                            }))
                            .child(div().id(SharedString::from(format!("tree-expand-{key}"))).test_support().w(px(16.)).on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if row.has_children { this.toggle_expanded(&key, cx); }
                            })).text_color(theme.muted_foreground).when(
                                row.has_children,
                                |element| {
                                    element.child(
                                        Icon::new(if is_collapsed {
                                            IconName::ChevronRight
                                        } else {
                                            IconName::ChevronDown
                                        })
                                        .small(),
                                    )
                                },
                            ))
                            .child(
                                div()
                                    .mr_1p5()
                                    .size(px(20.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(crate::theme::object_icon(node.kind, &theme))
                                    .child(if let Some(image) = thumbnail {
                                        img(image).size_full().into_any_element()
                                    } else {
                                        Icon::new(icon).small().into_any_element()
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_xs()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .text_color(if row.context_only { theme.muted_foreground } else { theme.foreground })
                                    .child(StyledText::new(label.clone()).with_highlights(
                                        highlight_ranges(&label, &query).into_iter().map(|range| (
                                            range,
                                            HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() },
                                        )),
                                    ))
                                    .when_some(node.diagnostic,|this,d|this.child(format!(" ⚠ {d}"))),
                            )
                            .when_some(node.visible, |element, visible| {
                                element.child(
                                    div()
                                        .id(SharedString::from(format!("{}-vis-{id_for_vis}", if DEFORMER { "deformer" } else { "mesh" })))
                                        .test_support()
                                        .ml_auto()
                                        .w(px(24.))
                                        .h_full()
                                        .flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .justify_center()
                                        .text_color(if visible { theme.muted_foreground } else { theme.muted_foreground.opacity(0.4) })
                                        .hover(|style| style.text_color(theme.foreground))
                                        .on_click(cx.listener(move |_, _, window, cx| {
                                            cx.stop_propagation();
                                            if disabled { return; }
                                            let (Some(owner), Some(target)) = (owner_for_vis.upgrade(), target) else { return; };
                                            owner.update(cx, |owner, cx| {
                                                if owner.modal || window.has_active_dialog(cx) || owner.editor.is_busy() || owner.editor.active_id() != Some(target) {
                                                    return;
                                                }
                                                let result = owner.editor.execute_for(target, EditorCommand::SetLayerVisible {
                                                    layer_id: id_for_vis.clone(),
                                                    visible: !visible,
                                                });
                                                owner.complete_edit(result, cx);
                                            });
                                        }))
                                        .child(Icon::new(if visible { IconName::Eye } else { IconName::EyeOff }).small()),
                                )
                            })
                        }).collect()
                    }),
                )
                    .flex_1()
                    .min_h_0()
                    .track_scroll(&self.scroll)
                    .into_any_element()
            })
            .into_any_element()
    }
}
