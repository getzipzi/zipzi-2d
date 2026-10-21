//! Receipts drive the same projection updates for commands and history.
use super::*;
use kasane_sdk::{ChangeCategory, ObjectKind as SdkKind};

impl Document {
    pub(in crate::document) fn invalidate_expression_projection(&mut self) {
        self.invalidate_projection(false);
        self.animation_catalog.get_mut().unwrap().take();
    }

    pub(in crate::document) fn refresh_expression_index(
        &self,
        previous_revision: u64,
        owners: &[String],
    ) {
        let mut cache = self.object_index.lock().unwrap();
        if let Some((revision, index)) = cache.as_mut() {
            if *revision == previous_revision
                && owners
                    .iter()
                    .all(|id| Arc::make_mut(index).refresh_expression(self.model(), id))
            {
                *revision = self.projection_revision;
            }
        }
    }

    pub(in crate::document) fn consume_authoring_receipt(
        &mut self,
        receipt: &EditReceipt,
        fallback_topology: bool,
    ) {
        if !receipt.changed {
            return;
        }
        let changes = &receipt.changes;
        let expression_only = !changes.full_invalidation
            && !changes.categories.contains(&ChangeCategory::Topology)
            && (!changes.created.is_empty()
                || !changes.deleted.is_empty()
                || !changes.modified.is_empty()
                || !changes.collections.is_empty()
                || !changes.references.is_empty())
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .all(|entity| {
                    matches!(entity.kind, SdkKind::Expression | SdkKind::ExpressionEntry)
                });
        if expression_only
            && changes.created.is_empty()
            && changes.deleted.is_empty()
            && changes.collections.is_empty()
            && changes.references.is_empty()
        {
            let mut owners = std::collections::BTreeSet::new();
            for entity in &changes.modified {
                match entity.kind {
                    SdkKind::Expression => {
                        owners.insert(entity.id.clone());
                    }
                    SdkKind::ExpressionEntry => {
                        if let Some((owner, _)) = self.model().get_expression_entry(&entity.id) {
                            owners.insert(owner.id.clone());
                        }
                    }
                    _ => unreachable!(),
                }
            }
            let previous_revision = self.projection_revision;
            self.invalidate_expression_projection();
            self.refresh_expression_index(
                previous_revision,
                &owners.into_iter().collect::<Vec<_>>(),
            );
            self.session.drain_events();
        } else {
            // Other slices retain their existing invalidation policy. Expression
            // structural edits rebuild relations, but never canvas topology.
            self.after_history(!expression_only && fallback_topology);
        }
    }
}
