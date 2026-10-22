use super::*;
use crate::reference_authoring::kind;
use kasane_core::document::{
    ReferenceCollection as C, ReferenceFields as F, ReferenceSelectionField as S,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceFieldChange {
    pub entity: EntityRef,
    pub before_owner: Option<String>,
    pub after_owner: Option<String>,
    pub before: Option<F>,
    pub after: Option<F>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceSelectionChange {
    pub entity: EntityRef,
    pub field: S,
    pub before: Vec<String>,
    pub after: Vec<String>,
}
pub(super) fn summarize_references(before: &Document, after: &Document, out: &mut ChangeSet) {
    let ids = before
        .reference_identities()
        .iter()
        .chain(after.reference_identities())
        .map(|i| i.id.as_str())
        .collect::<BTreeSet<_>>();
    let entity = |kind, id: &str| EntityRef {
        document_id: after.id().into(),
        kind,
        id: id.into(),
    };
    for id in ids {
        let k = kind(after, id).or_else(|| kind(before, id)).unwrap();
        let a = before.reference_fields(id).ok();
        let b = after.reference_fields(id).ok();
        let same_fields = a == b;
        let owner = |d: &Document| d.reference_identity(id).and_then(|i| i.owner.clone());
        if a != b || owner(before) != owner(after) {
            let change = ReferenceFieldChange {
                entity: entity(k, id),
                before_owner: owner(before),
                after_owner: owner(after),
                before: a,
                after: b,
            };
            let label = match (&change.before, &change.after) {
                (
                    Some(F::ModelParameterGroup { name: a }),
                    Some(F::ModelParameterGroup { name: b }),
                ) => a != b,
                (
                    Some(F::ModelHitArea { name: a, mesh: ma }),
                    Some(F::ModelHitArea { name: b, mesh: mb }),
                ) => a != b && ma == mb,
                _ => false,
            };
            out.categories.insert(if label {
                ChangeCategory::Labels
            } else {
                ChangeCategory::References
            });
            out.modified.push(change.entity.clone());
            out.reference_fields.push(change);
        }
        // Root records also carry extension payloads and owned row arrays.
        let root_changed = match k {
            ObjectKind::ModelParameterGroup => {
                before.get_model_parameter_group(id) != after.get_model_parameter_group(id)
            }
            ObjectKind::ModelHitArea => {
                before.get_model_hit_area(id) != after.get_model_hit_area(id)
            }
            ObjectKind::CdiCombinedSet => before.get_combined_set(id) != after.get_combined_set(id),
            _ => false,
        };
        if root_changed {
            out.modified.push(entity(k, id));
            if same_fields {
                out.categories.insert(ChangeCategory::Content);
            }
        }
    }
    let collections = [
        C::ModelParameterGroups,
        C::ModelHitAreas,
        C::CdiCombinedSets,
    ]
    .into_iter()
    .chain(
        before
            .model3_settings()
            .groups
            .iter()
            .flatten()
            .chain(after.model3_settings().groups.iter().flatten())
            .map(|g| C::ModelParameterMembers {
                group_id: g.id.clone(),
            }),
    )
    .chain(
        before
            .display_info()
            .combined_parameters
            .iter()
            .flatten()
            .chain(after.display_info().combined_parameters.iter().flatten())
            .map(|s| C::CdiCombinedMembers {
                set_id: s.id.clone(),
            }),
    );
    for c in collections {
        if before.reference_collection(&c).ok() != after.reference_collection(&c).ok() {
            let collection = CollectionRef::References {
                document_id: after.id().into(),
                collection: c,
            };
            if !out.collections.contains(&collection) {
                out.collections.push(collection);
                out.categories.insert(ChangeCategory::Order);
            }
        }
    }
    for field in [S::MeshMasks, S::OffscreenMasks, S::BlendConstraints] {
        let ids: BTreeSet<_> = match field {
            S::MeshMasks => before
                .mesh_order()
                .iter()
                .chain(after.mesh_order())
                .collect(),
            S::OffscreenMasks => before
                .offscreen_order()
                .iter()
                .chain(after.offscreen_order())
                .collect(),
            S::BlendConstraints => before
                .blend_binding_order()
                .iter()
                .chain(after.blend_binding_order())
                .collect(),
        };
        for id in ids {
            let a = before.reference_selection(id, field).unwrap_or_default();
            let b = after.reference_selection(id, field).unwrap_or_default();
            if a != b {
                let k = match field {
                    S::MeshMasks => ObjectKind::Mesh,
                    S::OffscreenMasks => ObjectKind::Offscreen,
                    S::BlendConstraints => ObjectKind::BlendBinding,
                };
                out.reference_selections.push(ReferenceSelectionChange {
                    entity: entity(k, id),
                    field,
                    before: a.to_vec(),
                    after: b.to_vec(),
                });
                out.categories.insert(ChangeCategory::References);
            }
        }
    }
}
impl ReferenceFieldChange {
    pub(super) fn extra_bytes(&self) -> usize {
        let fields = |v: &F| match v {
            F::ModelParameterGroup { name } => name.capacity(),
            F::ModelHitArea { name, mesh } => name.capacity() + model_bytes(mesh),
            F::ModelParameterMember { target } => model_bytes(target),
            F::CdiCombinedMember { target } => match target {
                kasane_core::document::CdiParameterRef::Unspecified => 0,
                kasane_core::document::CdiParameterRef::Resolved { parameter_id } => {
                    parameter_id.capacity()
                }
                kasane_core::document::CdiParameterRef::Unresolved { runtime_id } => {
                    runtime_id.capacity()
                }
            },
        };
        self.entity.document_id.capacity()
            + self.entity.id.capacity()
            + self.before_owner.as_ref().map_or(0, String::capacity)
            + self.after_owner.as_ref().map_or(0, String::capacity)
            + self.before.as_ref().map_or(0, fields)
            + self.after.as_ref().map_or(0, fields)
    }
}
fn model_bytes(v: &kasane_core::document::ModelTargetRef) -> usize {
    match v {
        kasane_core::document::ModelTargetRef::Unspecified => 0,
        kasane_core::document::ModelTargetRef::Resolved { object_id } => object_id.capacity(),
        kasane_core::document::ModelTargetRef::Unresolved { runtime_id } => runtime_id.capacity(),
    }
}
