use crate::document::{
    DisabledSceneItem, SceneDisabledReason, ScenePairSlot, SceneProjection, SceneUse,
};
use std::collections::HashMap;
pub(crate) fn build_scene_projection(doc: &impl super::EvaluationModel) -> SceneProjection {
    let mut out = SceneProjection {
        geometry: doc.mesh_order().to_vec(),
        ..Default::default()
    };
    let mut blocked: HashMap<String, Vec<String>> = HashMap::new();
    for id in doc.mesh_order() {
        let mesh = doc.get_mesh(id).unwrap();
        let reason = match &mesh.texture_asset_id {
            crate::SceneReference::Unspecified => Some(SceneDisabledReason::UnspecifiedReference),
            crate::SceneReference::Unresolved { runtime_id } => {
                Some(SceneDisabledReason::UnresolvedReference {
                    runtime_id: runtime_id.clone(),
                })
            }
            crate::SceneReference::Resolved { .. }
                if mesh.vertex_ids.is_empty() || mesh.triangles.is_empty() =>
            {
                Some(SceneDisabledReason::EmptyGeometry)
            }
            crate::SceneReference::Resolved { object_id } if !doc.has_image_content(object_id) => {
                Some(SceneDisabledReason::MissingImageContent {
                    asset_id: object_id.clone(),
                })
            }
            _ => None,
        };
        if let Some(reason) = reason {
            blocked.insert(id.clone(), vec![id.clone()]);
            out.disabled.push(DisabledSceneItem {
                id: id.clone(),
                usage: SceneUse::Drawing,
                reason,
                chain: vec![id.clone()],
            });
        }
    }
    // Monotone propagation also handles legal mask cycles: an available
    // cycle remains available, while an unavailable source disables its users.
    loop {
        let mut changed = false;
        for (id, masks, owners, usage) in doc
            .mesh_order()
            .iter()
            .map(|id| {
                let m = doc.get_mesh(id).unwrap();
                let owners = doc
                    .offscreen_order()
                    .iter()
                    .filter(|os| {
                        doc.is_part_ancestor(&doc.get_offscreen(os).unwrap().part_id, &m.part_id)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                (id, &m.masks, owners, SceneUse::Drawing)
            })
            .chain(doc.offscreen_order().iter().map(|id| {
                let os = doc.get_offscreen(id).unwrap();
                (
                    id,
                    &os.masks,
                    doc.parent_offscreen_for_part(&os.part_id)
                        .map(|parent| vec![parent.id.clone()])
                        .unwrap_or_default(),
                    SceneUse::Composite,
                )
            }))
        {
            if blocked.contains_key(id) {
                continue;
            }
            if let Some(source) = masks
                .iter()
                .chain(owners.iter())
                .find(|target| blocked.contains_key(*target))
            {
                let mut chain = vec![id.clone()];
                chain.extend(blocked[source].clone());
                blocked.insert(id.clone(), chain.clone());
                out.disabled.push(DisabledSceneItem {
                    id: id.clone(),
                    usage,
                    reason: SceneDisabledReason::DependencyUnavailable {
                        object_id: source.clone(),
                    },
                    chain,
                });
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    out.drawing = doc
        .mesh_order()
        .iter()
        .filter(|id| !blocked.contains_key(*id))
        .cloned()
        .collect();
    out.composites = doc
        .offscreen_order()
        .iter()
        .filter(|id| !blocked.contains_key(*id))
        .cloned()
        .collect();
    for id in doc.glue_order() {
        let g = doc.get_glue(id).unwrap();
        for pair in &g.pairs {
            if pair.vertex_a.is_none() || pair.vertex_b.is_none() {
                out.disabled.push(DisabledSceneItem {
                    id: pair.id.clone(),
                    usage: SceneUse::GluePair,
                    reason: SceneDisabledReason::UnspecifiedReference,
                    chain: vec![g.id.clone(), pair.id.clone()],
                });
            }
        }
        let reason = [&g.mesh_a_id, &g.mesh_b_id]
            .iter()
            .find_map(|target| match target {
                crate::SceneReference::Unspecified => {
                    Some(SceneDisabledReason::UnspecifiedReference)
                }
                crate::SceneReference::Unresolved { runtime_id } => {
                    Some(SceneDisabledReason::UnresolvedReference {
                        runtime_id: runtime_id.clone(),
                    })
                }
                crate::SceneReference::Resolved { object_id }
                    if doc
                        .get_mesh(object_id)
                        .is_none_or(|m| m.vertex_ids.is_empty()) =>
                {
                    Some(SceneDisabledReason::DependencyUnavailable {
                        object_id: object_id.clone(),
                    })
                }
                _ => None,
            });
        if let Some(reason) = reason {
            out.disabled.push(DisabledSceneItem {
                id: id.clone(),
                usage: SceneUse::Glue,
                reason,
                chain: vec![id.clone()],
            });
        } else {
            out.glues.push(id.clone());
            for (source_slot, pair) in g.pairs.iter().enumerate() {
                if pair.vertex_a.is_some() && pair.vertex_b.is_some() {
                    out.pairs.push(pair.id.clone());
                    out.pair_slots.insert(
                        pair.id.clone(),
                        ScenePairSlot {
                            glue_id: id.clone(),
                            source_slot,
                        },
                    );
                }
            }
        }
    }
    fn slots(ids: &[String]) -> HashMap<String, usize> {
        ids.iter()
            .enumerate()
            .map(|(slot, id)| (id.clone(), slot))
            .collect()
    }
    out.geometry_slots = slots(&out.geometry);
    out.drawing_slots = slots(&out.drawing);
    out.glue_slots = slots(&out.glues);
    out.composite_slots = slots(&out.composites);
    out
}
