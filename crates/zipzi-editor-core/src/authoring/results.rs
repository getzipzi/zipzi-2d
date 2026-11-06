//! Every domain receipt uses the same typed identity and location hints. These
//! hints never perform selection or activate animation.
use super::*;
impl Document {
    /// Stable view neighbors, captured before deletion. Domain deletion policy
    /// stays in the native plan; this list is only an inspection preference.
    pub fn authoring_neighbors(&self, object: &ObjectRef) -> Vec<ObjectRef> {
        let collection = match object.kind {
            ObjectKind::Parameter => Some(
                zipzi_contract::document::OrganizationCollection::Parameters {
                    group_id: self
                        .model()
                        .organization_owner(&object.id)
                        .map(str::to_owned),
                },
            ),
            ObjectKind::ParameterGroup => Some(
                zipzi_contract::document::OrganizationCollection::ParameterGroups {
                    parent_id: self
                        .model()
                        .organization_owner(&object.id)
                        .map(str::to_owned),
                },
            ),
            _ => None,
        };
        if let Some(collection) = collection {
            if let Ok(members) = self.model().organization_collection(&collection) {
                if let Some(at) = members.iter().position(|h| h == &object.id) {
                    let mut result: Vec<_> = members[at + 1..]
                        .iter()
                        .chain(members[..at].iter().rev())
                        .map(|id| ObjectRef::new(object.kind, id))
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
    pub(crate) fn authoring_result(&self, receipt: zipzi_sdk::Publication) -> OperationResult {
        use zipzi_core::state::{RecordChange, RecordDomain, RootDomain};
        let reference = |record: &zipzi_core::state::RecordWrite| {
            let kind = match record.domain {
                RecordDomain::Asset => ObjectKind::Texture,
                RecordDomain::Parameter => ObjectKind::Parameter,
                RecordDomain::Part => ObjectKind::Part,
                RecordDomain::Transform => ObjectKind::Deformer,
                RecordDomain::Mesh => ObjectKind::Mesh,
                RecordDomain::Binding => ObjectKind::MeshBinding,
                RecordDomain::SceneBinding => ObjectKind::SceneBinding,
                RecordDomain::BlendKeyTable => ObjectKind::BlendKeyTable,
                RecordDomain::BlendConstraint => ObjectKind::BlendConstraint,
                RecordDomain::BlendBinding => ObjectKind::BlendBinding,
                RecordDomain::Glue => ObjectKind::Glue,
                RecordDomain::Offscreen => ObjectKind::Offscreen,
                RecordDomain::Expression => ObjectKind::Expression,
                RecordDomain::Motion => ObjectKind::Motion,
            };
            ObjectRef::new(kind, &record.id)
        };
        let suggested_selection: Vec<_> = receipt
            .writes
            .records()
            .iter()
            .filter(|r| r.change == RecordChange::Created)
            .map(reference)
            .collect();
        let mut locations: Vec<_> = receipt
            .writes
            .records()
            .iter()
            .filter(|r| r.change != RecordChange::Deleted)
            .map(reference)
            .collect();
        let index = self.object_index();
        let changed = locations.clone();
        for object in changed {
            locations.extend(
                index
                    .incoming(&object)
                    .filter(|r| r.kind == crate::RelationKind::Contains)
                    .map(|r| r.source.clone()),
            );
            locations.extend(index.outgoing(&object).filter_map(|relation| {
                matches!(
                    relation.kind,
                    crate::RelationKind::PartOwner
                        | crate::RelationKind::DeformParent
                        | crate::RelationKind::ParameterGroup
                )
                .then(|| relation.target.clone())
            }));
        }
        let model = self.model();
        if receipt.writes.root_changed(RootDomain::Pose) {
            if let Some(pose) = model.pose() {
                locations.push(ObjectRef::new(ObjectKind::Pose, &pose.id));
            }
        }
        if receipt.writes.root_changed(RootDomain::Physics) {
            if let Some(physics) = model.physics() {
                locations.push(ObjectRef::new(ObjectKind::Physics, &physics.id));
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
