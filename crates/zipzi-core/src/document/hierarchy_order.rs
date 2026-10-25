//! Palette order is independent of object storage and runtime draw order.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Independent palette ranks for the organization and deformation projections.
/// Partial lists are allowed: consumers retain source order for unlisted objects.
/// Entries use globally unique scene object IDs, never visible row indices.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct HierarchyOrder {
    #[serde(default)]
    pub organization: Vec<String>,
    #[serde(default)]
    pub deformation: Vec<String>,
    /// Management order only. Glue evaluation retains its native execution order.
    #[serde(default)]
    pub glues: Vec<String>,
}

impl HierarchyOrder {
    pub fn is_empty(&self) -> bool {
        self.organization.is_empty() && self.deformation.is_empty() && self.glues.is_empty()
    }
}

impl Document {
    /// Palette ordering has no effect on relationships or evaluated draw order.
    pub fn hierarchy_order(&self) -> &HierarchyOrder {
        &self.content.hierarchy_order
    }

    /// Validate and replace both display orders as one metadata edit.
    pub fn replace_hierarchy_order(&mut self, order: HierarchyOrder) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let Some(issue) = self.hierarchy_order_issues(&order).into_iter().next() {
            return self.failed(issue.status);
        }
        if self.content.hierarchy_order == order {
            return self.failed(Status::ok());
        }
        if self.content.hierarchy_order.glues != order.glues {
            let complete = |explicit: &[String]| {
                let mut seen = HashSet::new();
                explicit
                    .iter()
                    .chain(self.glue_order())
                    .filter(|id| seen.insert(id.as_str()))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let previous = complete(&self.content.hierarchy_order.glues);
            let proposed = complete(&order.glues);
            for (rank, id) in previous.iter().enumerate() {
                if proposed.get(rank) != Some(id) {
                    if let Err(status) =
                        self.scene_lifecycle_unlocked(&[id.clone()], SceneCopyScope::Selected)
                    {
                        return self.failed(status);
                    }
                }
            }
        }
        if !self.editor_state().is_empty()
            && self.content.hierarchy_order.organization != order.organization
        {
            let previous = self.scene_organization_order();
            let mut seen = HashSet::new();
            let proposed: Vec<_> = order
                .organization
                .iter()
                .chain(previous.iter())
                .filter(|id| seen.insert(id.as_str()))
                .cloned()
                .collect();
            let ranks = |ids: &[String]| {
                let mut counts = HashMap::new();
                let mut ranks = HashMap::new();
                for id in ids {
                    let owner = self.organization_owner(id).unwrap_or("");
                    let partition = owner.is_empty() && self.get_part(id).is_some();
                    let next = counts.entry((owner, partition)).or_insert(0usize);
                    ranks.insert(id.clone(), *next);
                    *next += 1;
                }
                ranks
            };
            let old = ranks(&previous);
            let new = ranks(&proposed);
            for id in &previous {
                if old.get(id) != new.get(id) {
                    if let Err(status) = self.organization_subtree_unlocked(&[id.clone()]) {
                        return self.failed(status);
                    }
                }
            }
        }
        if self.content.hierarchy_order.organization != order.organization {
            self.organization_index.take();
        }
        self.content.hierarchy_order = order;
        self.changed(
            ChangeKind::Metadata,
            Vec::new(),
            vec![self.content.id.clone()],
        )
    }

    pub(super) fn hierarchy_order_issues(&self, order: &HierarchyOrder) -> Vec<StructureIssue> {
        let mut issues = Vec::new();
        let mut seen_glues = HashSet::new();
        for id in &order.glues {
            if self.get_glue(id).is_none() || !seen_glues.insert(id) {
                issues.push(StructureIssue {
                    object_id: id.clone(),
                    status: Status::error(
                        "INVALID_HIERARCHY_ORDER",
                        "Missing or repeated Glue management entry",
                    ),
                });
            }
        }

        for (ids, organization) in [(&order.organization, true), (&order.deformation, false)] {
            let mut seen = HashSet::new();
            for id in ids {
                let valid = self.content.meshes.contains_key(id)
                    || self.content.transforms.contains_key(id)
                    || (organization
                        && (self.content.parts.contains_key(id)
                            || self.content.offscreens.contains_key(id)));
                if !valid || !seen.insert(id) {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status: Status::error(
                            "INVALID_HIERARCHY_ORDER",
                            "Hierarchy order contains a missing, incompatible or repeated object",
                        ),
                    });
                }
            }
        }
        issues
    }
}
