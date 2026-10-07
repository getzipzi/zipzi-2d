//! Persistent palette moves, validated against the complete document projection.
use crate::{Document, ObjectKind, ObjectRef, TreeNode};
use anyhow::{Result, anyhow, ensure};
use kasane_core::document::CdiParameterEntry;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReorderPosition {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParameterDestination {
    Parameter {
        id: String,
        position: ReorderPosition,
    },
    Group(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrganizationDestination {
    Sibling {
        target: ObjectRef,
        position: ReorderPosition,
    },
    /// Append to a Part, or remove the Part assignment with `None`.
    Part(Option<String>),
}

fn collect_order(nodes: &[TreeNode], ids: &mut Vec<String>) {
    for node in nodes {
        if node.reference().is_some() {
            ids.push(node.id.clone());
        }
        collect_order(&node.children, ids);
    }
}

fn relocate<T: PartialEq>(
    items: &mut Vec<T>,
    source: &T,
    target: &T,
    position: ReorderPosition,
) -> Result<()> {
    let from = items
        .iter()
        .position(|item| item == source)
        .ok_or_else(|| anyhow!("Source no longer exists"))?;
    ensure!(
        items.iter().any(|item| item == target),
        "Target no longer exists"
    );
    if source == target {
        return Ok(());
    }
    let item = items.remove(from);
    let to = items.iter().position(|item| item == target).unwrap();
    items.insert(to + usize::from(position == ReorderPosition::After), item);
    Ok(())
}

impl Document {
    /// Change organization and palette position atomically; deformation is independent.
    pub fn move_organization(
        &mut self,
        source: &ObjectRef,
        destination: OrganizationDestination,
    ) -> Result<()> {
        fn index(
            nodes: &[TreeNode],
            parent: &str,
            entries: &mut HashMap<ObjectRef, (String, bool)>,
        ) {
            for node in nodes {
                if let Some(reference) = node.reference() {
                    entries.insert(reference, (parent.to_owned(), node.diagnostic.is_none()));
                    index(&node.children, &node.id, entries);
                } else {
                    index(&node.children, "", entries);
                }
            }
        }
        self.ensure_tree_editable(source, false)?;
        let destination_object = match destination {
            OrganizationDestination::Sibling { ref target, .. } => Some(target.clone()),
            OrganizationDestination::Part(Some(ref id)) => {
                Some(ObjectRef::new(ObjectKind::Part, id))
            }
            _ => None,
        };
        if let Some(target) = &destination_object {
            self.ensure_editable(std::slice::from_ref(target))?;
        }
        let tree = self.mesh_tree();
        let mut entries = HashMap::new();
        index(&tree, "", &mut entries);
        let (old_parent, valid) = entries
            .get(source)
            .ok_or_else(|| anyhow!("Object no longer exists"))?;
        ensure!(*valid, "Repair missing references before moving");
        let parent = match &destination {
            OrganizationDestination::Sibling { target, .. } => {
                let (parent, valid) = entries
                    .get(target)
                    .ok_or_else(|| anyhow!("Target no longer exists"))?;
                ensure!(*valid, "Repair missing references before moving");
                ensure!(
                    !parent.is_empty()
                        || (source.kind == ObjectKind::Part) == (target.kind == ObjectKind::Part),
                    "Root Parts and unassigned objects occupy separate lists"
                );
                parent.clone()
            }
            OrganizationDestination::Part(parent) => parent.clone().unwrap_or_default(),
        };
        if !parent.is_empty() {
            let mut current = parent.as_str();
            let mut visited = HashSet::new();
            while !current.is_empty() {
                ensure!(
                    current != source.id && visited.insert(current),
                    "Cannot move a Part into its own subtree"
                );
                let (next, valid) = entries
                    .get(&ObjectRef::new(ObjectKind::Part, current))
                    .ok_or_else(|| anyhow!("Target Part no longer exists"))?;
                ensure!(*valid, "Repair missing references before moving");
                current = next;
            }
        }
        let mut original = Vec::new();
        collect_order(&tree, &mut original);
        let siblings: HashSet<_> = entries
            .iter()
            .filter(|(reference, (owner, _))| {
                owner == &parent
                    && (!parent.is_empty()
                        || (reference.kind == ObjectKind::Part)
                            == (source.kind == ObjectKind::Part))
            })
            .map(|(reference, _)| &reference.id)
            .collect();
        let is_sibling = |id: &String| siblings.contains(id);
        let mut ids = original.clone();
        match &destination {
            OrganizationDestination::Sibling { target, position } => {
                relocate(&mut ids, &source.id, &target.id, *position)?
            }
            OrganizationDestination::Part(_) => {
                ids.retain(|id| id != &source.id);
                let at = ids
                    .iter()
                    .rposition(&is_sibling)
                    .map_or(ids.len(), |i| i + 1);
                ids.insert(at, source.id.clone());
            }
        }
        if old_parent == &parent
            && original
                .iter()
                .filter(|id| is_sibling(id))
                .eq(ids.iter().filter(|id| is_sibling(id)))
        {
            return Ok(());
        }
        let mut order = self.model().hierarchy_order().clone();
        order.organization = ids;
        let offscreen = self.model().get_offscreen(&source.id).cloned();
        let parent_changed = old_parent != &parent;
        self.finish_edit()?;
        // Legacy projects derive drawing groups from Part ownership. Preserve that
        // drawing hierarchy before changing organization, including its tie order
        // and animated draw-order ranges, rather than freezing one evaluated pose.
        let drawing_groups = (parent_changed && self.model().draw_order_groups().is_none())
            .then(|| kasane_core::draw_order::resolved_groups(self.model()));
        self.session.edit(
            if parent_changed {
                "Move Part assignment"
            } else {
                "Reorder hierarchy"
            },
            None,
            |edit| {
                if let Some(groups) = drawing_groups {
                    edit.replace_draw_order_groups(groups)?;
                }
                if parent_changed {
                    match source.kind {
                        ObjectKind::Part => edit.set_organization_parent(&source.id, &parent)?,
                        ObjectKind::Mesh => edit.set_mesh_part(&source.id, &parent)?,
                        ObjectKind::Deformer => edit.set_transform_part(
                            &source.id,
                            (!parent.is_empty()).then(|| parent.clone().into()),
                        )?,
                        ObjectKind::Offscreen => {
                            let mut offscreen = offscreen.unwrap();
                            offscreen.part_id = parent;
                            edit.replace_offscreen(offscreen)?;
                        }
                        _ => unreachable!("Only organization tree objects enter the index"),
                    }
                }
                edit.replace_hierarchy_order(order)
            },
        )?;
        self.sorted();
        Ok(())
    }

    pub fn reorder_tree(
        &mut self,
        deformer: bool,
        source: &ObjectRef,
        target: &ObjectRef,
        position: ReorderPosition,
    ) -> Result<()> {
        self.ensure_tree_editable(source, deformer)?;
        let mut tree = if deformer {
            self.deformer_tree()
        } else {
            self.mesh_tree()
        };
        fn siblings<'a>(
            nodes: &'a mut Vec<TreeNode>,
            source: &ObjectRef,
        ) -> Option<&'a mut Vec<TreeNode>> {
            if nodes
                .iter()
                .any(|node| node.reference().as_ref() == Some(source))
            {
                return Some(nodes);
            }
            for node in nodes {
                if let Some(found) = siblings(&mut node.children, source) {
                    return Some(found);
                }
            }
            None
        }
        let nodes =
            siblings(&mut tree, source).ok_or_else(|| anyhow!("Object no longer exists"))?;
        ensure!(
            nodes
                .iter()
                .any(|node| node.reference().as_ref() == Some(target)),
            "Only siblings can be reordered"
        );
        ensure!(
            nodes
                .iter()
                .filter(|node| [Some(source), Some(target)].contains(&node.reference().as_ref()))
                .all(|node| node.diagnostic.is_none()),
            "Repair missing references before reordering"
        );
        let original: Vec<_> = nodes.iter().map(|node| node.id.clone()).collect();
        let mut ids = original.clone();
        relocate(&mut ids, &source.id, &target.id, position)?;
        if ids == original {
            return Ok(());
        }
        let ranks: HashMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        nodes.sort_by_key(|node| ranks[node.id.as_str()]);
        let mut order = self.model().hierarchy_order().clone();
        let ids = if deformer {
            &mut order.deformation
        } else {
            &mut order.organization
        };
        ids.clear();
        collect_order(&tree, ids);
        self.finish_edit()?;
        self.session.edit("Reorder hierarchy", None, |edit| {
            edit.replace_hierarchy_order(order)
        })?;
        self.sorted();
        Ok(())
    }

    pub fn move_parameter(
        &mut self,
        source: &str,
        destination: ParameterDestination,
    ) -> Result<()> {
        let parameters = self.parameters();
        let parameter = parameters
            .iter()
            .find(|parameter| parameter.id == source)
            .ok_or_else(|| anyhow!("Parameter no longer exists"))?;
        let mut ids: Vec<_> = parameters
            .iter()
            .map(|parameter| parameter.id.clone())
            .collect();
        let group_id = match destination {
            ParameterDestination::Parameter { id, position } => {
                let target = parameters
                    .iter()
                    .find(|parameter| parameter.id == id)
                    .ok_or_else(|| anyhow!("Target parameter no longer exists"))?;
                relocate(&mut ids, &source.to_owned(), &id, position)?;
                target.group_id.clone()
            }
            ParameterDestination::Group(group) => {
                ensure!(
                    group.as_ref().is_none_or(|id| self
                        .parameter_groups()
                        .iter()
                        .any(|group| &group.id == id)),
                    "Group no longer exists"
                );
                ids.retain(|id| id != source);
                let last = ids.iter().rposition(|id| {
                    parameters
                        .iter()
                        .any(|parameter| &parameter.id == id && parameter.group_id == group)
                });
                ids.insert(last.map_or(ids.len(), |i| i + 1), source.to_owned());
                group
            }
        };
        if parameter.group_id == group_id {
            let original_members: Vec<_> = parameters
                .iter()
                .filter(|p| p.group_id == group_id)
                .map(|p| &p.id)
                .collect();
            let next_members: Vec<_> = ids
                .iter()
                .filter(|id| {
                    parameters
                        .iter()
                        .any(|p| &p.id == *id && p.group_id == group_id)
                })
                .collect();
            if original_members == next_members {
                return Ok(());
            }
        }
        let mut info = self.model().display_info().clone();
        let mut entries = info.parameters.take().unwrap_or_default();
        let mut resolved: HashMap<_, _> = entries
            .iter()
            .filter_map(|entry| match entry {
                CdiParameterEntry::Resolved { parameter_id, .. } => {
                    Some((parameter_id.clone(), entry.clone()))
                }
                _ => None,
            })
            .collect();
        let mut ordered = ids.into_iter().map(|id| {
            let mut entry = resolved
                .remove(&id)
                .unwrap_or_else(|| CdiParameterEntry::Resolved {
                    parameter_id: id.clone(),
                    group_id: None,
                    extensions: Default::default(),
                });
            if let CdiParameterEntry::Resolved {
                group_id: current, ..
            } = &mut entry
            {
                if id == source {
                    *current = group_id.clone();
                }
            }
            entry
        });
        // Opaque/unresolved entries retain their relative positions and complete data.
        for entry in &mut entries {
            if matches!(entry, CdiParameterEntry::Resolved { .. }) {
                *entry = ordered.next().unwrap();
            }
        }
        entries.extend(ordered);
        info.parameters = Some(entries);
        self.finish_edit()?;
        self.session.edit("Move parameter", None, |edit| {
            edit.replace_display_info(info)
        })?;
        self.sorted();
        Ok(())
    }

    pub fn reorder_parameter_group(
        &mut self,
        source: &str,
        target: &str,
        position: ReorderPosition,
    ) -> Result<()> {
        let mut info = self.model().display_info().clone();
        let groups = info
            .parameter_groups
            .as_mut()
            .ok_or_else(|| anyhow!("Group no longer exists"))?;
        let from = groups
            .iter()
            .find(|group| group.id == source)
            .ok_or_else(|| anyhow!("Group no longer exists"))?;
        let to = groups
            .iter()
            .find(|group| group.id == target)
            .ok_or_else(|| anyhow!("Target group no longer exists"))?;
        ensure!(
            from.parent_id == to.parent_id,
            "Only sibling groups can be reordered"
        );
        let original: Vec<_> = groups.iter().map(|group| group.id.clone()).collect();
        let mut ids = original.clone();
        relocate(&mut ids, &source.to_owned(), &target.to_owned(), position)?;
        let siblings: HashSet<_> = groups
            .iter()
            .filter(|group| group.parent_id == from.parent_id)
            .map(|group| &group.id)
            .collect();
        if original
            .iter()
            .filter(|id| siblings.contains(id))
            .eq(ids.iter().filter(|id| siblings.contains(id)))
        {
            return Ok(());
        }
        let ranks: HashMap<_, _> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        groups.sort_by_key(|group| ranks[group.id.as_str()]);
        self.finish_edit()?;
        self.session
            .edit("Reorder parameter groups", None, |edit| {
                edit.replace_display_info(info)
            })?;
        self.sorted();
        Ok(())
    }

    fn sorted(&mut self) {
        self.session.drain_events();
        self.pristine_blank = false;
        self.projection_revision += 1;
    }
}

pub(crate) fn ordered_parameter_ids(model: &kasane_core::Document) -> Vec<String> {
    let mut seen = HashSet::new();
    model
        .display_info()
        .parameters
        .iter()
        .flatten()
        .filter_map(|entry| match entry {
            CdiParameterEntry::Resolved { parameter_id, .. } => Some(parameter_id),
            _ => None,
        })
        .chain(model.parameter_order())
        .filter(|id| model.get_parameter(id).is_some() && seen.insert((*id).clone()))
        .cloned()
        .collect()
}
