//! Immutable relationship/order cache. Labels and scalar edits do not invalidate it.
use super::*;
#[derive(Debug, Clone, Default)]
pub(crate) struct OrganizationIndex {
    pub(super) owners: HashMap<String, Option<String>>,
    pub(super) ranks: HashMap<String, usize>,
    pub(super) parameters: Vec<String>,
    pub(super) scene: Vec<String>,
    pub(super) collections: HashMap<(u8, Option<String>), Vec<String>>,
    pub(super) children: HashMap<String, Vec<String>>,
}
impl OrganizationIndex {
    pub(crate) fn parameter_ids(&self) -> &[String] {
        &self.parameters
    }
    pub(crate) fn owner(&self, id: &str) -> Option<&str> {
        self.owners.get(id).and_then(Option::as_deref)
    }
}
impl Model<'_> {
    pub(crate) fn shared_organization_index(&self) -> Arc<OrganizationIndex> {
        self.organization_index();
        self.organization_index.get().unwrap().clone()
    }
    pub(crate) fn organization_index(&self) -> &OrganizationIndex {
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
