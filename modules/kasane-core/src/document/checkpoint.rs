use super::{Document, DocumentContent};
use crate::types::{ChangeKind, ImageAsset, Status};
use std::{collections::HashMap, sync::Arc};

mod size;

/// Persistent document content, without revisions, saved state, or derived caches.
/// A checkpoint may be exchanged with a document to implement full-content undo.
#[derive(Debug, Clone)]
pub struct DocumentCheckpoint(DocumentContent);

impl DocumentCheckpoint {
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
        asset.source = source;
        asset.sha256 = sha256;
        true
    }
}

impl Document {
    pub fn estimated_content_bytes(&self) -> usize {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.document_size");
        size::estimated_content_bytes(&self.content)
    }
    pub fn checkpoint(&self) -> DocumentCheckpoint {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.checkpoint_clone");
        DocumentCheckpoint(self.content.clone())
    }

    /// Build an isolated edit candidate. Its saved baseline and evaluation caches
    /// are deliberately absent; only `publish_candidate` can commit it.
    pub fn fork_candidate(&self) -> Document {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.fork_candidate");
        let mut candidate = Document::new();
        let mut content = self.checkpoint();
        candidate.swap_checkpoint(&mut content, true);
        candidate.snapshot_identity = self.snapshot_identity.clone();
        candidate.revision = self.revision;
        candidate.evaluation_revision = self.evaluation_revision;
        candidate
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
        // Preserve geometry caches only when all persistent changes are known
        // display metadata; identity and runtime inputs must still invalidate.
        let metadata_only = if !identity_changed && declared_kind == ChangeKind::Metadata {
            self.same_content_except_visual_metadata(&candidate.content)
        } else {
            false
        };
        let mut candidate = candidate;
        let mut content = DocumentCheckpoint(DocumentContent::default());
        candidate.swap_checkpoint(&mut content, true);
        self.swap_checkpoint(&mut content, !metadata_only);
        self.advance_checkpoint_revision(!metadata_only);
        Ok(true)
    }

    fn same_content_except_visual_metadata(&self, content: &DocumentContent) -> bool {
        let original = &self.content;
        // Compare borrowed data directly: metadata commits must not clone mesh
        // geometry merely to decide whether the prepared frame remains valid.
        original.id == content.id
            && original.assets == content.assets
            && original.asset_order == content.asset_order
            && original.part_order == content.part_order
            && original.transform_order == content.transform_order
            && original.transforms == content.transforms
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
                (Some(a), Some(b)) => a.same_execution(b),
                (None, None) => true,
                _ => false,
            }
            && original.physics == content.physics
            && original.missing_attachments == content.missing_attachments
            && original.model3_settings.layout == content.model3_settings.layout
            && original.model3_settings.user_data == content.model3_settings.user_data
            && original.model3_settings.extensions == content.model3_settings.extensions
            && original.model3_settings.source_runtime_ids
                == content.model3_settings.source_runtime_ids
            && original.model3_settings.source_content == content.model3_settings.source_content
            && original.package_attachments == content.package_attachments
            && original.meshes.len() == content.meshes.len()
            && original.meshes.iter().all(|(id, old)| {
                content.meshes.get(id).is_some_and(|new| {
                    old.id == new.id
                        && old.texture_asset_id == new.texture_asset_id
                        && old.vertex_ids == new.vertex_ids
                        && old.base_positions == new.base_positions
                        && old.uvs == new.uvs
                        && old.triangles == new.triangles
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
        self.swap_checkpoint(checkpoint, !metadata_only);
        self.advance_checkpoint_revision(!metadata_only);
        Ok(())
    }

    /// Swap all persistent content while retaining the saved baseline.
    fn swap_checkpoint(&mut self, checkpoint: &mut DocumentCheckpoint, invalidate: bool) {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.checkpoint_swap_indexes");
        self.modified_cache.take();
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
        std::mem::swap(&mut self.content, &mut checkpoint.0);
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
        if invalidate {
            self.lookup.take();
            self.binding_lookup.take();
            self.pair_lookup.take();
            self.prepared.take();
            self.scene_projection.take();
        }
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
