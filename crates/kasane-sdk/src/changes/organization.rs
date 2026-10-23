use super::*;
use kasane_core::document::OrganizationCollection;
use std::collections::{BTreeMap, HashSet};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrganizationReferenceChange {
    pub entity: EntityRef,
    pub before: Option<String>,
    pub after: Option<String>,
}
fn collections(d: &Document) -> BTreeMap<(u8, Option<String>), Vec<String>> {
    let mut out: BTreeMap<_, Vec<String>> = BTreeMap::new();
    for g in d.display_info().parameter_groups.iter().flatten() {
        out.entry((0, g.parent_id.clone()))
            .or_default()
            .push(g.id.clone());
    }
    for id in d.ordered_parameter_ids() {
        out.entry((1, d.parameter_group_for(&id).map(str::to_owned)))
            .or_default()
            .push(id);
    }
    let mut seen = HashSet::new();
    for id in d
        .hierarchy_order()
        .organization
        .iter()
        .chain(d.part_order())
        .chain(d.transform_order())
        .chain(d.mesh_order())
        .chain(d.offscreen_order())
        .filter(|id| seen.insert(id.as_str()))
    {
        out.entry((2, d.organization_owner(id).map(str::to_owned)))
            .or_default()
            .push(id.clone());
    }
    out
}
pub(super) fn summarize_organization(before: &Document, after: &Document, out: &mut ChangeSet) {
    let entity = |kind, id: &str| EntityRef::new(after.id(), kind, id);
    let a = collections(before);
    let b = collections(after);
    for (key, old, new) in paired_records(&a, &b) {
        if old.map(Vec::as_slice).unwrap_or_default() != new.map(Vec::as_slice).unwrap_or_default()
        {
            let collection = match key.0 {
                0 => OrganizationCollection::ParameterGroups {
                    parent_id: key.1.clone(),
                },
                1 => OrganizationCollection::Parameters {
                    group_id: key.1.clone(),
                },
                _ => OrganizationCollection::PartMembers {
                    part_id: key.1.clone(),
                },
            };
            out.collections.push(CollectionRef::Organization {
                document_id: after.id().into(),
                collection,
            });
            out.categories.insert(ChangeCategory::Order);
        }
    }
    for id in before
        .parameter_order()
        .iter()
        .chain(after.parameter_order())
        .chain(before.part_order())
        .chain(after.part_order())
        .chain(before.transform_order())
        .chain(after.transform_order())
        .chain(before.mesh_order())
        .chain(after.mesh_order())
        .chain(before.offscreen_order())
        .chain(after.offscreen_order())
        .map(String::as_str)
        .chain(
            before
                .display_info()
                .parameter_groups
                .iter()
                .flatten()
                .map(|g| g.id.as_str()),
        )
        .chain(
            after
                .display_info()
                .parameter_groups
                .iter()
                .flatten()
                .map(|g| g.id.as_str()),
        )
        .collect::<BTreeSet<_>>()
    {
        let kind = crate::organization_authoring::kind(after, id)
            .or_else(|| crate::organization_authoring::kind(before, id))
            .unwrap();
        let old = before.organization_owner(id).map(str::to_owned);
        let new = after.organization_owner(id).map(str::to_owned);
        if old != new {
            out.organization_references
                .push(OrganizationReferenceChange {
                    entity: entity(kind, id),
                    before: old,
                    after: new,
                });
            out.modified.push(entity(kind, id));
            out.categories.insert(ChangeCategory::References);
            if !matches!(kind, ObjectKind::Parameter | ObjectKind::CdiParameterGroup) {
                out.categories.insert(ChangeCategory::EvaluationInputs);
            }
        }
        let changed = match kind {
            ObjectKind::CdiParameterGroup => {
                let a = before.get_parameter_group(id);
                let b = after.get_parameter_group(id);
                if a.map(|g| (&g.name, &g.runtime_id)) != b.map(|g| (&g.name, &g.runtime_id)) {
                    out.categories.insert(ChangeCategory::Labels);
                }
                if a.map(|g| &g.extensions) != b.map(|g| &g.extensions) {
                    out.categories.insert(ChangeCategory::Content);
                }
                a != b
            }
            ObjectKind::Parameter => {
                let a = before.get_parameter(id);
                let b = after.get_parameter(id);
                if a.map(|p| &p.name) != b.map(|p| &p.name) {
                    out.categories.insert(ChangeCategory::Labels);
                }
                if a.map(|p| {
                    (
                        &p.runtime_id,
                        p.minimum,
                        p.maximum,
                        p.default_value,
                        p.decimal_places,
                        p.kind,
                        p.repeat,
                    )
                }) != b.map(|p| {
                    (
                        &p.runtime_id,
                        p.minimum,
                        p.maximum,
                        p.default_value,
                        p.decimal_places,
                        p.kind,
                        p.repeat,
                    )
                }) {
                    out.categories.insert(ChangeCategory::EvaluationInputs);
                }
                a != b
            }
            ObjectKind::Part => {
                let a = before.get_part(id);
                let b = after.get_part(id);
                if a.map(|p| &p.name) != b.map(|p| &p.name) {
                    out.categories.insert(ChangeCategory::Labels);
                }
                if a.map(|p| (&p.runtime_id, &p.parent_id, p.enabled, p.draw_order))
                    != b.map(|p| (&p.runtime_id, &p.parent_id, p.enabled, p.draw_order))
                {
                    out.categories.insert(ChangeCategory::EvaluationInputs);
                }
                a != b
            }
            _ => false,
        };
        if changed {
            out.modified.push(entity(kind, id));
        }
    }
    if before.hierarchy_order().organization != after.hierarchy_order().organization {
        out.categories.insert(ChangeCategory::Order);
    }
    if before.draw_order_groups() != after.draw_order_groups() {
        out.categories
            .extend([ChangeCategory::References, ChangeCategory::EvaluationInputs]);
    }
    // Display rows own only labels and membership, never the parameter objects.
    if before.display_info().parameters != after.display_info().parameters {
        out.categories.insert(ChangeCategory::Content);
    }
    out.retain_existing_modified();
}
