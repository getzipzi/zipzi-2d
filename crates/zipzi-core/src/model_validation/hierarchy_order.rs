use super::*;
impl Model<'_> {
    pub(crate) fn hierarchy_order_issues(&self, order: &HierarchyOrder) -> Vec<StructureIssue> {
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
