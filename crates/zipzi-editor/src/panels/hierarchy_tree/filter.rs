use std::{collections::HashSet, ops::Range};
use zipzi_editor_core::{TreeNode, TreeNodeKind};

#[derive(Clone, Copy, Default)]
pub(super) enum RevealKind {
    #[default]
    Created,
    Moved,
}

#[derive(Clone, Default)]
pub(super) struct TreeFilter {
    pub query: String,
    pub excluded: Vec<TreeNodeKind>,
    pub revealed: Option<zipzi_editor_core::ObjectRef>,
    pub reveal_kind: RevealKind,
}

impl TreeFilter {
    pub fn is_active(&self) -> bool {
        !self.query.trim().is_empty() || !self.excluded.is_empty()
    }

    pub fn query(&self) -> String {
        lowercase(self.query.trim())
    }

    pub fn includes(&self, kind: TreeNodeKind) -> bool {
        kind != TreeNodeKind::Group && !self.excluded.contains(&kind)
    }

    pub fn set_included(&mut self, kind: TreeNodeKind, included: bool) {
        self.excluded.retain(|excluded| *excluded != kind);
        if !included {
            self.excluded.push(kind);
        }
    }
}

#[derive(Clone)]
pub(super) struct VisibleRow {
    pub node: TreeNode,
    pub depth: usize,
    pub has_children: bool,
    pub context_only: bool,
}

pub(super) fn node_key(node: &TreeNode) -> String {
    format!("{:?}:{}", node.kind, node.id)
}

fn row(node: &TreeNode, depth: usize, context_only: bool) -> VisibleRow {
    VisibleRow {
        // Rows cache metadata only, never cloned subtrees.
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
        has_children: !node.children.is_empty(),
        context_only,
    }
}

pub(super) fn visible_rows(
    nodes: &[TreeNode],
    collapsed: &HashSet<String>,
    filter: &TreeFilter,
) -> Vec<VisibleRow> {
    fn unfiltered(
        node: &TreeNode,
        depth: usize,
        collapsed: &HashSet<String>,
        rows: &mut Vec<VisibleRow>,
    ) {
        rows.push(row(node, depth, false));
        if !collapsed.contains(&node_key(node)) {
            for child in &node.children {
                unfiltered(child, depth + 1, collapsed, rows);
            }
        }
    }

    fn filtered(
        node: &TreeNode,
        depth: usize,
        ancestor_match: bool,
        query: &str,
        filter: &TreeFilter,
        collapsed: &HashSet<String>,
        rows: &mut Vec<VisibleRow>,
    ) -> bool {
        // Search scope and object type are independent: a Part's name can
        // include its Mesh descendants even when only Mesh is checked.
        let in_scope = ancestor_match
            || query.is_empty()
            || lowercase(&node.name).contains(query)
            || lowercase(&node.id).contains(query);
        let matches = (in_scope && filter.includes(node.kind))
            || filter
                .revealed
                .as_ref()
                .is_some_and(|object| node.reference().as_ref() == Some(object));
        let start = rows.len();
        rows.push(row(node, depth, !matches));
        let mut has_children = false;
        for child in &node.children {
            has_children |= filtered(child, depth + 1, in_scope, query, filter, collapsed, rows);
        }
        if !matches && !has_children {
            rows.truncate(start);
            return false;
        }
        rows[start].has_children = has_children;
        if collapsed.contains(&node_key(node)) {
            rows.truncate(start + 1);
        }
        true
    }

    let mut rows = Vec::new();
    if filter.is_active() {
        let query = filter.query();
        for node in nodes {
            filtered(node, 0, false, &query, filter, collapsed, &mut rows);
        }
    } else {
        for node in nodes {
            unfiltered(node, 0, collapsed, &mut rows);
        }
    }
    rows
}

fn lowercase(text: &str) -> String {
    text.chars().flat_map(char::to_lowercase).collect()
}

/// Match in lowercase, but keep highlight boundaries in the original UTF-8
/// string (lowercasing can change the number of bytes or characters).
pub(super) fn highlight_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut original = Vec::new();
    for (start, ch) in text.char_indices() {
        let part: String = ch.to_lowercase().collect();
        original.extend(std::iter::repeat_n(
            start..start + ch.len_utf8(),
            part.len(),
        ));
        folded.push_str(&part);
    }
    folded
        .match_indices(query)
        .map(|(start, matched)| original[start].start..original[start + matched.len() - 1].end)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use TreeNodeKind::*;

    fn fixture() -> Vec<TreeNode> {
        let mut root = TreeNode::new("root".into(), "Character".into(), Part);
        let mut head = TreeNode::new("head".into(), "头部".into(), Part);
        head.children = vec![
            TreeNode::new("eye-left".into(), "Eye".into(), Mesh),
            TreeNode::new("warp".into(), "Head warp".into(), WarpDeformer),
        ];
        root.children = vec![head, TreeNode::new("body".into(), "Body".into(), Mesh)];
        vec![root]
    }

    fn ids(rows: &[VisibleRow]) -> Vec<&str> {
        rows.iter().map(|row| row.node.id.as_str()).collect()
    }

    #[test]
    fn searching_parent_includes_descendants_and_preserves_ancestor_path() {
        let filter = TreeFilter {
            query: "  头部  ".into(),
            ..Default::default()
        };
        let rows = visible_rows(&fixture(), &HashSet::new(), &filter);
        assert_eq!(ids(&rows), ["root", "head", "eye-left", "warp"]);
        assert_eq!(
            rows.iter().map(|row| row.depth).collect::<Vec<_>>(),
            [0, 1, 2, 2]
        );
        assert!(rows[0].context_only);
        assert!(!rows[1].context_only);
        assert!(!rows[2].context_only);
    }

    #[test]
    fn type_filter_applies_after_parent_search_and_keeps_only_needed_ancestors() {
        let filter = TreeFilter {
            query: "头部".into(),
            excluded: vec![Part, WarpDeformer, RotationDeformer, Offscreen],
            ..Default::default()
        };
        let rows = visible_rows(&fixture(), &HashSet::new(), &filter);
        assert_eq!(ids(&rows), ["root", "head", "eye-left"]);
        assert!(rows[0].context_only && rows[1].context_only);
        assert!(!rows[2].context_only);
        assert!(!rows[2].has_children);
    }

    #[test]
    fn search_matches_name_and_id_without_changing_source_order() {
        for query in ["EYE", "EYE-LEFT"] {
            let filter = TreeFilter {
                query: query.into(),
                ..Default::default()
            };
            assert_eq!(
                ids(&visible_rows(&fixture(), &HashSet::new(), &filter)),
                ["root", "head", "eye-left"]
            );
        }
        let filter = TreeFilter {
            excluded: vec![Part],
            ..Default::default()
        };
        assert_eq!(
            ids(&visible_rows(&fixture(), &HashSet::new(), &filter)),
            ["root", "head", "eye-left", "warp", "body"]
        );
    }

    #[test]
    fn excluded_parents_survive_as_paths_but_empty_branches_and_all_excluded_do_not() {
        let mut filter = TreeFilter {
            excluded: vec![Mesh, WarpDeformer],
            ..Default::default()
        };
        let rows = visible_rows(&fixture(), &HashSet::new(), &filter);
        assert_eq!(ids(&rows), ["root", "head"]);
        assert!(!rows[1].has_children);
        filter.set_included(Part, false);
        assert!(visible_rows(&fixture(), &HashSet::new(), &filter).is_empty());
        filter = TreeFilter {
            query: "missing".into(),
            ..Default::default()
        };
        assert!(visible_rows(&fixture(), &HashSet::new(), &filter).is_empty());
    }

    #[test]
    fn filtered_collapsing_still_finds_matches_in_hidden_descendants() {
        let filter = TreeFilter {
            query: "eye".into(),
            ..Default::default()
        };
        let collapsed = HashSet::from([node_key(&fixture()[0])]);
        let rows = visible_rows(&fixture(), &collapsed, &filter);
        assert_eq!(ids(&rows), ["root"]);
        assert!(rows[0].has_children && rows[0].context_only);
        assert_eq!(
            ids(&visible_rows(
                &fixture(),
                &collapsed,
                &TreeFilter::default()
            )),
            ["root"]
        );
    }

    #[test]
    fn temporary_reveal_keeps_only_the_created_item_and_its_ancestor_path() {
        let filter = TreeFilter {
            query: "missing".into(),
            excluded: vec![Mesh],
            revealed: Some(zipzi_editor_core::ObjectRef::new(
                zipzi_editor_core::ObjectKind::Mesh,
                "eye-left",
            )),
            ..Default::default()
        };
        let rows = visible_rows(&fixture(), &HashSet::new(), &filter);
        assert_eq!(ids(&rows), ["root", "head", "eye-left"]);
        assert!(rows[0].context_only && rows[1].context_only);
        assert!(!rows[2].context_only);
        let filter = TreeFilter {
            revealed: Some(zipzi_editor_core::ObjectRef::new(
                zipzi_editor_core::ObjectKind::Part,
                "head",
            )),
            ..filter
        };
        assert_eq!(
            ids(&visible_rows(&fixture(), &HashSet::new(), &filter)),
            ["root", "head"]
        );
    }

    #[test]
    fn highlight_ranges_preserve_original_unicode_boundaries() {
        assert_eq!(highlight_ranges("Eye EYE", "eye"), [0..3, 4..7]);
        assert_eq!(highlight_ranges("左眼 头部", "头部"), [7..13]);
        assert_eq!(highlight_ranges("İris", "i"), [0..2, 3..4]);
    }
}
