use super::*;
use kasane_core::{
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
    let entity = |kind, id: &str| EntityRef {
        document_id: after.id().into(),
        kind,
        id: id.into(),
    };
    let mut candidates: BTreeSet<String> = object_ids.iter().cloned().collect();
    candidates.extend(out.created.iter().chain(&out.deleted).map(|e| e.id.clone()));
    let mut field = |kind, id: &str, tag, changed: bool| {
        if changed {
            out.scene_fields.push(SceneFieldChange {
                entity: entity(kind, id),
                field: tag,
            });
            out.modified.push(entity(kind, id));
            out.categories.insert(if tag == SceneField::Label {
                ChangeCategory::Labels
            } else if matches!(
                tag,
                SceneField::Texture
                    | SceneField::Masks
                    | SceneField::GlueEndpoints
                    | SceneField::Context
            ) {
                ChangeCategory::References
            } else {
                ChangeCategory::Content
            });
        }
    };
    for id in candidates {
        let id = id.as_str();
        let kind = crate::scene_authoring::kind(after, id)
            .or_else(|| crate::scene_authoring::kind(before, id));
        let Some(kind) = kind else { continue };
        use SceneField as F;
        let a = before.get_mesh(id);
        let b = after.get_mesh(id);
        if a.is_some() || b.is_some() {
            field(kind, id, F::Label, a.map(|v| &v.name) != b.map(|v| &v.name));
            field(
                kind,
                id,
                F::RuntimeId,
                a.map(|v| &v.runtime_id) != b.map(|v| &v.runtime_id),
            );
            field(
                kind,
                id,
                F::Enabled,
                a.map(|v| v.enabled) != b.map(|v| v.enabled),
            );
            field(
                kind,
                id,
                F::Appearance,
                a.map(|v| v.appearance) != b.map(|v| v.appearance),
            );
            field(
                kind,
                id,
                F::Texture,
                a.map(|v| &v.texture_asset_id) != b.map(|v| &v.texture_asset_id),
            );
            field(
                kind,
                id,
                F::Masks,
                a.map(|v| &v.masks) != b.map(|v| &v.masks),
            );
            field(
                kind,
                id,
                F::MeshDrawing,
                a.map(|v| {
                    (
                        v.draw_order,
                        v.blend_mode,
                        v.raw_blend_mode,
                        v.double_sided,
                        v.inverted_mask,
                    )
                }) != b.map(|v| {
                    (
                        v.draw_order,
                        v.blend_mode,
                        v.raw_blend_mode,
                        v.double_sided,
                        v.inverted_mask,
                    )
                }),
            );
            field(
                kind,
                id,
                F::Context,
                a.map(|v| (&v.part_id, &v.deformer_id, &v.vertex_ids, &v.triangles))
                    != b.map(|v| (&v.part_id, &v.deformer_id, &v.vertex_ids, &v.triangles)),
            );
        }
        let a = before.get_transform(id);
        let b = after.get_transform(id);
        if a.is_some() || b.is_some() {
            field(kind, id, F::Label, a.map(|v| &v.name) != b.map(|v| &v.name));
            field(
                kind,
                id,
                F::RuntimeId,
                a.map(|v| &v.runtime_id) != b.map(|v| &v.runtime_id),
            );
            field(
                kind,
                id,
                F::Enabled,
                a.map(|v| v.enabled) != b.map(|v| v.enabled),
            );
            field(
                kind,
                id,
                F::Appearance,
                a.map(|v| v.appearance) != b.map(|v| v.appearance),
            );
            field(
                kind,
                id,
                F::Warp,
                a.and_then(|v| v.warp()) != b.and_then(|v| v.warp()),
            );
            field(
                kind,
                id,
                F::Rotation,
                a.and_then(|v| v.rotation()) != b.and_then(|v| v.rotation()),
            );
            field(
                kind,
                id,
                F::Context,
                a.map(|v| {
                    (
                        &v.part_id,
                        &v.parent_id,
                        v.warp().map(|w| (w.rows, w.columns, w.quad)),
                    )
                }) != b.map(|v| {
                    (
                        &v.part_id,
                        &v.parent_id,
                        v.warp().map(|w| (w.rows, w.columns, w.quad)),
                    )
                }),
            );
        }
        let a = before.get_offscreen(id);
        let b = after.get_offscreen(id);
        if a.is_some() || b.is_some() {
            field(kind, id, F::Label, a.map(|v| &v.name) != b.map(|v| &v.name));
            field(
                kind,
                id,
                F::RuntimeId,
                a.map(|v| &v.runtime_id) != b.map(|v| &v.runtime_id),
            );
            field(
                kind,
                id,
                F::Enabled,
                a.map(|v| v.enabled) != b.map(|v| v.enabled),
            );
            field(
                kind,
                id,
                F::Masks,
                a.map(|v| &v.masks) != b.map(|v| &v.masks),
            );
            field(
                kind,
                id,
                F::Offscreen,
                a.map(|v| (v.blend_mode, v.flags, &v.keyforms, &v.part_keyform_indices))
                    != b.map(|v| (v.blend_mode, v.flags, &v.keyforms, &v.part_keyform_indices)),
            );
            field(
                kind,
                id,
                F::Context,
                a.map(|v| &v.part_id) != b.map(|v| &v.part_id),
            );
        }
        let a = before.get_glue(id);
        let b = after.get_glue(id);
        if a.is_some() || b.is_some() {
            field(kind, id, F::Label, a.map(|v| &v.name) != b.map(|v| &v.name));
            field(
                kind,
                id,
                F::RuntimeId,
                a.map(|v| &v.runtime_id) != b.map(|v| &v.runtime_id),
            );
            field(
                kind,
                id,
                F::GlueIntensity,
                a.map(|v| v.intensity) != b.map(|v| v.intensity),
            );
            field(
                kind,
                id,
                F::GlueEndpoints,
                a.map(|v| (&v.mesh_a_id, &v.mesh_b_id)) != b.map(|v| (&v.mesh_a_id, &v.mesh_b_id)),
            );
            field(
                kind,
                id,
                F::Context,
                a.map(|v| (&v.pairs, &v.mesh_a_id, &v.mesh_b_id))
                    != b.map(|v| (&v.pairs, &v.mesh_a_id, &v.mesh_b_id)),
            );
        }
    }
    drop(field);
    for change in &out.scene_fields {
        if !matches!(
            change.field,
            SceneField::Texture
                | SceneField::Masks
                | SceneField::GlueEndpoints
                | SceneField::Context
        ) {
            continue;
        }
        let values = |d: &Document| -> Vec<SceneReference> {
            match d.scene_field(&change.entity.id, change.field) {
                Ok(kasane_core::document::SceneFieldValue::Texture(target)) => vec![target],
                Ok(kasane_core::document::SceneFieldValue::Masks(targets)) => {
                    targets.into_iter().map(Into::into).collect()
                }
                Ok(kasane_core::document::SceneFieldValue::GlueEndpoints { a, b }) => vec![a, b],
                Ok(kasane_core::document::SceneFieldValue::Context { part_id, parent_id }) => vec![
                    part_id.unwrap_or_default().into(),
                    parent_id.unwrap_or_default().into(),
                ],
                _ => vec![],
            }
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
            .unwrap_or_else(|| kasane_core::draw_order::resolved_groups(before));
        let b = after
            .draw_order_groups()
            .map(|g| g.to_vec())
            .unwrap_or_else(|| kasane_core::draw_order::resolved_groups(after));
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
    out.modified
        .retain(|e| !out.created.contains(e) && !out.deleted.contains(e));
}
