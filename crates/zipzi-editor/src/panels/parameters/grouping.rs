use std::collections::HashSet;
pub use zipzi_editor_core::{ParameterLayout, ParameterRow as ListRow};

/// Ungrouped parameters belong directly to the panel root. Ignore any old
/// collapse state for the synthetic bucket while preserving real group depths.
pub(super) fn visible_rows(
    layout: &ParameterLayout,
    collapsed: &HashSet<Option<String>>,
    related: Option<&HashSet<usize>>,
) -> Vec<ListRow> {
    let mut collapsed = collapsed.clone();
    collapsed.remove(&None);
    // Find matching ancestors before applying collapse, so a folded group
    // remains available when it contains related parameters.
    let groups = related.map(|related| {
        let mut ancestors = Vec::new();
        let mut groups = HashSet::new();
        for row in layout.rows() {
            while ancestors
                .last()
                .is_some_and(|(depth, _)| *depth >= row.depth())
            {
                ancestors.pop();
            }
            match row {
                ListRow::Group { id, depth, .. } => ancestors.push((*depth, id.clone())),
                ListRow::Parameter { index, .. } if related.contains(index) => {
                    groups.extend(ancestors.iter().map(|(_, id)| id.clone()));
                }
                _ => {}
            }
        }
        groups
    });
    let mut root_members = false;
    layout
        .visible(&collapsed)
        .into_iter()
        .filter(|row| match row {
            ListRow::Group { id, .. } => groups.as_ref().is_none_or(|groups| groups.contains(id)),
            ListRow::Parameter { index, .. } => {
                related.is_none_or(|related| related.contains(index))
            }
        })
        .filter_map(|row| match row {
            ListRow::Group { id: None, .. } => {
                root_members = true;
                None
            }
            ListRow::Parameter { index, .. } if root_members => {
                Some(ListRow::Parameter { index, depth: 0 })
            }
            row => Some(row),
        })
        .collect()
}
