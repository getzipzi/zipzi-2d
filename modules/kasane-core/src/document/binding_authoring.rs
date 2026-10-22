//! Conversion plans own only their write fragments. They never expose an invalid
//! intermediate Document, and cannot be applied to another document snapshot.
use super::*;
use crate::{BindingAxis, ParameterKind, Vec2};
use serde::{Deserialize, Serialize};

pub(super) mod identity;
pub use identity::{BindingIdentity, BindingIdentityKind};
mod report;
pub use report::{
    ConversionAddress, ConversionDomain, ConversionElement, ConversionLocation,
    ConversionOwnerDomain, ConversionReconstruction, ConversionRemapping, ConversionReport,
    ParameterRangeConversion,
};
mod control;
pub use control::ConversionControl;
mod projection;
pub use projection::BindingOmission;
mod blend;
mod deltas;
pub use blend::*;
mod lattice;
pub use lattice::GridOperation;
mod ordinary;
pub use ordinary::*;
mod grid;
mod range;
mod reparent;
pub use grid::*;
pub use range::*;
pub use reparent::*;

/// Finite, caller-reducible limits checked before constructing a Cartesian grid.
/// Byte counts are conservative payload estimates, not process RSS measurements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoringLimits {
    pub max_sample_combinations: usize,
    pub max_conversion_bytes: usize,
    pub max_history_bytes: usize,
}
impl Default for AuthoringLimits {
    fn default() -> Self {
        Self {
            max_sample_combinations: 65_536,
            max_conversion_bytes: 64 * 1024 * 1024,
            max_history_bytes: 256 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConversionCost {
    pub sample_combinations: usize,
    pub payload_bytes: usize,
    pub temporary_bytes: usize,
    /// Includes the SDK's complete previous checkpoint and new candidate.
    pub history_bytes: usize,
}
impl AuthoringLimits {
    pub fn check_grid(
        self,
        dimensions: &[usize],
        bytes_per_form: usize,
        retained_bytes: usize,
    ) -> Result<ConversionCost, Status> {
        let overflow = || {
            Status::error(
                "AUTHORING_CAPACITY_OVERFLOW",
                "Grid size or byte count overflows",
            )
        };
        let combinations = dimensions.iter().try_fold(1usize, |n, &d| {
            if d == 0 {
                return Err(Status::error("INVALID_KEYS", "Empty grid dimension"));
            }
            n.checked_mul(d).ok_or_else(overflow)
        })?;
        let payload = combinations
            .checked_mul(bytes_per_form)
            .ok_or_else(overflow)?;
        self.check_conversion(combinations, payload, retained_bytes)
    }
    pub fn check_conversion(
        self,
        combinations: usize,
        payload: usize,
        retained_bytes: usize,
    ) -> Result<ConversionCost, Status> {
        let overflow = || {
            Status::error(
                "AUTHORING_CAPACITY_OVERFLOW",
                "Conversion byte count overflows",
            )
        };
        // Real-model allocation probes include candidate/checkpoint copies,
        // identity sets, ChangeSet/context summaries and plan inventories. Six
        // retained payloads leave headroom above the observed 3.3x commit peak;
        // this remains a capacity estimate rather than an allocator/RSS limit.
        let temporary = retained_bytes
            .checked_mul(6)
            .and_then(|n| {
                payload
                    .checked_mul(2)
                    .and_then(|payload| n.checked_add(payload))
            })
            .ok_or_else(overflow)?;
        let history = retained_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_add(payload))
            .ok_or_else(overflow)?;
        let cost = ConversionCost {
            sample_combinations: combinations,
            payload_bytes: payload,
            temporary_bytes: temporary,
            history_bytes: history,
        };
        if combinations > self.max_sample_combinations {
            return Err(Status::error(
                "AUTHORING_GRID_LIMIT",
                format!(
                    "{combinations} combinations exceed {}",
                    self.max_sample_combinations
                ),
            ));
        }
        if temporary > self.max_conversion_bytes {
            return Err(Status::error(
                "AUTHORING_MEMORY_LIMIT",
                format!(
                    "{temporary} temporary bytes exceed {}",
                    self.max_conversion_bytes
                ),
            ));
        }
        if history > self.max_history_bytes {
            return Err(Status::error(
                "AUTHORING_HISTORY_LIMIT",
                format!("{history} history bytes exceed {}", self.max_history_bytes),
            ));
        }
        Ok(cost)
    }
}

pub(super) fn require_ok(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}
pub(super) fn check_keys(parameter: &Parameter, keys: &[f32]) -> Result<(), Status> {
    if keys.is_empty()
        || keys.iter().enumerate().any(|(i, &k)| {
            !k.is_finite()
                || k < parameter.minimum
                || k > parameter.maximum
                || (i > 0 && k <= keys[i - 1])
        })
    {
        Err(Status::error("INVALID_KEYS", &parameter.id))
    } else {
        Ok(())
    }
}
impl Document {
    /// Locks inherit through organizational owners. Deformation parents protect
    /// their own fields; reparent/conversion operations check affected parents explicitly.
    pub(super) fn authoring_unlocked(&self, id: &str) -> Result<(), Status> {
        if self.editor_state().is_empty() {
            return Ok(());
        }
        let mut pending = vec![id];
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = pending.pop() {
            if id.is_empty() || !seen.insert(id) {
                continue;
            }
            if self.editor_state().get(id).locked {
                return Err(Status::error("OBJECT_LOCKED", id));
            }
            // Whole-owner replacements and shared conversions must honor locks
            // on their owned axes and samples as well as the resource itself.
            let axes = self
                .get_binding(id)
                .map(|b| b.axes.as_slice())
                .or_else(|| self.get_scene_binding(id).map(|b| b.axes.as_slice()))
                .or_else(|| {
                    self.get_glue(id)
                        .and_then(|g| g.binding.as_ref())
                        .map(|b| b.axes.as_slice())
                });
            if let Some(axes) = axes {
                for axis in axes {
                    for child in std::iter::once(axis.id.as_str())
                        .chain(axis.samples.iter().map(|s| s.id.as_str()))
                    {
                        if self.editor_state().get(child).locked {
                            return Err(Status::error("OBJECT_LOCKED", child));
                        }
                    }
                }
            }
            let samples = self
                .get_blend_key_table(id)
                .map(|t| t.samples.iter().map(|s| s.id.as_str()).collect::<Vec<_>>())
                .or_else(|| {
                    self.get_blend_constraint(id)
                        .map(|c| c.samples.iter().map(|s| s.id.as_str()).collect())
                });
            for child in samples.into_iter().flatten() {
                if self.editor_state().get(child).locked {
                    return Err(Status::error("OBJECT_LOCKED", child));
                }
            }
            if let Some(child) = self.binding_identity(id) {
                return self.authoring_unlocked(&child.owner);
            } else if let Some(m) = self.get_mesh(id) {
                pending.push(m.part_id.as_str());
            } else if let Some(t) = self.get_transform(id) {
                pending.push(t.part());
            } else if self.get_parameter(id).is_some() {
                if let Some(group) = self.parameter_group_for(id) {
                    pending.push(group);
                }
            } else if let Some(group) = self.get_parameter_group(id) {
                if let Some(parent) = group.parent_id.as_deref() {
                    pending.push(parent);
                }
            } else if let Some(p) = self.get_part(id) {
                pending.push(&p.parent_id);
            } else if let Some(o) = self.get_offscreen(id) {
                pending.push(&o.part_id);
            } else if let Some(b) = self.get_binding(id) {
                pending.push(&b.mesh_id);
            } else if let Some(b) = self.get_scene_binding(id) {
                pending.push(b.target_id());
            } else if let Some(b) = self.get_blend_binding(id) {
                pending.push(&b.target_id);
            } else if self.get_blend_key_table(id).is_some() {
                pending.extend(
                    self.content
                        .blend_bindings
                        .values()
                        .filter(|b| b.key_table_id == id)
                        .map(|b| b.id.as_str()),
                );
            } else if self.get_blend_constraint(id).is_some() {
                pending.extend(
                    self.content
                        .blend_bindings
                        .values()
                        .filter(|b| b.constraint_ids.iter().any(|c| c == id))
                        .map(|b| b.id.as_str()),
                );
            }
        }
        Ok(())
    }
}
