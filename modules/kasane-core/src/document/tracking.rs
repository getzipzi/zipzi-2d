//! Transaction-local coverage of persistent writes. This is independent of
//! validation, identity lifetimes and the SDK's publication categories.
use super::*;
use std::collections::BTreeSet;

/// A record installed by an audited authoring entry point. Currently only
/// existing Mesh scalar fields and identity-preserving MeshBinding payloads
/// can retain coverage.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackedRecord {
    Mesh(String),
    MeshBinding(String),
}

#[derive(Debug, Clone)]
struct Complete {
    baseline: Arc<()>,
    current: Arc<()>,
    touched: BTreeSet<TrackedRecord>,
}

#[derive(Debug, Default)]
pub(super) struct TrackingState(Option<Complete>);

// Ordinary document/planner clones can write raw content before calling changed.
// Only fork_tracked_candidate may establish coverage for a new transaction.
impl Clone for TrackingState {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl TrackingState {
    pub(super) fn for_rollback(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Captured before an audited installation, never reconstructed from ChangeKind.
pub(super) struct TrackedWrite {
    snapshot: Arc<()>,
    records: Vec<TrackedRecord>,
}

impl Document {
    /// Start coverage at this exact snapshot. Ordinary forks and clones remain
    /// untracked. The result still requires normal structure validation.
    pub fn fork_tracked_candidate(&self) -> Document {
        let mut candidate = self.fork_candidate();
        candidate.tracking = TrackingState(Some(Complete {
            baseline: self.snapshot_identity.clone(),
            current: candidate.snapshot_identity.clone(),
            touched: BTreeSet::new(),
        }));
        candidate
    }

    /// Complete write coverage since an exact baseline, or None after any
    /// unsupported write, clone, restore or mismatched snapshot. This proves
    /// neither validity nor net change; callers must compare final records.
    pub fn tracked_records_since(&self, baseline: &Document) -> Option<&BTreeSet<TrackedRecord>> {
        let complete = self.tracking.0.as_ref()?;
        if !Arc::ptr_eq(&complete.baseline, &baseline.snapshot_identity)
            || !Arc::ptr_eq(&complete.current, &self.snapshot_identity)
        {
            return None;
        }
        // Recheck the supported field boundary against the transaction baseline,
        // rather than interpreting coverage as an unrestricted Mesh replacement.
        for record in &complete.touched {
            #[cfg(feature = "bench-instrumentation")]
            crate::benchmark::count("core.tracking.record_reads", 2);
            match record {
                TrackedRecord::Mesh(id) => {
                    let (Some(before), Some(after)) = (baseline.get_mesh(id), self.get_mesh(id))
                    else {
                        return None;
                    };
                    if !mesh_update_is_tracked(before, after) {
                        return None;
                    }
                }
                TrackedRecord::MeshBinding(id) => {
                    #[cfg(feature = "bench-instrumentation")]
                    crate::benchmark::count("core.tracking.binding_record_reads", 2);
                    let (Some(before), Some(after)) =
                        (baseline.get_binding(id), self.get_binding(id))
                    else {
                        return None;
                    };
                    if !binding_payload_is_tracked(before, after) {
                        return None;
                    }
                }
            }
        }
        Some(&complete.touched)
    }

    pub(super) fn tracked_authoring_identities_since(&self, baseline: &Document) -> bool {
        self.tracked_records_since(baseline).is_some_and(|records| {
            // Coverage alone is not a lifecycle proof. Only these audited
            // record kinds and field boundaries preserve every authoring key.
            records.iter().all(|record| match record {
                TrackedRecord::Mesh(_) | TrackedRecord::MeshBinding(_) => true,
            })
        })
    }

    /// Validation has a narrower allowlist than publication. Reject Binding
    /// records before reading their payloads, then recheck exact endpoints and
    /// every Mesh field boundary through the normal coverage query.
    pub(super) fn tracked_mesh_validation_records_since(
        &self,
        baseline: &Document,
    ) -> Option<&BTreeSet<TrackedRecord>> {
        let complete = self.tracking.0.as_ref()?;
        if !complete.touched.iter().all(|record| match record {
            TrackedRecord::Mesh(_) => true,
            TrackedRecord::MeshBinding(_) => false,
        }) {
            return None;
        }
        self.tracked_records_since(baseline)
    }

    pub(super) fn track_mesh_write(&self, ids: Vec<String>) -> TrackedWrite {
        self.track_records_write(ids.into_iter().map(TrackedRecord::Mesh).collect())
    }

    pub(super) fn track_binding_payload_write(&self, id: &str) -> TrackedWrite {
        self.track_records_write(vec![TrackedRecord::MeshBinding(id.into())])
    }

    fn track_records_write(&self, records: Vec<TrackedRecord>) -> TrackedWrite {
        TrackedWrite {
            snapshot: self.snapshot_identity.clone(),
            records,
        }
    }

    pub(super) fn changed_tracked(
        &mut self,
        write: TrackedWrite,
        kind: ChangeKind,
        mesh_ids: Vec<String>,
        object_ids: Vec<String>,
    ) -> EditResult {
        let mut tracking = std::mem::take(&mut self.tracking);
        let matches_snapshot = Arc::ptr_eq(&write.snapshot, &self.snapshot_identity);
        let flags = self.content.editor_state.objects.len();
        let result = self.changed(kind, mesh_ids, object_ids);
        if matches_snapshot && flags == self.content.editor_state.objects.len() {
            if let Some(complete) = &mut tracking.0 {
                if Arc::ptr_eq(&complete.current, &write.snapshot) {
                    complete.current = self.snapshot_identity.clone();
                    complete.touched.extend(write.records);
                    self.tracking = tracking;
                }
            }
        }
        // changed() may also remove stale editor-state entries. Such a write
        // exceeds record coverage and must leave the whole transaction Unknown.
        result
    }
}

pub(super) fn binding_payload_is_tracked(a: &MeshBinding, b: &MeshBinding) -> bool {
    // Compare the installed canonical records, including derived coordinates.
    // Changes to axes, sample identities, ownership or grid shape stay unknown.
    let MeshBinding {
        id,
        mesh_id,
        axes,
        keyforms,
    } = a;
    id == &b.id
        && mesh_id == &b.mesh_id
        && axes == &b.axes
        && keyforms.len() == b.keyforms.len()
        && keyforms.iter().zip(&b.keyforms).all(|(a, b)| {
            let crate::MeshKeyform {
                keys,
                samples,
                positions: _,
                appearance: _,
                draw_order: _,
            } = a;
            keys == &b.keys && samples == &b.samples
        })
}

pub(super) fn mesh_update_is_tracked(a: &Mesh, b: &Mesh) -> bool {
    // Exhaustive destructuring makes newly added persistent fields require an
    // explicit coverage decision. Never clone geometry merely to mask fields.
    let Mesh {
        id,
        name: _,
        texture_asset_id,
        vertex_ids,
        base_positions,
        uvs,
        triangles,
        runtime_id,
        part_id,
        deformer_id,
        appearance: _,
        draw_order,
        blend_mode,
        enabled: _,
        double_sided,
        inverted_mask,
        masks,
        raw_blend_mode,
    } = a;
    id == &b.id
        && texture_asset_id == &b.texture_asset_id
        && vertex_ids == &b.vertex_ids
        && base_positions == &b.base_positions
        && uvs == &b.uvs
        && triangles == &b.triangles
        && runtime_id == &b.runtime_id
        && part_id == &b.part_id
        && deformer_id == &b.deformer_id
        && draw_order == &b.draw_order
        && blend_mode == &b.blend_mode
        && double_sided == &b.double_sided
        && inverted_mask == &b.inverted_mask
        && masks == &b.masks
        && raw_blend_mode == &b.raw_blend_mode
}

#[cfg(test)]
mod tests;
