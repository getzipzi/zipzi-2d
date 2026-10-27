mod filter;
mod locking;
mod management;
mod selection;
mod visibility;

use super::sorting::{self, SortItem, SortState};
use super::toolbar::{PanelToolbar, PanelToolbarPopover, PanelToolbarSearch};
use crate::{EditorView, t};
use filter::{TreeFilter, VisibleRow, highlight_ranges, node_key, visible_rows};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    dock::{BasePanel, Panel, PanelEvent},
    input::{InputEvent, InputState},
};
use gpui_kit::{prelude::*, *};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};
use zipzi_editor_core::{DocumentId, EditorCommand, TreeNode, TreeNodeKind};

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
    last_selection: Option<zipzi_editor_core::ObjectRef>,
    locate_target: Option<zipzi_editor_core::ObjectRef>,
    selection: selection::TreeSelection,
    selection_context: Option<zipzi_editor_core::EditingContextToken>,
    nodes: Vec<TreeNode>,
    rows: Rc<Vec<VisibleRow>>,
    rows_dirty: bool,
    scroll: UniformListScrollHandle,
    sorting: Entity<SortState>,
    _sorting_observe: Subscription,
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
            let sorting = SortState::new(owner.clone(), focus.clone(), window, cx);
            let sorting_observe = cx.observe(&sorting, |_, _, cx| cx.notify());
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
                sorting,
                _sorting_observe: sorting_observe,
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
                locate_target: None,
                selection: selection::TreeSelection::default(),
                selection_context: None,
                nodes: Vec::new(),
                rows: Rc::new(Vec::new()),
                rows_dirty: true,
                scroll: UniformListScrollHandle::new(),
            }
        })
    }

    pub(crate) fn locate_moved_object(
        &mut self,
        document: DocumentId,
        object: zipzi_editor_core::ObjectRef,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document) {
            return;
        }
        if self.view.filter.is_active() {
            self.view.filter.revealed = Some(object.clone());
            self.view.filter.reveal_kind = filter::RevealKind::Moved;
        }
        self.locate_target = Some(object);
        self.rows_dirty = true;
        cx.notify();
    }

    pub(crate) fn locate_created_object(
        &mut self,
        document: DocumentId,
        object: zipzi_editor_core::ObjectRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_header(window, cx);
        if self.document_id != Some(document) {
            return;
        }
        self.view.filter.revealed = Some(object.clone());
        self.view.filter.reveal_kind = filter::RevealKind::Created;
        self.last_selection = None;
        self.selection = selection::TreeSelection::default();
        self.locate_target = Some(object);
        self.rows_dirty = true;
        cx.notify();
    }

    fn prefix() -> &'static str {
        if DEFORMER { "deformer" } else { "parts" }
    }

    #[cfg(test)]
    pub(crate) fn sorting_scroll_handle(&self) -> UniformListScrollHandle {
        self.scroll.clone()
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
        self.view.filter.revealed = None;
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

    fn set_all_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        fn collect(nodes: &[TreeNode], keys: &mut HashSet<String>) {
            for node in nodes {
                if !node.children.is_empty() {
                    if node.kind != TreeNodeKind::Group || node.id != "unowned" {
                        keys.insert(node_key(node));
                    }
                    collect(&node.children, keys);
                }
            }
        }
        let collapsed = if self.view.filter.is_active() {
            &mut self.filtered_collapsed
        } else {
            &mut self.view.collapsed
        };
        collapsed.clear();
        if !expanded {
            collect(&self.nodes, collapsed);
        }
        self.rows_dirty = true;
        cx.notify();
    }

    fn has_collapsed_rows(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.has_children && self.collapsed().contains(&node_key(&row.node)))
    }

    fn expansion_button(&self, cx: &Context<Self>) -> Button {
        let expanded = self.has_collapsed_rows();
        let document = self.document_id;
        let disabled = self.owner.upgrade().is_none_or(|owner| {
            let state = owner.read(cx);
            state.modal
                || state.editor.is_busy()
                || document.is_none()
                || state.editor.active_id() != document
        });
        let label = if expanded {
            t!("editor.tree_unfold_all")
        } else {
            t!("editor.tree_fold_all")
        };
        crate::ui::compact::header_button(Button::new(SharedString::from(format!(
            "{}-tree-toggle-expansion",
            Self::prefix()
        ))))
        .icon(if expanded {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        })
        .accessibility_label(label.clone())
        .tooltip(label)
        .disabled(disabled)
        .on_click(cx.listener(move |this, _, window, cx| {
            if window.has_active_dialog(cx) || this.document_id != document {
                return;
            }
            let available = this.owner.upgrade().is_some_and(|owner| {
                let state = owner.read(cx);
                !state.modal
                    && !state.editor.is_busy()
                    && document.is_some()
                    && state.editor.active_id() == document
            });
            if available {
                let expanded = this.has_collapsed_rows();
                this.set_all_expanded(expanded, cx);
            }
        }))
    }

    fn sync_header(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).editor.active_id());
        let context = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).editor.editing_context_token());
        if self.selection_context != context {
            self.selection_context = context;
            self.selection = selection::TreeSelection::default();
        }
        let document_changed = self.document_id != target;
        if document_changed {
            if let Some(previous) = self.document_id {
                self.document_views.insert(previous, self.view.clone());
            }
            self.view = target
                .and_then(|id| self.document_views.get(&id).cloned())
                .unwrap_or_default();
            self.view.filter.revealed = None;
            self.document_id = target;
            self.revision = None;
            self.last_selection = None;
            self.locate_target = None;
            self.rows_dirty = true;
            self.nodes.clear();
            self.rows = Rc::new(Vec::new());
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
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let prefix = Self::prefix();
        let active = self.view.filter.is_active();
        let type_active = !self.view.filter.excluded.is_empty();
        let included = Self::object_types().len() - self.view.filter.excluded.len();
        let filter_label = if type_active {
            t!("editor.tree_visible_types", count = included).into_owned()
        } else {
            t!("editor.tree_filter_types").into_owned()
        };
        let target = self.document_id;
        let panel = cx.weak_entity();
        PanelToolbar::new(SharedString::from(format!("{prefix}-panel-toolbar")))
            .control(management::button(self, cx))
            .when(!DEFORMER, |toolbar| {
                toolbar.control(management::new_part_button(self, cx))
            })
            .control(self.expansion_button(cx))
            .control(
                PanelToolbarPopover::new(
                    SharedString::from(format!("{prefix}-tree-filter-popover")),
                    Button::new(SharedString::from(format!("{prefix}-tree-filter")))
                        .icon(IconName::ListFilter)
                        .selected(type_active)
                        .when(type_active, |button| button.label(included.to_string()))
                        .accessibility_label(filter_label.clone())
                        .tooltip(filter_label),
                )
                .open(self.filter_open)
                .on_open_change(cx.listener(|this, open, _, cx| {
                    this.filter_open = *open;
                    cx.notify();
                }))
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
                        .w(crate::ui::compact::rem(172.))
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
                                .h(crate::ui::compact::rem(24.))
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
            .search(
                PanelToolbarSearch::new(&self.search)
                    .id(SharedString::from(format!("{prefix}-tree-search")))
                    .label(t!("editor.tree_search"))
                    .reset_action(
                        SharedString::from(format!("{prefix}-tree-clear")),
                        t!("editor.tree_clear"),
                        active,
                        cx.listener(|this, _, window, cx| this.clear_filter(window, cx)),
                    ),
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
        if DEFORMER { "Deformers" } else { "PartsTree" }
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl<const DEFORMER: bool> Panel for HierarchyTreePanel<DEFORMER> {
    fn inner_padding(&self, _: &App) -> bool {
        // The toolbar owns spacing below both single and grouped dock titles.
        false
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        if DEFORMER {
            t!("editor.deformer_tree")
        } else {
            t!("editor.part_tree")
        }
    }
}

impl<const DEFORMER: bool> crate::docking::header::PanelHeader for HierarchyTreePanel<DEFORMER> {
    fn header_filtered(&self, _: &App) -> bool {
        self.view.filter.is_active()
    }

    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync_header(window, cx);
        self.toolbar(cx).into_any_element()
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
        self.sync_header(window, cx);
        if document_changed || self.revision != revision {
            self.revision = revision;
            self.nodes = if DEFORMER {
                owner.read(cx).editor.deformer_tree()
            } else {
                owner.read(cx).editor.part_tree()
            };
            if let Some(doc) = owner.read(cx).editor.document() {
                self.selection.reconcile(&doc.object_index());
            }
            self.rows_dirty = true;
        }
        let locate = self.locate_target.take();
        let selection_changed = self.last_selection != selected || document_changed;
        if selection_changed || locate.is_some() {
            if locate.is_none() && self.view.filter.revealed.as_ref() != selected.as_ref() {
                self.view.filter.revealed = None;
            }
            fn reveal(
                nodes: &[TreeNode],
                target: &zipzi_editor_core::ObjectRef,
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
            if let Some(r) = locate.as_ref().or(selected.as_ref()) {
                let collapsed = if self.view.filter.is_active() {
                    &mut self.filtered_collapsed
                } else {
                    &mut self.view.collapsed
                };
                reveal(&self.nodes, r, collapsed);
            }
            if self.selection.cursor.is_none() {
                self.selection.cursor = self
                    .rows
                    .iter()
                    .find(|row| row.node.reference().as_ref() == selected.as_ref())
                    .map(|row| node_key(&row.node));
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
            if self
                .selection
                .cursor
                .as_ref()
                .is_some_and(|key| !self.rows.iter().any(|row| node_key(&row.node) == *key))
            {
                self.selection.cursor = None;
            }
            self.rows_dirty = false;
        }
        if selection_changed || locate.is_some() {
            let target = locate.as_ref().or(selected.as_ref());
            if let Some(i) = self
                .rows
                .iter()
                .position(|r| r.node.reference().as_ref() == target && target.is_some())
            {
                self.scroll.scroll_to_item(i, ScrollStrategy::Center);
            }
        }
        self.sorting.update(cx, |state, cx| {
            state.reconcile(target, !disabled, window, cx)
        });
        let rows = self.rows.clone();
        let panel = cx.entity();
        let query = self.view.filter.query();
        let filtered = self.view.filter.is_active();
        thumbnails.update(cx, |cache, cx| cache.sync_document(target, window, cx));
        let theme = cx.theme().clone();

        sorting::panel_actions(
            div().id(if DEFORMER {
                "deformer-tree-panel"
            } else {
                "parts-tree-panel"
            }),
            &self.sorting,
        )
        .test_support()
        .size_full()
        .min_h_0()
        .overflow_hidden()
        .flex()
        .flex_col()
        .bg(theme.background)
        .text_color(theme.foreground)
        .text_sm()
        .key_context("ZipziPalette")
        .track_focus(&self.focus)
        .on_key_down(cx.listener(Self::tree_key_down))
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
                .map(|element| management::root_menu(element, cx.weak_entity(), target))
                .into_any_element()
        } else {
            sorting::scroll_area(
                uniform_list(
                    if DEFORMER {
                        "deformer-tree-scroll-area"
                    } else {
                        "parts-tree-scroll-area"
                    },
                    rows.len(),
                    move |range, window, cx| {
                        panel.update(cx, |this, cx| {
                            let mesh_ids: Vec<_> = rows[range.clone()]
                                .iter()
                                .filter(|row| row.node.kind == TreeNodeKind::Mesh)
                                .map(|row| row.node.id.as_str())
                                .collect();
                            let mut images = thumbnails
                                .update(cx, |cache, cx| {
                                    cache.request(&owner, &mesh_ids, window, cx)
                                })
                                .into_iter();
                            let theme = cx.theme().clone();
                            range
                                .map(|index| {
                                    let row = rows[index].clone();
                                    let node = row.node;
                                    let is_mesh = node.kind == TreeNodeKind::Mesh;
                                    let thumbnail = if is_mesh {
                                        images.next().flatten()
                                    } else {
                                        None
                                    };
                                    let reference = node.reference();
                                    let created_revealed = filtered
                                        && reference.as_ref().is_some_and(|object| {
                                            this.view.filter.revealed.as_ref() == Some(object)
                                        });
                                    let visibility_object = reference.clone();
                                    let lock_object = reference.clone();
                                    let lock_owner = this.owner.clone();
                                    let locked =
                                        reference.as_ref().is_some_and(|r| {
                                            owner.read(cx).editor.document().is_some_and(|d| {
                                                d.object_state_index().is_locked(r)
                                            })
                                        });
                                    let sort_item =
                                        reference.clone().map(|object| SortItem::Tree {
                                            deformer: DEFORMER,
                                            object,
                                        });
                                    let panel = cx.weak_entity();
                                    let expand_key = node_key(&node);
                                    let expand: sorting::ExpandTarget = Rc::new(move |_, _, cx| {
                                        _ = panel.update(cx, |this, cx| {
                                            let collapsed = if this.view.filter.is_active() {
                                                &mut this.filtered_collapsed
                                            } else {
                                                &mut this.view.collapsed
                                            };
                                            if collapsed.remove(&expand_key) {
                                                this.rows_dirty = true;
                                                cx.notify();
                                            }
                                        });
                                    });
                                    let is_selected =
                                        reference.as_ref().is_some_and(|r| selections.contains(r));
                                    let key = node_key(&node);
                                    let is_collapsed = this.collapsed().contains(&key);
                                    let id = node.id.clone();
                                    let cursor = this.focus.is_focused(window)
                                        && this.selection.cursor.as_ref() == Some(&key);
                                    let icon = match node.kind {
                                        TreeNodeKind::Group | TreeNodeKind::Part => {
                                            IconName::Folder
                                        }
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
                                    let label = if !query.is_empty()
                                        && highlight_ranges(&label, &query).is_empty()
                                        && !highlight_ranges(&id, &query).is_empty()
                                    {
                                        format!("{label} · {id}")
                                    } else {
                                        label
                                    };

                                    div()
                                        .id(SharedString::from(format!(
                                            "{}-item-{key}",
                                            if DEFORMER { "deformer" } else { "parts" }
                                        )))
                                        .group("layer-row")
                                        .flex()
                                        .w_full()
                                        .min_w_0()
                                        .items_center()
                                        .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                                        .pl(crate::ui::compact::rem(4. + row.depth as f32 * 10.))
                                        .pr_2()
                                        .rounded_none()
                                        .bg(if is_selected {
                                            theme.primary.opacity(0.15)
                                        } else {
                                            gpui::transparent_black()
                                        })
                                        .hover(|style| {
                                            style.bg(if is_selected {
                                                theme.primary.opacity(0.22)
                                            } else {
                                                theme.muted.opacity(0.5)
                                            })
                                        })
                                        .cursor_pointer()
                                        .test_support()
                                        .when_some(sort_item.clone(), |element, target_item| {
                                            sorting::drop_target(
                                                element,
                                                &this.sorting,
                                                target_item,
                                                Some(expand.clone()),
                                                cx,
                                            )
                                        })
                                        .border_1()
                                        .border_color(if cursor {
                                            theme.primary
                                        } else {
                                            gpui::transparent_black()
                                        })
                                        .on_click(cx.listener({
                                            let key = key.clone();
                                            move |this, event: &ClickEvent, window, cx| {
                                                if let Some(target) = target {
                                                    this.select_row(
                                                        target,
                                                        &key,
                                                        event.modifiers(),
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            }
                                        }))
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "tree-expand-{key}"
                                                )))
                                                .test_support()
                                                .w(crate::ui::compact::rem(16.))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    if row.has_children {
                                                        this.toggle_expanded(&key, cx);
                                                    }
                                                }))
                                                .text_color(theme.muted_foreground)
                                                .when(row.has_children, |element| {
                                                    element.child(
                                                        Icon::new(if is_collapsed {
                                                            IconName::ChevronRight
                                                        } else {
                                                            IconName::ChevronDown
                                                        })
                                                        .small(),
                                                    )
                                                }),
                                        )
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "{}-drag-{id}",
                                                    Self::prefix()
                                                )))
                                                .test_support()
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .when_some(sort_item, |element, source| {
                                                    sorting::drag_source(
                                                        element,
                                                        &this.sorting,
                                                        target.unwrap(),
                                                        source,
                                                        label.clone(),
                                                        !disabled
                                                            && !locked
                                                            && node.diagnostic.is_none(),
                                                    )
                                                })
                                                .flex_1()
                                                .min_w_0()
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "{}-icon-{id}",
                                                            Self::prefix()
                                                        )))
                                                        .test_support()
                                                        .mr_1p5()
                                                        .size(crate::ui::compact::rem(20.))
                                                        .flex_shrink_0()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .text_color(crate::theme::object_icon(
                                                            node.kind, &theme,
                                                        ))
                                                        .child(if let Some(image) = thumbnail {
                                                            img(image).size_full().into_any_element()
                                                        } else {
                                                            Icon::new(icon).small().into_any_element()
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!(
                                                            "{}-label-{id}",
                                                            Self::prefix()
                                                        )))
                                                        .test_support()
                                                        .h_full()
                                                        .flex()
                                                        .items_center()
                                                        .flex_1()
                                                        .min_w_0()
                                                        .text_xs()
                                                        .text_ellipsis()
                                                        .overflow_hidden()
                                                        .text_color(if row.context_only {
                                                            theme.muted_foreground
                                                        } else {
                                                            theme.foreground
                                                        })
                                                        .child(
                                                            StyledText::new(label.clone()).with_highlights(
                                                                highlight_ranges(&label, &query)
                                                                    .into_iter()
                                                                    .map(|range| {
                                                                        (
                                                                            range,
                                                                            HighlightStyle {
                                                                                font_weight: Some(
                                                                                    FontWeight::SEMIBOLD,
                                                                                ),
                                                                                ..Default::default()
                                                                            },
                                                                        )
                                                                    }),
                                                            ),
                                                        )
                                                        .when_some(node.diagnostic, |this, d| {
                                                            this.child(format!(" ⚠ {d}"))
                                                        }),
                                                ),
                                        )
                                        .when(created_revealed, |element| {
                                            element.child(
                                                div()
                                                    .id(match this.view.filter.reveal_kind {
                                                        filter::RevealKind::Created => {
                                                            "tree-created-item"
                                                        }
                                                        filter::RevealKind::Moved => {
                                                            "tree-moved-item"
                                                        }
                                                    })
                                                    .test_support()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child(t!(
                                                        match this.view.filter.reveal_kind {
                                                            filter::RevealKind::Created =>
                                                                "authoring.new_item",
                                                            filter::RevealKind::Moved =>
                                                                "authoring.moved_item",
                                                        }
                                                    )),
                                            )
                                        })
                                        .when_some(visibility_object, |element, object| {
                                            element.when_some(target, |element, target| {
                                                element.child(visibility::button(
                                                    this.owner.clone(),
                                                    target,
                                                    object,
                                                    SharedString::from(format!(
                                                        "{}-vis-{id}",
                                                        Self::prefix()
                                                    )),
                                                    disabled,
                                                    cx,
                                                ))
                                            })
                                        })
                                        .when_some(lock_object.clone(), |element, object| {
                                            element.when_some(target, |element, target| {
                                                element.child(locking::button(
                                                    lock_owner.clone(),
                                                    target,
                                                    object,
                                                    SharedString::from(format!(
                                                        "{}-lock-{id}",
                                                        Self::prefix()
                                                    )),
                                                    disabled,
                                                    cx,
                                                ))
                                            })
                                        })
                                        .map(|element| match (lock_object, target) {
                                            (Some(object), Some(target)) => locking::menu(
                                                element,
                                                lock_owner.clone(),
                                                target,
                                                object,
                                                cx.weak_entity(),
                                            )
                                            .into_any_element(),
                                            _ => management::root_menu(
                                                element,
                                                cx.weak_entity(),
                                                target,
                                            )
                                            .into_any_element(),
                                        })
                                })
                                .collect()
                        })
                    },
                ),
                &self.sorting,
                &self.scroll,
                self.rows.len(),
            )
            .flex_1()
            .min_h_0()
            .track_scroll(&self.scroll)
            .map(|list| {
                crate::ui::compact::scroll_frame(list, &self.scroll)
                    .id(if DEFORMER {
                        "deformer-tree-scroll-frame"
                    } else {
                        "parts-tree-scroll-frame"
                    })
                    .test_support()
            })
            .into_any_element()
        })
        .when(target.is_some(), |element| {
            element.child(management::root_menu(
                sorting::drop_target(
                    div()
                        .id(if DEFORMER {
                            "deformer-tree-root-drop"
                        } else {
                            "parts-tree-root-drop"
                        })
                        .test_support()
                        .h(crate::ui::compact::rem(crate::ui::compact::ROW))
                        .flex_shrink_0()
                        .px_3()
                        .flex()
                        .items_center()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(t!("editor.tree_drop_root")),
                    &self.sorting,
                    SortItem::TreeRoot { deformer: DEFORMER },
                    None,
                    cx,
                ),
                cx.weak_entity(),
                target,
            ))
        })
        .into_any_element()
    }
}
