//! Every domain receipt uses the same typed identity and location hints. These
//! hints never perform selection or activate animation.
use super::*;
impl Document {
    /// Stable view neighbors, captured before deletion. Domain deletion policy
    /// stays in the native plan; this list is only an inspection preference.
    pub fn authoring_neighbors(&self, object: &ObjectRef) -> Vec<ObjectRef> {
        let collection = match object.kind {
            ObjectKind::Parameter => Some(zipzi_sdk::OrganizationCollection::Parameters {
                group_id: self
                    .model()
                    .organization_owner(&object.id)
                    .map(str::to_owned),
            }),
            ObjectKind::ParameterGroup => {
                Some(zipzi_sdk::OrganizationCollection::ParameterGroups {
                    parent_id: self
                        .model()
                        .organization_owner(&object.id)
                        .map(str::to_owned),
                })
            }
            _ => None,
        };
        if let Some(collection) = collection {
            if let Ok(members) = self.session.organization_collection(collection) {
                if let Some(at) = members.iter().position(|h| h.id() == object.id) {
                    let mut result: Vec<_> = members[at + 1..]
                        .iter()
                        .chain(members[..at].iter().rev())
                        .map(|h| ObjectRef::new(ObjectKind::from_sdk(h.kind()), h.id()))
                        .collect();
                    if let Some(owner) = self.model().organization_owner(&object.id) {
                        result.push(ObjectRef::new(ObjectKind::ParameterGroup, owner));
                    }
                    return result;
                }
            }
        }
        let index = self.object_index();
        let owner = |object: &ObjectRef| {
            index
                .incoming(object)
                .find(|r| r.kind == crate::RelationKind::Contains)
                .map(|r| r.source.clone())
        };
        let parent = owner(object);
        let siblings: Vec<_> = index
            .objects
            .iter()
            .filter(|i| i.reference.kind == object.kind && owner(&i.reference) == parent)
            .map(|i| i.reference.clone())
            .collect();
        let Some(at) = siblings.iter().position(|r| r == object) else {
            return vec![];
        };
        siblings[at + 1..]
            .iter()
            .chain(siblings[..at].iter().rev())
            .cloned()
            .chain(parent)
            .collect()
    }
    pub(crate) fn authoring_result(&self, receipt: zipzi_sdk::EditReceipt) -> OperationResult {
        let reference = |entity: &zipzi_sdk::EntityRef| {
            ObjectRef::new(ObjectKind::from_sdk(entity.kind), &entity.id)
        };
        let suggested_selection: Vec<_> = receipt.changes.created.iter().map(reference).collect();
        let mut locations = suggested_selection.clone();
        locations.extend(receipt.changes.modified.iter().map(reference));
        for change in &receipt.changes.organization_references {
            locations.push(reference(&change.entity));
            if let Some(id) = &change.after {
                locations.push(ObjectRef::new(
                    if matches!(
                        change.entity.kind,
                        zipzi_sdk::ObjectKind::Parameter | zipzi_sdk::ObjectKind::CdiParameterGroup
                    ) {
                        ObjectKind::ParameterGroup
                    } else {
                        ObjectKind::Part
                    },
                    id,
                ));
            }
        }
        // IDs in collection receipts are globally unique. Resolve their typed
        // projection once rather than teaching every domain result adapter a
        // second identity table.
        let index = self.object_index();
        for collection in &receipt.changes.collections {
            use zipzi_sdk::{
                CollectionRef as C, OrganizationCollection as O, ReferenceCollection as R,
                SceneCollection as S,
            };
            let owner = match collection {
                C::ExpressionEntries { expression_id, .. } => Some(expression_id),
                C::SceneContent { owner_id, .. } => Some(owner_id),
                C::BindingGrid { binding_id, .. } => Some(binding_id),
                C::DeformationChildren { parent_id, .. } => Some(parent_id),
                C::Pose { owner, .. } | C::Physics { owner, .. } | C::Motion { owner, .. } => {
                    owner.as_ref()
                }
                C::Organization { collection, .. } => match collection {
                    O::Parameters { group_id } => group_id.as_ref(),
                    O::ParameterGroups { parent_id } => parent_id.as_ref(),
                    O::PartMembers { part_id } => part_id.as_ref(),
                },
                C::Scene { collection, .. } => match collection {
                    S::Organization { part_id } => part_id.as_ref(),
                    S::Deformation { parent_id } => parent_id.as_ref(),
                    S::Drawing { owner } => Some(owner),
                    S::Glues | S::DrawingGroups => None,
                },
                C::References { collection, .. } => match collection {
                    R::ModelParameterMembers { group_id } => Some(group_id),
                    R::CdiCombinedMembers { set_id } => Some(set_id),
                    R::ModelParameterGroups | R::ModelHitAreas | R::CdiCombinedSets => None,
                },
                C::Resources { .. } | C::Expressions { .. } => None,
            };
            if let Some(owner) = owner {
                if let Some(info) = index.objects.iter().find(|i| i.reference.id == *owner) {
                    locations.push(info.reference.clone());
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        locations.retain(|object| seen.insert(object.clone()));
        OperationResult {
            receipt,
            suggested_selection,
            locations,
        }
    }
}
