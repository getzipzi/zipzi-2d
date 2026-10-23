//! Availability is independent of geometry evaluation and authoring identity.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneUse {
    Geometry,
    Drawing,
    Glue,
    GluePair,
    Composite,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SceneDisabledReason {
    UnspecifiedReference,
    UnresolvedReference { runtime_id: String },
    EmptyGeometry,
    MissingImageContent { asset_id: String },
    DependencyUnavailable { object_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisabledSceneItem {
    pub id: String,
    pub usage: SceneUse,
    pub reason: SceneDisabledReason,
    pub chain: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SceneProjection {
    pub geometry: Vec<String>,
    pub drawing: Vec<String>,
    pub glues: Vec<String>,
    pub pairs: Vec<String>,
    /// Stable PairId -> original, uncompressed evaluation coordinates.
    pub pair_slots: HashMap<String, ScenePairSlot>,
    pub composites: Vec<String>,
    pub geometry_slots: HashMap<String, usize>,
    pub drawing_slots: HashMap<String, usize>,
    pub glue_slots: HashMap<String, usize>,
    pub composite_slots: HashMap<String, usize>,
    pub disabled: Vec<DisabledSceneItem>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenePairSlot {
    pub glue_id: String,
    pub source_slot: usize,
}
impl Document {
    pub fn scene_projection(&self) -> Arc<SceneProjection> {
        self.scene_projection
            .get_or_init(|| Arc::new(self.build_scene_projection()))
            .clone()
    }
    fn build_scene_projection(&self) -> SceneProjection {
        let mut out = SceneProjection {
            geometry: self.mesh_order().to_vec(),
            ..Default::default()
        };
        let mut blocked: HashMap<String, Vec<String>> = HashMap::new();
        for id in self.mesh_order() {
            let mesh = self.get_mesh(id).unwrap();
            let reason = match &mesh.texture_asset_id {
                crate::SceneReference::Unspecified => {
                    Some(SceneDisabledReason::UnspecifiedReference)
                }
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
                crate::SceneReference::Resolved { object_id }
                    if self
                        .get_asset(object_id)
                        .is_none_or(|a| a.content.is_none()) =>
                {
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
            for (id, masks, owners, usage) in self
                .mesh_order()
                .iter()
                .map(|id| {
                    let m = self.get_mesh(id).unwrap();
                    let owners = self
                        .offscreen_order()
                        .iter()
                        .filter(|os| {
                            self.is_part_ancestor(
                                &self.get_offscreen(os).unwrap().part_id,
                                &m.part_id,
                            )
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    (id, &m.masks, owners, SceneUse::Drawing)
                })
                .chain(self.offscreen_order().iter().map(|id| {
                    let os = self.get_offscreen(id).unwrap();
                    (
                        id,
                        &os.masks,
                        self.parent_offscreen_for_part(&os.part_id)
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
        out.drawing = self
            .mesh_order()
            .iter()
            .filter(|id| !blocked.contains_key(*id))
            .cloned()
            .collect();
        out.composites = self
            .offscreen_order()
            .iter()
            .filter(|id| !blocked.contains_key(*id))
            .cloned()
            .collect();
        for id in self.glue_order() {
            let g = self.get_glue(id).unwrap();
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
                        if self
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
    /// Strict foreign formats cannot represent pending native scene resources.
    pub fn scene_export_omissions(&self) -> Vec<DisabledSceneItem> {
        let projection = self.scene_projection();
        // Empty geometry is representable in foreign scene formats (including
        // helper meshes used by Glue). Export readiness is stricter only for
        // missing configuration and consumers that depend on it.
        let pending: HashSet<_> = projection
            .disabled
            .iter()
            .filter(|v| {
                matches!(
                    v.reason,
                    SceneDisabledReason::UnspecifiedReference
                        | SceneDisabledReason::MissingImageContent { .. }
                        | SceneDisabledReason::UnresolvedReference { .. }
                )
            })
            .map(|v| v.id.as_str())
            .collect();
        let mut out: Vec<_> = projection
            .disabled
            .iter()
            .filter(|v| v.chain.iter().any(|id| pending.contains(id.as_str())))
            .cloned()
            .collect();
        let omitted: HashSet<_> = out.iter().map(|v| v.id.clone()).collect();
        for id in &projection.glues {
            let g = self.get_glue(id).unwrap();
            if let Some(source) = [&g.mesh_a_id, &g.mesh_b_id]
                .iter()
                .filter_map(|r| r.resolved_id())
                .find(|id| omitted.contains(*id))
            {
                out.push(DisabledSceneItem {
                    id: id.clone(),
                    usage: SceneUse::Glue,
                    reason: SceneDisabledReason::DependencyUnavailable {
                        object_id: source.into(),
                    },
                    chain: vec![id.clone(), source.into()],
                });
            }
        }
        out
    }
}
