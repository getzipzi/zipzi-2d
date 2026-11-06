//! Dedicated geometry and scene-content edits. Array positions are topology
//! coordinates, never object identities. Every plan binds the native snapshot.
use super::conversion::{ConversionControl, ConversionLimits, ConversionWork};
use super::organization::{Delta, OrganizationChanges, SceneWorkspace};
use super::*;
use crate::evaluation::EvaluationModel;
use crate::{GlueVertexPair, OffscreenKeyform, Vec2, VertexId, WarpBezier};
use std::collections::{BTreeMap, HashSet};
mod address;
mod budget;
mod deformer_original;
mod effects;
mod mesh;
mod warp;

pub use zipzi_contract::document::{
    NewMeshVertex, OffscreenDeletePolicy, OffscreenMappingPatch, PartKeyformSlot, SceneContentEdit,
    SceneContentReport, ShapeTarget, VertexContentPatch, VertexDeletePolicy, VertexSeed,
    WarpPointPatch,
};

#[derive(Debug, Clone)]
pub struct ContentChanges {
    pub(crate) domain: OrganizationChanges,
    pub(crate) report: SceneContentReport,
    pub(crate) cost: ConversionWork,
}
impl Model<'_> {
    pub fn plan_scene_content_with_control(
        &self,
        edit: SceneContentEdit,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ContentChanges, Status> {
        control.check()?;
        let (work, growth) = self.content_growth(&edit)?;
        let input = self.content_edit_payload(&edit);
        let payload = input.checked_add(growth).ok_or_else(|| {
            Status::error("AUTHORING_CAPACITY_OVERFLOW", "Content payload overflows")
        })?;
        let cost = limits.check_conversion(work, payload)?;
        let mut plan = ContentChanges {
            domain: OrganizationChanges {
                delta: Delta::default(),
                affected: vec![],
                copied: BTreeMap::new(),
                effects: Default::default(),
                no_op: true,
            },
            report: Default::default(),
            cost,
        };
        match &edit {
            SceneContentEdit::DeformerOriginal {
                transform_id,
                target,
                action,
                context,
            } => {
                self.plan_deformer_original(
                    &mut plan,
                    transform_id,
                    target,
                    *action,
                    context,
                    limits,
                    control,
                )?;
            }
            SceneContentEdit::WarpKeepChildren { transform_id, warp } => {
                let (delta, affected, error, samples) = self.deformer_keep_children_delta(
                    transform_id,
                    &crate::TransformData::Warp(warp.clone()),
                    control,
                )?;
                plan.domain.delta = delta;
                plan.domain.affected = affected;
                plan.report.preservation_error = Some(error);
                plan.report.preservation_samples = samples;
            }
            SceneContentEdit::RotationKeepChildren {
                transform_id,
                rotation,
            } => {
                let (delta, affected, error, samples) = self.deformer_keep_children_delta(
                    transform_id,
                    &crate::TransformData::Rotation(rotation.clone()),
                    control,
                )?;
                plan.domain.delta = delta;
                plan.domain.affected = affected;
                plan.report.preservation_error = Some(error);
                plan.report.preservation_samples = samples;
            }
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
            | SceneContentEdit::WarpBezierControls { .. }
            | SceneContentEdit::WarpAuthoring { .. }
            | SceneContentEdit::WarpShape { .. }
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
        plan.domain.affected.sort();
        plan.domain.affected.dedup();
        control.check()?;
        Ok(plan)
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

impl Model<'_> {
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
            E::DeformerOriginal { .. } => return Ok((1, 0)),
            E::WarpKeepChildren { .. } | E::RotationKeepChildren { .. } => {
                // Conversion fragments and native verification are bounded by the
                // retained document; reject capacity before cloning any candidate.
                return Ok((self.mesh_order().len() + self.transform_order().len(), 0));
            }
            E::WarpAuthoring {
                transform_id,
                settings,
            } => {
                if let Some(grid) = settings.bezier_grid.filter(|grid| {
                    self.get_transform(transform_id)
                        .and_then(|t| t.warp_authoring.as_ref())
                        .and_then(|s| s.bezier_grid)
                        != Some(*grid)
                }) {
                    check(grid.validate())?;
                    (
                        shapes(transform_id),
                        (grid.rows as usize + 1) * (grid.columns as usize + 1),
                        std::mem::size_of::<crate::WarpBezierNode>(),
                    )
                } else {
                    (1, 1, std::mem::size_of::<crate::WarpAuthoringSettings>())
                }
            }
            E::WarpShape { transform_id, .. } | E::WarpBezierControls { transform_id, .. } => {
                let warp = self
                    .get_transform(transform_id)
                    .and_then(|t| t.warp())
                    .ok_or_else(|| Status::error("NOT_WARP", transform_id))?;
                (
                    1,
                    ((warp.rows as usize + 1) * (warp.columns as usize + 1)).max(33 * 33),
                    std::mem::size_of::<crate::WarpBezierNode>(),
                )
            }
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
