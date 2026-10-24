use std::collections::HashSet;
pub use zipzi_editor_core::{ParameterLayout, ParameterRow as ListRow};

/// Ungrouped parameters belong directly to the panel root. Ignore any old
/// collapse state for the synthetic bucket while preserving real group depths.
pub(super) fn visible_rows(
    layout: &ParameterLayout,
    collapsed: &HashSet<Option<String>>,
) -> Vec<ListRow> {
    let mut collapsed = collapsed.clone();
    collapsed.remove(&None);
    let mut root_members = false;
    layout
        .visible(&collapsed)
        .into_iter()
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
