use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, TreeNode, TreeNodeKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};
use std::{collections::HashSet, rc::Rc};

#[derive(Clone)]

struct VisibleRow {
    node: TreeNode,
    depth: usize,
    has_children: bool,
}

fn node_key(node: &TreeNode) -> String {
    format!("{:?}:{}", node.kind, node.id)
}

fn visible_rows(nodes: &[TreeNode], collapsed: &HashSet<String>) -> Vec<VisibleRow> {
    fn visit(
        node: &TreeNode,
        depth: usize,
        collapsed: &HashSet<String>,
        rows: &mut Vec<VisibleRow>,
    ) {
        let has_children = !node.children.is_empty();
        let expanded = !collapsed.contains(&node_key(node));
        // Copy only row metadata; collapsed descendants never enter the row cache.
        rows.push(VisibleRow {
            node: TreeNode {
                id: node.id.clone(),
                name: node.name.clone(),
                kind: node.kind,
                diagnostic: node.diagnostic.clone(),
                triangles: node.triangles,
                visible: node.visible,
                children: Vec::new(),
            },
            depth,
            has_children,
        });
        if expanded {
            for child in &node.children {
                visit(child, depth + 1, collapsed, rows);
            }
        }
    }
    let mut rows = Vec::new();
    for node in nodes {
        visit(node, 0, collapsed, &mut rows);
    }
    rows
}

fn count_kind(nodes: &[TreeNode], wanted: impl Fn(TreeNodeKind) -> bool + Copy) -> usize {
    nodes
        .iter()
        .map(|node| usize::from(wanted(node.kind)) + count_kind(&node.children, wanted))
        .sum()
}

pub struct HierarchyTreePanel<const DEFORMER: bool> {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
    _thumbnails_observe: Subscription,
    collapsed: HashSet<String>,
    document_collapsed: std::collections::HashMap<DocumentId, HashSet<String>>,
    document_id: Option<DocumentId>,
    revision: Option<u64>,
    last_selection: Option<editor_core::ObjectRef>,
    nodes: Vec<TreeNode>,
    rows: Rc<Vec<VisibleRow>>,
    rows_dirty: bool,
    count: usize,
    scroll: UniformListScrollHandle,
}

impl<const DEFORMER: bool> HierarchyTreePanel<DEFORMER> {
    pub fn new(
        owner: WeakEntity<EditorView>,
        thumbnails: Entity<crate::mesh_thumbnails::MeshThumbnails>,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            let observe = cx.observe(&owner.upgrade().unwrap(), |_, _, cx| {
                cx.notify();
            });
            let thumbnails_observe = cx.observe(&thumbnails, |_, _, cx| cx.notify());
            Self {
                owner,
                focus,
                _observe: observe,
                _thumbnails_observe: thumbnails_observe,
                collapsed: HashSet::new(),
                document_collapsed: Default::default(),
                document_id: None,
                revision: None,
                last_selection: None,
                nodes: Vec::new(),
                rows: Rc::new(Vec::new()),
                rows_dirty: true,
                count: 0,
                scroll: UniformListScrollHandle::new(),
            }
        })
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
        let state = owner.read(cx);
        let target = state.editor.active_id();
        let revision = state.editor.tree_revision();
        let document_changed = self.document_id != target;
        if document_changed {
            if let Some(previous) = self.document_id {
                self.document_collapsed
                    .insert(previous, self.collapsed.clone());
            }
            self.collapsed = target
                .and_then(|id| self.document_collapsed.get(&id).cloned())
                .unwrap_or_default();
            self.document_id = target;
            self.scroll = UniformListScrollHandle::new();
        }
        if document_changed || self.revision != revision {
            self.revision = revision;
            self.nodes = if DEFORMER {
                state.editor.deformer_tree()
            } else {
                state.editor.mesh_tree()
            };
            self.count = count_kind(&self.nodes, |kind| {
                if DEFORMER {
                    matches!(
                        kind,
                        TreeNodeKind::WarpDeformer | TreeNodeKind::RotationDeformer
                    )
                } else {
                    kind == TreeNodeKind::Mesh
                }
            });
            self.rows_dirty = true;
        }
        let selected = state
            .editor
            .document()
            .and_then(|d| d.context().selection.clone());
        let selections = state
            .editor
            .document()
            .map(|d| d.context().selections.clone())
            .unwrap_or_default();
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
                reveal(&self.nodes, r, &mut self.collapsed);
            }
            self.last_selection = selected.clone();
            self.rows_dirty = true;
        }
        if self.rows_dirty {
            self.rows = Rc::new(visible_rows(&self.nodes, &self.collapsed));
            self.rows_dirty = false;
        }
        if selection_changed {
            if let Some(i) = self.rows.iter().position(|r| {
                r.node.reference().as_ref() == selected.as_ref() && selected.is_some()
            }) {
                self.scroll.scroll_to_item(i, ScrollStrategy::Center);
            }
        }
        let disabled = state.modal || state.editor.is_busy();
        let rows = self.rows.clone();
        let panel = cx.entity();
        let thumbnails = state.mesh_thumbnails.clone();
        thumbnails.update(cx, |cache, cx| cache.sync_document(target, window, cx));
        let theme = cx.theme();

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
            .child(if rows.is_empty() {
                div()
                    .p_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if DEFORMER {
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
                        let is_collapsed = this.collapsed.contains(&key);
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
                            .child(div().id(SharedString::from(format!("tree-expand-{key}"))).w(px(16.)).on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if row.has_children { if !this.collapsed.insert(key.clone()) { this.collapsed.remove(&key); } this.rows_dirty = true; cx.notify(); }
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
                                    .text_color(theme.foreground)
                                    .child(label)
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
