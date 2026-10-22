//! Persistent palette moves, validated against the complete document projection.
use crate::{Document, ObjectRef, TreeNode};
use anyhow::{Result, anyhow, ensure};
use kasane_sdk::{OrganizationCollection as C, OrganizationEdit as E};
use std::collections::HashMap;

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
    pub fn move_organization(
        &mut self,
        source: &ObjectRef,
        destination: OrganizationDestination,
    ) -> Result<()> {
        let Some(prepared) = self.prepare_organization_move(source, destination)? else {
            return Ok(());
        };
        self.finish_edit()?;
        let plan = self.session.plan_organization_edit(prepared.intent)?;
        self.execute_organization_authoring(&plan)?;
        Ok(())
    }
    pub(crate) fn prepare_organization_move(
        &self,
        source: &ObjectRef,
        destination: OrganizationDestination,
    ) -> Result<Option<OrganizationMove>> {
        self.ensure_tree_editable(source, false)?;
        let (parent, before) = match destination {
            OrganizationDestination::Part(parent) => (parent, None),
            OrganizationDestination::Sibling { target, position } => {
                if source == &target {
                    return Ok(None);
                }
                ensure!(
                    self.object_index().get(&target).is_some(),
                    "Target no longer exists"
                );
                self.ensure_editable(std::slice::from_ref(&target))?;
                let parent = self
                    .model()
                    .organization_owner(&target.id)
                    .map(str::to_owned);
                let collection = C::PartMembers {
                    part_id: parent.clone(),
                };
                let before =
                    placement_after(self.model(), &collection, &source.id, &target.id, position)?;
                (parent, before)
            }
        };
        let intent = E::Move {
            ids: vec![source.id.clone()],
            destination: C::PartMembers { part_id: parent },
            before,
        };
        let plan = self.session.plan_organization_edit(intent.clone())?;
        Ok((!plan.is_noop()).then_some(OrganizationMove { intent }))
    }
    pub fn reorder_tree(
        &mut self,
        deformer: bool,
        source: &ObjectRef,
        target: &ObjectRef,
        position: ReorderPosition,
    ) -> Result<()> {
        if !deformer {
            return self.move_organization(
                source,
                OrganizationDestination::Sibling {
                    target: target.clone(),
                    position,
                },
            );
        }
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
        let (group, before) = match destination {
            ParameterDestination::Group(group) => (group, None),
            ParameterDestination::Parameter { id, position } => {
                if source == id {
                    return Ok(());
                }
                ensure!(
                    self.model().get_parameter(&id).is_some(),
                    "Target parameter no longer exists"
                );
                let group = self.model().parameter_group_for(&id).map(str::to_owned);
                let collection = C::Parameters {
                    group_id: group.clone(),
                };
                let before = placement_after(self.model(), &collection, source, &id, position)?;
                (group, before)
            }
        };
        self.finish_edit()?;
        let plan = self.session.plan_organization_edit(E::Move {
            ids: vec![source.into()],
            destination: C::Parameters { group_id: group },
            before,
        })?;
        self.execute_organization_authoring(&plan)?;
        Ok(())
    }
    pub fn reorder_parameter_group(
        &mut self,
        source: &str,
        target: &str,
        position: ReorderPosition,
    ) -> Result<()> {
        if source == target {
            return Ok(());
        }
        let group = self
            .model()
            .get_parameter_group(target)
            .ok_or_else(|| anyhow!("Target group no longer exists"))?;
        let collection = C::ParameterGroups {
            parent_id: group.parent_id.clone(),
        };
        let before = placement_after(self.model(), &collection, source, target, position)?;
        self.finish_edit()?;
        let plan = self.session.plan_organization_edit(E::Move {
            ids: vec![source.into()],
            destination: collection,
            before,
        })?;
        self.execute_organization_authoring(&plan)?;
        Ok(())
    }
    fn sorted(&mut self) {
        self.session.drain_events();
        self.pristine_blank = false;
        self.invalidate_projection(false);
    }
}
fn placement_after(
    d: &kasane_core::Document,
    c: &C,
    source: &str,
    target: &str,
    position: ReorderPosition,
) -> Result<Option<String>> {
    if position == ReorderPosition::Before {
        return Ok(Some(target.into()));
    }
    let members = d
        .organization_collection(c)
        .map_err(|s| anyhow!("{}: {}", s.code, s.message))?;
    let members: Vec<_> = members
        .into_iter()
        .filter(|id| {
            id != source
                && (!matches!(c, C::PartMembers { part_id: None })
                    || d.get_part(id).is_some() == d.get_part(target).is_some())
        })
        .collect();
    let index = members
        .iter()
        .position(|id| id == target)
        .ok_or_else(|| anyhow!("Target no longer exists"))?;
    Ok(members.get(index + 1).cloned())
}
pub(crate) fn ordered_parameter_ids(model: &kasane_core::Document) -> Vec<String> {
    model.ordered_parameter_ids()
}
pub(crate) struct OrganizationMove {
    intent: E,
}
impl OrganizationMove {
    pub(crate) fn apply(
        self,
        edit: &mut kasane_sdk::EditSession<'_>,
        _source: &ObjectRef,
    ) -> std::result::Result<(), kasane_sdk::SdkError> {
        edit.edit_organization(self.intent)
    }
}
