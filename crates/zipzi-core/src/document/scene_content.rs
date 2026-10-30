//! Dedicated geometry and scene-content edits. Array positions are topology
//! coordinates, never object identities. Every plan binds the native snapshot.
use super::organization::Delta;
use super::*;
use crate::{GlueVertexPair, OffscreenKeyform, Vec2, VertexId, WarpBezier};
use std::collections::{BTreeMap, HashSet};
mod address;
mod effects;
mod mesh;
mod warp;
pub use address::*;

pub use zipzi_contract::document::{
    NewMeshVertex, OffscreenDeletePolicy, OffscreenMappingPatch, PartKeyformSlot,
    SceneContentDomain, SceneContentEdit, SceneContentReport, ShapeTarget, VertexContentPatch,
    VertexDeletePolicy, VertexSeed, VertexWeight, WarpPointPatch,
};

#[derive(Debug, Clone)]
pub struct SceneContentPlan {
    domain: OrganizationPlan,
    report: SceneContentReport,
    kind: ChangeKind,
    cost: ConversionCost,
}
impl SceneContentPlan {
    pub fn is_noop(&self) -> bool {
        self.domain.no_op
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.domain.affected
    }
    pub fn report(&self) -> &SceneContentReport {
        &self.report
    }
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
}
impl Document {
    pub fn plan_scene_content(&self, edit: SceneContentEdit) -> Result<SceneContentPlan, Status> {
        self.plan_scene_content_with_control(
            edit,
            AuthoringLimits::default(),
            &ConversionControl::default(),
        )
    }
    pub fn plan_scene_content_with_control(
        &self,
        edit: SceneContentEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<SceneContentPlan, Status> {
        control.check()?;
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        // Includes isolated validation and history storage. Recheck at publication.
        let (work, growth) = self.content_growth(&edit)?;
        let retained = self.estimated_content_bytes();
        let payload = retained.checked_add(growth).ok_or_else(|| {
            Status::error("AUTHORING_CAPACITY_OVERFLOW", "Content payload overflows")
        })?;
        let cost = limits.check_conversion(work, payload, retained)?;
        let mut plan = SceneContentPlan {
            domain: OrganizationPlan {
                baseline: self.snapshot_identity.clone(),
                revision: self.revision(),
                delta: Delta::default(),
                affected: vec![],
                copied: BTreeMap::new(),
                effects: Default::default(),
                no_op: true,
            },
            report: Default::default(),
            kind: ChangeKind::Structure,
            cost,
        };
        match &edit {
            SceneContentEdit::MeshVertices { .. }
            | SceneContentEdit::InsertMeshVertices { .. }
            | SceneContentEdit::DeleteMeshVertices { .. }
            | SceneContentEdit::ReorderMeshVertices { .. }
            | SceneContentEdit::InsertTriangle { .. }
            | SceneContentEdit::PatchTriangle { .. }
            | SceneContentEdit::DeleteTriangles { .. }
            | SceneContentEdit::ReplaceTriangles { .. } => {
                self.plan_mesh_content(&mut plan, edit, control)?
            }
            SceneContentEdit::WarpPoints { .. }
            | SceneContentEdit::WarpBezier { .. }
            | SceneContentEdit::ResizeWarp { .. } => {
                self.plan_warp_content(&mut plan, edit, control)?
            }
            _ => self.plan_effect_content(&mut plan, edit, control)?,
        }
        plan.domain.delta.removed = plan.report.removed_pair_ids.clone();
        // A plan owns only changed resources; no-op preserves redo and revisions.
        plan.domain
            .delta
            .meshes
            .retain(|v| self.get_mesh(&v.id) != Some(v));
        plan.domain
            .delta
            .transforms
            .retain(|v| self.get_transform(&v.id) != Some(v));
        plan.domain
            .delta
            .bindings
            .retain(|v| self.get_binding(&v.id) != Some(v));
        plan.domain
            .delta
            .scene_bindings
            .retain(|v| self.get_scene_binding(&v.id) != Some(v));
        plan.domain
            .delta
            .blends
            .retain(|v| self.get_blend_binding(&v.id) != Some(v));
        plan.domain
            .delta
            .glues
            .retain(|v| self.get_glue(&v.id) != Some(v));
        plan.domain
            .delta
            .offscreens
            .retain(|v| self.get_offscreen(&v.id) != Some(v));
        plan.domain.no_op = !plan.domain.delta.changed(self);
        if !plan.is_noop() {
            self.validate_compound_organization(&plan.domain)?;
        }
        plan.domain.affected.sort();
        plan.domain.affected.dedup();
        control.check()?;
        Ok(plan)
    }
    pub fn apply_scene_content_plan(&mut self, plan: &SceneContentPlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.domain.baseline)
            || self.revision() != plan.domain.revision
        {
            return self.failed(Status::error("STALE_PLAN", "Replan scene content"));
        }
        if plan.is_noop() {
            return self.failed(Status::ok());
        }
        plan.domain.delta.install(self);
        for id in &plan.report.removed_pair_ids {
            self.content.editor_state.objects.remove(id);
        }
        self.changed(
            plan.kind,
            self.mesh_order().to_vec(),
            plan.domain.affected.clone(),
        )
    }
    pub(super) fn content_unlocked(&self, id: &str) -> Result<(), Status> {
        self.authoring_unlocked(id)
    }
}
fn check(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}
fn permutation(order: &[usize], len: usize) -> Result<(), Status> {
    if order.len() != len
        || order.iter().any(|&i| i >= len)
        || order.iter().copied().collect::<HashSet<_>>().len() != len
    {
        Err(Status::error(
            "INVALID_PERMUTATION",
            "Every original slot must occur exactly once",
        ))
    } else {
        Ok(())
    }
}
fn distinct<T: Eq + std::hash::Hash>(ids: &[T]) -> Result<(), Status> {
    if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        Err(Status::error(
            "DUPLICATE_SELECTION",
            "Repeated content address",
        ))
    } else {
        Ok(())
    }
}

impl Document {
    fn content_growth(&self, edit: &SceneContentEdit) -> Result<(usize, usize), Status> {
        use SceneContentEdit as E;
        let shapes = |id: &str| {
            1 + self.binding_for_mesh(id).map_or(0, |b| b.keyforms.len())
                + self.binding_for_scene(id).map_or(0, |b| b.track.len())
                + self
                    .iter_blend_bindings_for_target(id)
                    .map(|b| b.keyforms.len())
                    .sum::<usize>()
        };
        let (work, items, bytes) = match edit {
            E::InsertMeshVertices {
                mesh_id, vertices, ..
            } => (shapes(mesh_id), vertices.len(), std::mem::size_of::<Vec2>()),
            E::ResizeWarp {
                transform_id,
                rows,
                columns,
                ..
            } => {
                if *rows > 64 || *columns > 64 || *rows == 0 || *columns == 0 {
                    return Err(Status::error("INVALID_WARP_DIMENSIONS", transform_id));
                }
                (
                    shapes(transform_id),
                    (*rows as usize + 1) * (*columns as usize + 1),
                    std::mem::size_of::<Vec2>(),
                )
            }
            E::DuplicateGluePairs { ids, .. } => {
                (1, ids.len(), std::mem::size_of::<GlueVertexPair>() + 36)
            }
            E::InsertGluePair { .. } => (1, 1, std::mem::size_of::<GlueVertexPair>() + 36),
            E::DuplicateOffscreenKeyforms { slots, .. } => {
                (1, slots.len(), std::mem::size_of::<OffscreenKeyform>())
            }
            E::InsertOffscreenKeyform { .. } => (1, 1, std::mem::size_of::<OffscreenKeyform>()),
            E::ReplaceTriangles { triangles, .. } => {
                (1, triangles.len(), std::mem::size_of::<[VertexId; 3]>())
            }
            E::WarpBezier {
                controller: Some(c),
                ..
            } => (
                1,
                c.nodes.len(),
                std::mem::size_of::<crate::WarpBezierNode>(),
            ),
            _ => (0, 0, 0),
        };
        // Three arrays for newly inserted vertices cover base positions, UV and
        // IDs in addition to every ordinary/Blend shape's positions.
        let multiplier = if matches!(edit, E::InsertMeshVertices { .. }) {
            work.checked_add(3)
        } else {
            Some(work)
        };
        let growth = multiplier
            .and_then(|n| n.checked_mul(items))
            .and_then(|n| n.checked_mul(bytes))
            .ok_or_else(|| {
                Status::error(
                    "AUTHORING_CAPACITY_OVERFLOW",
                    "Content allocation overflows",
                )
            })?;
        Ok((work, growth))
    }
}
