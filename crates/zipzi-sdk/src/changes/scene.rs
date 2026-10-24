use super::*;
use zipzi_core::{
    document::{SceneCollection, SceneField},
    SceneReference,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneFieldChange {
    pub entity: EntityRef,
    pub field: SceneField,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneReferenceChange {
    pub entity: EntityRef,
    pub field: SceneField,
    pub before: Vec<SceneReference>,
    pub after: Vec<SceneReference>,
}
pub(super) fn summarize_scene(
    before: &Document,
    after: &Document,
    object_ids: &[String],
    out: &mut ChangeSet,
) {
    let entity = |kind, id: &str| EntityRef::new(after.id(), kind, id);
    let mut candidates: BTreeSet<String> = object_ids.iter().cloned().collect();
    candidates.extend(out.created.iter().chain(&out.deleted).map(|e| e.id.clone()));
    for id in candidates {
        let kind = crate::scene_authoring::kind(after, &id)
            .or_else(|| crate::scene_authoring::kind(before, &id));
        let Some(kind) = kind else { continue };
        before.visit_scene_field_changes(after, &id, |tag| {
            record_field_change(out, entity(kind, &id), tag);
        });
    }
    for change in &out.scene_fields {
        if !change.field.is_reference() {
            continue;
        }
        let values = |d: &Document| {
            d.scene_field(&change.entity.id, change.field)
                .ok()
                .and_then(zipzi_core::document::SceneFieldValue::into_references)
                .unwrap_or_default()
        };
        let a = values(before);
        let b = values(after);
        if a != b {
            out.scene_references.push(SceneReferenceChange {
                entity: change.entity.clone(),
                field: change.field,
                before: a,
                after: b,
            });
        }
    }
    for c in [SceneCollection::Glues] {
        if before.scene_collection(&c).ok() != after.scene_collection(&c).ok() {
            out.collections.push(CollectionRef::Scene {
                document_id: after.id().into(),
                collection: c,
            });
            out.categories.insert(ChangeCategory::Order);
        }
    }
    if before.hierarchy_order().deformation != after.hierarchy_order().deformation
        || before.transform_order() != after.transform_order()
        || before.mesh_order() != after.mesh_order()
        || out
            .scene_fields
            .iter()
            .any(|c| c.field == SceneField::Context)
    {
        let mut parents = BTreeSet::from([None]);
        for d in [before, after] {
            for id in d.transform_order().iter().chain(d.mesh_order()) {
                parents.insert(
                    d.scene_deformer_parent(id)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned),
                );
            }
        }
        for parent_id in parents {
            let c = SceneCollection::Deformation { parent_id };
            if before.scene_collection(&c).ok() != after.scene_collection(&c).ok() {
                out.collections.push(CollectionRef::Scene {
                    document_id: after.id().into(),
                    collection: c,
                });
                out.categories.insert(ChangeCategory::Order);
            }
        }
    }
    if before.draw_order_groups() != after.draw_order_groups()
        || before.mesh_order() != after.mesh_order()
        || before.part_order() != after.part_order()
    {
        let a = before
            .draw_order_groups()
            .map(|g| g.to_vec())
            .unwrap_or_else(|| zipzi_core::draw_order::resolved_groups(before));
        let b = after
            .draw_order_groups()
            .map(|g| g.to_vec())
            .unwrap_or_else(|| zipzi_core::draw_order::resolved_groups(after));
        let owners: BTreeSet<_> = a.iter().chain(&b).map(|g| &g.owner).collect();
        if before.drawing_group_owners() != after.drawing_group_owners() {
            out.collections.push(CollectionRef::Scene {
                document_id: after.id().into(),
                collection: SceneCollection::DrawingGroups,
            });
        }
        for owner in owners {
            let x = a.iter().find(|g| &g.owner == owner);
            let y = b.iter().find(|g| &g.owner == owner);
            if x != y {
                let role = entity(
                    ObjectKind::DrawOrderGroup,
                    if owner.is_empty() { after.id() } else { owner },
                );
                out.modified.push(role.clone());
                out.scene_fields.push(SceneFieldChange {
                    entity: role,
                    field: SceneField::Context,
                });
                if x.map(|g| &g.items) != y.map(|g| &g.items) {
                    out.collections.push(CollectionRef::Scene {
                        document_id: after.id().into(),
                        collection: SceneCollection::Drawing {
                            owner: owner.clone(),
                        },
                    });
                }
                out.categories.extend([
                    ChangeCategory::Order,
                    ChangeCategory::References,
                    ChangeCategory::EvaluationInputs,
                ]);
            }
        }
    }
    for id in before
        .part_order()
        .iter()
        .chain(after.part_order())
        .collect::<BTreeSet<_>>()
    {
        if before.get_part(id).map(|p| &p.name) != after.get_part(id).map(|p| &p.name)
            && after.drawing_group_owners().contains(id)
        {
            let role = entity(ObjectKind::DrawOrderGroup, id);
            out.modified.push(role.clone());
            out.scene_fields.push(SceneFieldChange {
                entity: role,
                field: SceneField::Label,
            });
        }
    }
    let only_scene = !object_ids.is_empty()
        && object_ids.iter().all(|id| {
            crate::scene_authoring::kind(before, id).is_some()
                || crate::scene_authoring::kind(after, id).is_some()
        });
    let drawing_only = !out.scene_fields.is_empty()
        && out
            .scene_fields
            .iter()
            .all(|f| f.entity.kind == ObjectKind::DrawOrderGroup)
        && out
            .modified
            .iter()
            .all(|e| e.kind == ObjectKind::DrawOrderGroup);
    if (only_scene || drawing_only)
        && out
            .created
            .iter()
            .chain(&out.deleted)
            .all(|e| e.kind == ObjectKind::DrawOrderGroup)
        && !out
            .scene_fields
            .iter()
            .any(|f| f.field == SceneField::Context && f.entity.kind != ObjectKind::DrawOrderGroup)
    {
        out.categories.remove(&ChangeCategory::Topology);
    }
    out.retain_existing_modified();
}

pub(super) fn record_field_change(out: &mut ChangeSet, entity: EntityRef, field: SceneField) {
    out.scene_fields.push(SceneFieldChange {
        entity: entity.clone(),
        field,
    });
    out.modified.push(entity);
    out.categories.insert(if field == SceneField::Label {
        ChangeCategory::Labels
    } else if field.is_reference() {
        ChangeCategory::References
    } else {
        ChangeCategory::Content
    });
}
