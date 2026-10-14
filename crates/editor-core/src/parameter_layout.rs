use crate::{ParameterGroupValue, ParameterValue};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParameterRow {
    Group {
        id: Option<String>,
        name: String,
        depth: usize,
        count: usize,
    },
    Parameter {
        index: usize,
        depth: usize,
    },
}

impl ParameterRow {
    pub fn depth(&self) -> usize {
        match self {
            Self::Group { depth, .. } | Self::Parameter { depth, .. } => *depth,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct ParameterLayout {
    rows: Vec<ParameterRow>,
}

impl ParameterLayout {
    pub fn new(
        groups: &[ParameterGroupValue],
        parameters: &[ParameterValue],
        ungrouped_name: &str,
    ) -> Self {
        let by_id: HashMap<_, _> = groups
            .iter()
            .enumerate()
            .map(|(index, group)| (group.id.as_str(), index))
            .collect();
        let mut children = vec![Vec::new(); groups.len()];
        let mut roots = Vec::new();
        for (index, group) in groups.iter().enumerate() {
            if let Some(&parent) = group.parent_id.as_deref().and_then(|id| by_id.get(id)) {
                children[parent].push(index);
            } else {
                roots.push(index);
            }
        }
        let mut members = vec![Vec::new(); groups.len()];
        let mut ungrouped = Vec::new();
        for (index, parameter) in parameters.iter().enumerate() {
            if let Some(&group) = parameter.group_id.as_deref().and_then(|id| by_id.get(id)) {
                members[group].push(index);
            } else {
                ungrouped.push(index);
            }
        }
        let mut result = Self::default();
        let mut visited = HashSet::new();
        // Also visit disconnected groups defensively, without duplicating a row.
        for index in roots.into_iter().chain(0..groups.len()) {
            result.append_group(index, 0, groups, &children, &members, &mut visited);
        }
        if !groups.is_empty() {
            result.rows.push(ParameterRow::Group {
                id: None,
                name: ungrouped_name.into(),
                depth: 0,
                count: ungrouped.len(),
            });
        }
        result
            .rows
            .extend(ungrouped.into_iter().map(|index| ParameterRow::Parameter {
                index,
                depth: usize::from(!groups.is_empty()),
            }));
        result
    }

    fn append_group(
        &mut self,
        index: usize,
        depth: usize,
        groups: &[ParameterGroupValue],
        children: &[Vec<usize>],
        members: &[Vec<usize>],
        visited: &mut HashSet<usize>,
    ) -> usize {
        if !visited.insert(index) {
            return 0;
        }
        let header = self.rows.len();
        self.rows.push(ParameterRow::Group {
            id: Some(groups[index].id.clone()),
            name: groups[index].name.clone(),
            depth,
            count: 0,
        });
        self.rows
            .extend(members[index].iter().map(|&index| ParameterRow::Parameter {
                index,
                depth: depth + 1,
            }));
        let mut count = members[index].len();
        for &child in &children[index] {
            count += self.append_group(child, depth + 1, groups, children, members, visited);
        }
        if let ParameterRow::Group { count: total, .. } = &mut self.rows[header] {
            *total = count;
        }
        count
    }

    pub fn rows(&self) -> &[ParameterRow] {
        &self.rows
    }

    pub fn visible(&self, collapsed: &HashSet<Option<String>>) -> Vec<ParameterRow> {
        let mut hidden_depth = None;
        self.rows
            .iter()
            .filter_map(|row| {
                if hidden_depth.is_some_and(|depth| row.depth() > depth) {
                    return None;
                }
                hidden_depth = None;
                if let ParameterRow::Group { id, depth, .. } = row {
                    if collapsed.contains(id) {
                        hidden_depth = Some(*depth);
                    }
                }
                Some(row.clone())
            })
            .collect()
    }

    pub fn reveal(&self, index: usize, collapsed: &mut HashSet<Option<String>>) {
        let mut ancestors = Vec::new();
        for row in &self.rows {
            while ancestors.len() > row.depth() {
                ancestors.pop();
            }
            match row {
                ParameterRow::Group { id, .. } => ancestors.push(id.clone()),
                ParameterRow::Parameter {
                    index: row_index, ..
                } if *row_index == index => {
                    for group in ancestors {
                        collapsed.remove(&group);
                    }
                    return;
                }
                _ => {}
            }
        }
    }
}
