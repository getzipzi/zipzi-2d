use crate::{EditorView, t};
use editor_core::{DocumentId, EditorCommand, TreeNode, TreeNodeKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};
use std::collections::HashSet;

struct VisibleRow {
    node: TreeNode,
    depth: usize,
    has_children: bool,
}

fn node_key(node: &TreeNode) -> String {
    format!("{:?}:{}", node.kind, node.id)
}

fn visible_rows(nodes: Vec<TreeNode>, collapsed: &HashSet<String>) -> Vec<VisibleRow> {
    fn visit(
        mut node: TreeNode,
        depth: usize,
        collapsed: &HashSet<String>,
        rows: &mut Vec<VisibleRow>,
    ) {
        let children = std::mem::take(&mut node.children);
        let has_children = !children.is_empty();
        let expanded = !collapsed.contains(&node_key(&node));
        rows.push(VisibleRow {
            node,
            depth,
            has_children,
        });
        if expanded {
            for child in children {
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
    collapsed: HashSet<String>,
    document_id: Option<DocumentId>,
}

impl<const DEFORMER: bool> HierarchyTreePanel<DEFORMER> {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            let observe = cx.observe(&owner.upgrade().unwrap(), |_, _, cx| {
                cx.notify();
            });
            Self {
                owner,
                focus,
                _observe: observe,
                collapsed: HashSet::new(),
                document_id: None,
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let target = state.editor.active_id();
        if self.document_id != target {
            self.collapsed.clear();
            self.document_id = target;
        }
        let nodes = if DEFORMER {
            state.editor.deformer_tree()
        } else {
            state.editor.mesh_tree()
        };
        let count = count_kind(&nodes, |kind| {
            if DEFORMER {
                matches!(
                    kind,
                    TreeNodeKind::WarpDeformer | TreeNodeKind::RotationDeformer
                )
            } else {
                kind == TreeNodeKind::Mesh
            }
        });
        let selected = state.editor.selected_layer().map(|layer| layer.id);
        let disabled = state.modal || state.editor.is_busy();
        let rows = visible_rows(nodes, &self.collapsed);
        let theme = cx.theme();
        let title = if DEFORMER {
            t!("editor.deformer_tree")
        } else {
            t!("editor.mesh_tree")
        };

        div()
            .id(if DEFORMER {
                "deformer-tree-panel"
            } else {
                "mesh-tree-panel"
            })
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_sm()
            .track_focus(&self.focus)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.muted_foreground)
                    .child(format!("{} ({count})", title)),
            )
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
                div()
                    .id(if DEFORMER {
                        "deformer-tree-scroll-area"
                    } else {
                        "mesh-tree-scroll-area"
                    })
                    .flex_1()
                    .overflow_y_scroll()
                    .p_1p5()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(rows.into_iter().map(|row| {
                        let node = row.node;
                        let is_mesh = node.kind == TreeNodeKind::Mesh;
                        let is_selected = is_mesh && selected.as_ref() == Some(&node.id);
                        let key = node_key(&node);
                        let is_collapsed = self.collapsed.contains(&key);
                        let id = node.id.clone();
                        let owner = self.owner.clone();
                        let icon = match node.kind {
                            TreeNodeKind::Part => IconName::Folder,
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
                            .items_center()
                            .h(px(26.))
                            .pl(px(6. + row.depth as f32 * 16.))
                            .pr_2()
                            .rounded_md()
                            .bg(if is_selected {
                                theme.primary.opacity(0.15)
                            } else {
                                gpui::transparent_black()
                            })
                            .hover(|style| style.bg(theme.muted.opacity(0.5)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if is_mesh {
                                    if disabled {
                                        return;
                                    }
                                    let Some(owner) = owner.upgrade() else {
                                        return;
                                    };
                                    let Some(target) = target else {
                                        return;
                                    };
                                    owner.update(cx, |owner, cx| {
                                        if owner.modal
                                            || window.has_active_dialog(cx)
                                            || owner.editor.is_busy()
                                            || owner.editor.active_id() != Some(target)
                                        {
                                            return;
                                        }
                                        let result = owner
                                            .editor
                                            .execute_for(target, EditorCommand::Select(id.clone()));
                                        owner.complete_edit(result, cx);
                                    });
                                } else if row.has_children {
                                    if !this.collapsed.insert(key.clone()) {
                                        this.collapsed.remove(&key);
                                    }
                                    cx.notify();
                                }
                            }))
                            .child(div().w(px(16.)).text_color(theme.muted_foreground).when(
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
                                    .text_color(theme.muted_foreground)
                                    .child(Icon::new(icon).small()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .text_color(if is_selected {
                                        theme.primary
                                    } else {
                                        theme.foreground
                                    })
                                    .child(label),
                            )
                            .when_some(node.triangles, |element, triangles| {
                                element.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground.opacity(0.7))
                                        .child(format!("{triangles} tris")),
                                )
                            })
                    }))
                    .into_any_element()
            })
            .into_any_element()
    }
}
