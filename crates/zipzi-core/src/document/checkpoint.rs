use super::{Document, DocumentContent};
use crate::types::{ChangeKind, ImageAsset, Status};
use std::{collections::HashMap, sync::Arc};

mod size;

/// Persistent document content, without revisions, saved state, or derived caches.
/// A checkpoint may be exchanged with a document to implement full-content undo.
// The proof travels with this immutable content through undo/redo. Any direct
// checkpoint mutation must discard it before the checkpoint can be restored.
#[derive(Debug, Clone)]
pub struct DocumentCheckpoint(DocumentContent, bool);

impl DocumentCheckpoint {
    /// Inline content and copied allocations, excluding shared record payloads.
    pub fn estimated_content_metadata_bytes(&self) -> usize {
        size::estimated_content_metadata_bytes(&self.0)
    }
    /// Estimated heap and inline bytes owned by persistent content. This is a
    /// capacity-aware budget estimate, not a process RSS measurement.
    pub fn estimated_bytes(&self) -> usize {
        size::estimated_content_bytes(&self.0)
    }

    /// Read-only resource descriptions for SDK history relocation.
    pub fn assets(&self) -> Vec<ImageAsset> {
        self.0
            .asset_order
            .iter()
            .filter_map(|id| self.0.assets.get(id).cloned())
            .collect()
    }

    /// Rewrite only a checkpoint asset's storage location and verified hash.
    /// The complete expected descriptor prevents an ID-only relocation.
    pub fn relocate_asset_storage(
        &mut self,
        expected: &ImageAsset,
        source: String,
        sha256: String,
    ) -> bool {
        let Some(asset) = self.0.assets.get_mut(&expected.id) else {
            return false;
        };
        if asset != expected {
            return false;
        }
        let Some(content) = &mut asset.content else {
            return false;
        };
        content.source = source;
        content.sha256 = sha256;
        self.1 = false;
        true
    }
}

impl Document {
    /// Inline content and copied allocations, excluding shared record payloads.
    /// Forks/checkpoints share those payloads, so their exact size differs only
    /// by this metadata and can reuse the source's full content estimate.
    pub fn estimated_content_metadata_bytes(&self) -> usize {
        size::estimated_content_metadata_bytes(&self.content)
    }

    /// Allocations owned by editor preferences, for exact metadata size deltas.
    pub fn estimated_editor_state_bytes(&self) -> usize {
        size::estimated_editor_state_bytes(self.editor_state())
    }
    pub fn estimated_content_bytes(&self) -> usize {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.document_size");
        size::estimated_content_bytes(&self.content)
    }
    /// Persistent allocations copied by `fork_candidate`, excluding shared
    /// record payloads. Includes mutable lookup descriptors; saved state and
    /// derived evaluation caches are omitted.
    pub fn estimated_snapshot_bytes(&self) -> usize {
        size::estimated_snapshot_bytes(self)
    }
    /// Records a complete mesh topology replacement may detach or rebuild.
    pub fn estimated_mesh_dependency_bytes(&self, mesh_id: &str) -> usize {
        size::estimated_mesh_dependency_bytes(self, mesh_id)
    }
    /// Temporary identity inventory rebuilt during dependency validation.
    pub fn estimated_authoring_index_bytes(&self) -> usize {
        size::estimated_authoring_index_bytes(self)
    }
    pub(super) fn estimated_deletion_write_bytes(&self, report: &super::DeletionReport) -> usize {
        size::estimated_deletion_write_bytes(self, report)
    }
    pub fn checkpoint(&self) -> DocumentCheckpoint {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.checkpoint_clone");
        DocumentCheckpoint(self.content.clone(), self.structure_is_validated())
    }

    /// Build an isolated edit candidate. Its saved baseline and evaluation caches
    /// are deliberately absent; only `publish_candidate` can commit it.
    pub fn fork_candidate(&self) -> Document {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.fork_candidate");
        // A fork keeps the same content, so immutable per-mesh vertex indexes
        // remain valid and shared. Checkpoint restoration still rebuilds them.
        Document {
            content: self.content.clone(),
            mesh_runtime_ids: self.mesh_runtime_ids.clone(),
            vertex_slots: self.vertex_slots.clone(),
            snapshot_identity: self.snapshot_identity.clone(),
            revision: self.revision,
            evaluation_revision: self.evaluation_revision,
            original_capture_context: self.original_capture_context.clone(),
            ..Document::new()
        }
    }

    /// Publish a candidate as one revision while retaining this document's saved
    /// baseline. Callers must keep candidate mutation isolated until this point.
    pub fn publish_candidate(
        &mut self,
        candidate: Document,
        declared_kind: ChangeKind,
        identity_changed: bool,
    ) -> Result<bool, Status> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.publish_candidate");
        if self.transaction_active || candidate.transaction_active || candidate.batch_build_active {
            return Err(Status::error("EDIT_ACTIVE", "Finish the active edit first"));
        }
        if self.content.id != candidate.content.id {
            return Err(Status::error(
                "DIFFERENT_DOCUMENT",
                "Candidate document ID differs",
            ));
        }
        if !identity_changed && self.same_content(&candidate) {
            return Ok(false);
        }
        // Structural authoring metadata (for example meshing constraints) can also leave
        // every evaluation input unchanged. Compare fields rather than trusting the category.
        let metadata_only = if !identity_changed
            && matches!(declared_kind, ChangeKind::Metadata | ChangeKind::Structure)
        {
            candidate.has_only_editor_state_changes_since(self)
                || self.same_content_except_visual_metadata(&candidate.content)
        } else {
            false
        };
        let saved_match = candidate
            .has_only_editor_state_changes_since(self)
            .then(|| self.saved_non_editor_state_matches.get().copied())
            .flatten();
        let validated = candidate.structure_is_validated();
        let mut content = DocumentCheckpoint(candidate.content, validated);
        let validated = self.swap_checkpoint_content(&mut content, !metadata_only);
        if let Some(matches) = saved_match {
            let _ = self.saved_non_editor_state_matches.set(matches);
        }
        // Candidate authoring maintains these indexes together with its content.
        // Move them on publication instead of rebuilding every mesh's vertex map.
        self.mesh_runtime_ids = candidate.mesh_runtime_ids;
        self.vertex_slots = candidate.vertex_slots;
        self.advance_checkpoint_revision(!metadata_only);
        if validated {
            self.remember_valid_structure();
        }
        Ok(true)
    }

    /// Whether persistent runtime inputs are equal, excluding authoring-only metadata such
    /// as labels and modeling constraints. Does not evaluate poses or compare rendered pixels.
    pub fn same_evaluation_inputs(&self, other: &Document) -> bool {
        self.same_content_except_visual_metadata(&other.content)
    }

    fn same_content_except_visual_metadata(&self, content: &DocumentContent) -> bool {
        let original = &self.content;
        // Compare borrowed data directly: metadata commits must not clone mesh
        // geometry merely to decide whether the prepared frame remains valid.
        original.id == content.id
            && original
                .assets
                .values()
                .filter(|a| a.content.is_some())
                .count()
                == content
                    .assets
                    .values()
                    .filter(|a| a.content.is_some())
                    .count()
            && original
                .assets
                .iter()
                .filter(|(_, a)| a.content.is_some())
                .all(|(id, a)| {
                    content
                        .assets
                        .get(id)
                        .is_some_and(|b| a.content == b.content)
                })
            && original.part_order == content.part_order
            && original.transform_order == content.transform_order
            && original.transforms.len() == content.transforms.len()
            && original.transforms.iter().all(|(id, a)| {
                content.transforms.get(id).is_some_and(|b| {
                    (
                        &a.id,
                        &a.runtime_id,
                        &a.name,
                        &a.part_id,
                        &a.parent_id,
                        &a.data,
                        a.enabled,
                        a.appearance,
                        &a.warp_authoring,
                    ) == (
                        &b.id,
                        &b.runtime_id,
                        &b.name,
                        &b.part_id,
                        &b.parent_id,
                        &b.data,
                        b.enabled,
                        b.appearance,
                        &b.warp_authoring,
                    )
                })
            })
            && original.mesh_order == content.mesh_order
            && original.parameter_order == content.parameter_order
            && original.bindings == content.bindings
            && original.binding_order == content.binding_order
            && original.scene_bindings == content.scene_bindings
            && original.scene_binding_order == content.scene_binding_order
            && original.draw_order_groups == content.draw_order_groups
            && original.blend_key_tables == content.blend_key_tables
            && original.blend_key_table_order == content.blend_key_table_order
            && original.blend_constraints == content.blend_constraints
            && original.blend_constraint_order == content.blend_constraint_order
            && original.blend_bindings == content.blend_bindings
            && original.blend_binding_order == content.blend_binding_order
            && original.glues == content.glues
            && original.glue_order == content.glue_order
            && original.offscreens == content.offscreens
            && original.offscreen_order == content.offscreen_order
            && original.expressions.len() == content.expressions.len()
            && original.expressions.iter().all(|(id, old)| {
                content.expressions.get(id).is_some_and(|new| {
                    old.fade_in == new.fade_in
                        && old.fade_out == new.fade_out
                        && old.entries == new.entries
                        && old.extensions == new.extensions
                })
            })
            && original.motions.len() == content.motions.len()
            && original.motions.iter().all(|(id, a)| {
                content.motions.get(id).is_some_and(|b| {
                    (
                        a.duration,
                        a.fps,
                        a.looping,
                        a.restricted_beziers,
                        a.fade_in,
                        a.fade_out,
                        &a.tracks,
                        &a.events,
                        &a.extensions,
                        &a.meta_extensions,
                    ) == (
                        b.duration,
                        b.fps,
                        b.looping,
                        b.restricted_beziers,
                        b.fade_in,
                        b.fade_out,
                        &b.tracks,
                        &b.events,
                        &b.extensions,
                        &b.meta_extensions,
                    )
                })
            })
            && {
                let registrations = |c: &DocumentContent| {
                    c.motion_groups
                        .iter()
                        .flat_map(|g| &g.entries)
                        .chain(&c.motion_roots)
                        .map(|e| (e.id.clone(), (e.clip_id.clone(), e.fade_in, e.fade_out)))
                        .collect::<std::collections::BTreeMap<_, _>>()
                };
                registrations(original) == registrations(content)
            }
            && match (original.pose.as_deref(), content.pose.as_deref()) {
                (Some(a), Some(b)) => super::pose::same_execution(a, b),
                (None, None) => true,
                _ => false,
            }
            && original.physics == content.physics
            && original.meshes.len() == content.meshes.len()
            && original.meshes.iter().all(|(id, old)| {
                content.meshes.get(id).is_some_and(|new| {
                    old.id == new.id
                        && old.texture_asset_id == new.texture_asset_id
                        && old.vertex_ids == new.vertex_ids
                        && old.base_positions == new.base_positions
                        && old.uvs == new.uvs
                        && old.triangles == new.triangles
                        // meshing describes authoring intent; triangles already capture
                        // every connection used by evaluation and export.
                        && old.runtime_id == new.runtime_id
                        && old.part_id == new.part_id
                        && old.deformer_id == new.deformer_id
                        && old.appearance == new.appearance
                        && old.draw_order == new.draw_order
                        && old.blend_mode == new.blend_mode
                        && old.enabled == new.enabled
                        && old.double_sided == new.double_sided
                        && old.inverted_mask == new.inverted_mask
                        && old.masks == new.masks
                        && old.raw_blend_mode == new.raw_blend_mode
                })
            })
            && original.parameters.len() == content.parameters.len()
            && original.parameters.iter().all(|(id, old)| {
                content.parameters.get(id).is_some_and(|new| {
                    old.id == new.id
                        && old.runtime_id == new.runtime_id
                        && old.minimum == new.minimum
                        && old.maximum == new.maximum
                        && old.default_value == new.default_value
                        && old.decimal_places == new.decimal_places
                        && old.kind == new.kind
                        && old.repeat == new.repeat
                })
            })
            && original.parts.len() == content.parts.len()
            && original.parts.iter().all(|(id, old)| {
                content.parts.get(id).is_some_and(|new| {
                    old.id == new.id
                        && old.runtime_id == new.runtime_id
                        && old.parent_id == new.parent_id
                        && old.enabled == new.enabled
                        && old.draw_order == new.draw_order
                })
            })
    }

    /// Exchange persistent content with a checkpoint from this document, then
    /// advance its revision. A rejected exchange changes neither side.
    pub fn exchange_checkpoint(
        &mut self,
        checkpoint: &mut DocumentCheckpoint,
    ) -> Result<(), Status> {
        if self.transaction_active || self.batch_build_active {
            return Err(Status::error("EDIT_ACTIVE", "Finish the active edit first"));
        }
        if self.content.id != checkpoint.0.id {
            return Err(Status::error(
                "DIFFERENT_DOCUMENT",
                "Checkpoint document ID differs",
            ));
        }
        let metadata_only = self.same_content_except_visual_metadata(&checkpoint.0);
        let validated = self.swap_checkpoint(checkpoint, !metadata_only);
        self.advance_checkpoint_revision(!metadata_only);
        if validated {
            self.remember_valid_structure();
        }
        Ok(())
    }

    /// Swap all persistent content while retaining the saved baseline.
    fn swap_checkpoint(&mut self, checkpoint: &mut DocumentCheckpoint, invalidate: bool) -> bool {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.checkpoint_swap_indexes");
        let validated = self.swap_checkpoint_content(checkpoint, invalidate);
        self.mesh_runtime_ids = self
            .content
            .meshes
            .iter()
            .map(|(id, mesh)| (mesh.runtime_id.clone(), id.clone()))
            .collect();
        self.vertex_slots = self
            .content
            .meshes
            .iter()
            .map(|(id, mesh)| {
                let slots: HashMap<_, _> = mesh
                    .vertex_ids
                    .iter()
                    .enumerate()
                    .map(|(slot, vertex)| (*vertex, slot))
                    .collect();
                (id.clone(), slots)
            })
            .collect();
        validated
    }

    fn swap_checkpoint_content(
        &mut self,
        checkpoint: &mut DocumentCheckpoint,
        invalidate: bool,
    ) -> bool {
        self.tracking = Default::default();
        self.modified_cache.take();
        self.saved_non_editor_state_matches.take();
        self.expression_entries.take();
        self.motion_lookup.take();
        if invalidate
            || !super::organization::same_metadata_shape(
                &self.content.display_info,
                &checkpoint.0.display_info,
            )
            || self.content.hierarchy_order.organization
                != checkpoint.0.hierarchy_order.organization
        {
            self.organization_index.take();
        }
        self.reference_index.take();
        self.reference_projection.take();
        let validated = checkpoint.1;
        checkpoint.1 = self.structure_is_validated();
        self.forget_valid_structure();
        std::mem::swap(&mut self.content, &mut checkpoint.0);
        if invalidate {
            self.lookup.take();
            self.binding_lookup.take();
            self.pair_lookup.take();
            self.prepared.take();
            self.scene_projection.take();
        }
        validated
    }

    fn advance_checkpoint_revision(&mut self, evaluation_changed: bool) {
        self.reference_index.take();
        self.reference_projection.take();
        self.snapshot_identity = Arc::new(());
        self.revision += 1;
        if evaluation_changed {
            self.evaluation_revision = self.revision;
            self.lookup.take();
            self.binding_lookup.take();
            self.pair_lookup.take();
            self.prepared.take();
            self.scene_projection.take();
        }
    }
}

pub(super) fn expression_entries_bytes(entries: &Vec<super::ExpressionEntry>) -> usize {
    size::expression_entries_bytes(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Mesh, ModelSetup, Vec2};

    #[test]
    fn publication_moves_candidate_indexes_and_checkpoint_exchange_restores_topology() {
        let id = |n| format!("00000000-0000-4000-8000-{n:012x}");
        let mut document = Document::new();
        assert!(document.initialize(id(1), ModelSetup::default()).is_ok());
        for n in [2, 3] {
            assert!(document
                .create_mesh(Mesh {
                    id: id(n),
                    vertex_ids: vec![1, 2, 3],
                    base_positions: vec![Vec2::default(); 3],
                    uvs: vec![Vec2::default(); 3],
                    triangles: vec![[1, 2, 3]],
                    ..Default::default()
                })
                .status
                .is_ok());
        }
        let original = document.fork_candidate();
        let mut undo = document.checkpoint();
        let mut candidate = document.fork_candidate();
        let mut mesh = candidate.get_mesh(&id(2)).unwrap().clone();
        mesh.vertex_ids = vec![30, 20, 10];
        mesh.triangles = vec![[30, 20, 10]];
        assert!(candidate.replace_mesh(mesh).status.is_ok());
        assert!(candidate.validate_structure().is_empty());
        let indexes = candidate.vertex_slots.clone();
        let revision = document.revision();
        assert!(document
            .publish_candidate(candidate, ChangeKind::Structure, true)
            .unwrap());
        assert!(document.revision() > revision);
        assert!(document.structure_is_validated());
        for n in [2, 3] {
            assert!(std::ptr::eq(
                document.vertex_slots.get(&id(n)).unwrap(),
                indexes.get(&id(n)).unwrap(),
            ));
        }
        assert!(std::ptr::eq(
            document.vertex_slots.get(&id(3)).unwrap(),
            original.vertex_slots.get(&id(3)).unwrap(),
        ));
        assert_eq!(document.vertex_slot(&id(2), 30), Some(0));
        assert_eq!(document.vertex_slot(&id(2), 1), None);
        document.exchange_checkpoint(&mut undo).unwrap();
        assert_eq!(document.vertex_slot(&id(2), 1), Some(0));
        assert_eq!(document.vertex_slot(&id(2), 30), None);
        document.exchange_checkpoint(&mut undo).unwrap();
        assert_eq!(document.vertex_slot(&id(2), 30), Some(0));
        assert!(document.validate_structure().is_empty());
    }
}
