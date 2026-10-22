//! Immutable relationship/order cache. Labels and scalar edits do not invalidate it.
use super::*;
#[derive(Debug, Clone, Default)]
pub(in crate::document) struct OrganizationIndex {
    pub(super) owners: HashMap<String, Option<String>>,
    pub(super) ranks: HashMap<String, usize>,
    pub(super) parameters: Vec<String>,
    pub(super) scene: Vec<String>,
    pub(super) collections: HashMap<(u8, Option<String>), Vec<String>>,
    pub(super) children: HashMap<String, Vec<String>>,
}
impl Document {
    pub(in crate::document) fn organization_index(&self) -> &OrganizationIndex {
        self.organization_index
            .get_or_init(|| {
                Arc::new({
                    let mut index = OrganizationIndex::default();
                    for g in self.display_info().parameter_groups.iter().flatten() {
                        index.owners.insert(g.id.clone(), g.parent_id.clone());
                        index
                            .collections
                            .entry((0, g.parent_id.clone()))
                            .or_default()
                            .push(g.id.clone());
                        if let Some(owner) = &g.parent_id {
                            index
                                .children
                                .entry(owner.clone())
                                .or_default()
                                .push(g.id.clone());
                        }
                    }
                    let mut seen = HashSet::new();
                    let memberships: HashMap<_, _> = self
                        .display_info()
                        .parameters
                        .iter()
                        .flatten()
                        .filter_map(|e| match e {
                            CdiParameterEntry::Resolved {
                                parameter_id,
                                group_id,
                                ..
                            } => Some((parameter_id.as_str(), group_id.clone())),
                            _ => None,
                        })
                        .collect();
                    index.parameters = self
                        .display_info()
                        .parameters
                        .iter()
                        .flatten()
                        .filter_map(|e| match e {
                            CdiParameterEntry::Resolved { parameter_id, .. } => Some(parameter_id),
                            _ => None,
                        })
                        .chain(self.parameter_order())
                        .filter(|id| seen.insert(id.as_str()))
                        .cloned()
                        .collect();
                    for id in &index.parameters {
                        let owner = memberships.get(id.as_str()).cloned().flatten();
                        index.owners.insert(id.clone(), owner.clone());
                        index
                            .collections
                            .entry((1, owner.clone()))
                            .or_default()
                            .push(id.clone());
                        if let Some(owner) = owner {
                            index.children.entry(owner).or_default().push(id.clone());
                        }
                    }
                    seen.clear();
                    let ranked = self
                        .hierarchy_order()
                        .organization
                        .iter()
                        .chain(self.part_order())
                        .chain(self.transform_order())
                        .chain(self.mesh_order())
                        .chain(self.offscreen_order())
                        .filter(|id| seen.insert(id.as_str()));
                    for id in ranked {
                        let owner = self
                            .scene_organization_owner(id)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned);
                        index.owners.insert(id.clone(), owner.clone());
                        index
                            .collections
                            .entry((2, owner.clone()))
                            .or_default()
                            .push(id.clone());
                        if let Some(owner) = owner {
                            index.children.entry(owner).or_default().push(id.clone());
                        }
                    }
                    fn visit_scene(owner: Option<String>, index: &mut OrganizationIndex) {
                        for id in index
                            .collections
                            .get(&(2, owner))
                            .cloned()
                            .unwrap_or_default()
                        {
                            index.scene.push(id.clone());
                            visit_scene(Some(id), index);
                        }
                    }
                    visit_scene(None, &mut index);
                    let mut ordered = Vec::new();
                    fn visit_group(
                        owner: Option<String>,
                        index: &OrganizationIndex,
                        out: &mut Vec<String>,
                    ) {
                        for id in index.collections.get(&(0, owner)).into_iter().flatten() {
                            out.push(id.clone());
                            out.extend(
                                index
                                    .collections
                                    .get(&(1, Some(id.clone())))
                                    .into_iter()
                                    .flatten()
                                    .cloned(),
                            );
                            visit_group(Some(id.clone()), index, out);
                        }
                    }
                    visit_group(None, &index, &mut ordered);
                    ordered.extend(
                        index
                            .collections
                            .get(&(1, None))
                            .into_iter()
                            .flatten()
                            .cloned(),
                    );
                    ordered.extend(index.scene.iter().cloned());
                    index.ranks = ordered
                        .into_iter()
                        .enumerate()
                        .map(|(rank, id)| (id, rank))
                        .collect();
                    index
                })
            })
            .as_ref()
    }
}
pub(in crate::document) fn same_metadata_shape(a: &DisplayInfo, b: &DisplayInfo) -> bool {
    a.parameter_groups
        .iter()
        .flatten()
        .map(|g| (&g.id, &g.parent_id))
        .eq(b
            .parameter_groups
            .iter()
            .flatten()
            .map(|g| (&g.id, &g.parent_id)))
        && a.parameters
            .iter()
            .flatten()
            .filter_map(|e| match e {
                CdiParameterEntry::Resolved {
                    parameter_id,
                    group_id,
                    ..
                } => Some((parameter_id, group_id)),
                _ => None,
            })
            .eq(b.parameters.iter().flatten().filter_map(|e| match e {
                CdiParameterEntry::Resolved {
                    parameter_id,
                    group_id,
                    ..
                } => Some((parameter_id, group_id)),
                _ => None,
            }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_and_label_history_reuse_relationship_cache_but_membership_invalidates() {
        let id = |n| format!("00000000-0000-4000-8000-{n:012x}");
        let mut d = Document::new();
        assert!(d
            .initialize(
                id(1),
                crate::Canvas::new(10., 10., crate::Vec2::default(), 1.)
            )
            .is_ok());
        let group = CdiParameterGroup {
            id: id(2),
            runtime_id: "Group".into(),
            name: "Before".into(),
            parent_id: None,
            extensions: Default::default(),
        };
        let plan = d
            .plan_organization_edit(OrganizationEdit::CreateGroup {
                group: group.clone(),
                before: None,
            })
            .unwrap();
        assert!(d.apply_organization_plan(&plan).status.is_ok());
        let plan = d
            .plan_organization_edit(OrganizationEdit::CreateParameter {
                parameter: Parameter {
                    id: id(3),
                    ..Default::default()
                },
                group_id: None,
                before: None,
            })
            .unwrap();
        assert!(d.apply_organization_plan(&plan).status.is_ok());
        d.organization_index();
        let cache = d.organization_index.get().unwrap().clone();
        let mut checkpoint = d.checkpoint();
        let mut renamed = group.clone();
        renamed.name = "After".into();
        let plan = d
            .plan_organization_edit(OrganizationEdit::PatchGroup {
                expected: group,
                group: renamed,
            })
            .unwrap();
        assert!(d.apply_organization_plan(&plan).status.is_ok());
        d.organization_index();
        assert!(Arc::ptr_eq(&cache, d.organization_index.get().unwrap()));
        d.exchange_checkpoint(&mut checkpoint).unwrap();
        d.organization_index();
        assert!(Arc::ptr_eq(&cache, d.organization_index.get().unwrap()));
        let plan = d
            .plan_organization_edit(OrganizationEdit::Move {
                ids: vec![id(3)],
                destination: OrganizationCollection::Parameters {
                    group_id: Some(id(2)),
                },
                before: None,
            })
            .unwrap();
        assert!(d.apply_organization_plan(&plan).status.is_ok());
        d.organization_index();
        assert!(!Arc::ptr_eq(&cache, d.organization_index.get().unwrap()));
        assert_eq!(d.organization_owner(&id(3)), Some(id(2).as_str()));
    }
}
