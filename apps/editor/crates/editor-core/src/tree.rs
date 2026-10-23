//! Read-only projections of the model's two independent parent relationships.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeNodeKind {
    Group,
    Offscreen,
    Part,
    Mesh,
    WarpDeformer,
    RotationDeformer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub id: String,
    pub name: String,
    pub kind: TreeNodeKind,
    pub diagnostic: Option<String>,
    pub triangles: Option<usize>,
    pub visible: Option<bool>,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(id: String, name: String, kind: TreeNodeKind) -> Self {
        Self {
            id,
            name,
            kind,
            diagnostic: None,
            triangles: None,
            visible: None,
            children: Vec::new(),
        }
    }
}

impl TreeNode {
    pub fn reference(&self) -> Option<crate::ObjectRef> {
        use crate::ObjectKind as K;
        let kind = match self.kind {
            TreeNodeKind::Group => return None,
            TreeNodeKind::Part => K::Part,
            TreeNodeKind::Offscreen => K::Offscreen,
            TreeNodeKind::Mesh => K::Mesh,
            _ => K::Deformer,
        };
        Some(crate::ObjectRef::new(kind, &self.id))
    }
}

/// Keep source order among siblings. Broken references become roots so an
/// incomplete model still leaves every object visible in the palettes.
pub(crate) fn nest_nodes(nodes: Vec<TreeNode>, parents: Vec<Option<usize>>) -> Vec<TreeNode> {
    let mut children = vec![Vec::new(); nodes.len()];
    let mut roots = Vec::new();
    for (index, parent) in parents.into_iter().enumerate() {
        if let Some(parent) = parent.filter(|&parent| parent < nodes.len() && parent != index) {
            children[parent].push(index);
        } else {
            roots.push(index);
        }
    }

    fn take_tree(
        index: usize,
        nodes: &mut [Option<TreeNode>],
        children: &[Vec<usize>],
    ) -> Option<TreeNode> {
        let mut node = nodes[index].take()?;
        node.children = children[index]
            .iter()
            .filter_map(|&child| take_tree(child, nodes, children))
            .collect();
        Some(node)
    }

    let mut nodes: Vec<_> = nodes.into_iter().map(Some).collect();
    let mut result: Vec<_> = roots
        .into_iter()
        .filter_map(|index| take_tree(index, &mut nodes, &children))
        .collect();
    // SDK validation rejects cycles, but do not silently drop their objects if
    // an incomplete candidate document is ever displayed here.
    for index in 0..nodes.len() {
        if let Some(node) = take_tree(index, &mut nodes, &children) {
            result.push(node);
        }
    }
    result
}
