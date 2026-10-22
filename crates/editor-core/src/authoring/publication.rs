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
        let pose_only = !changes.full_invalidation
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .any(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Pose | SdkKind::PoseGroup | SdkKind::PoseEntry | SdkKind::PoseLink
                    )
                })
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .all(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Pose | SdkKind::PoseGroup | SdkKind::PoseEntry | SdkKind::PoseLink
                    )
                });
        if pose_only {
            let previous = self.animation_input_revision;
            if changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.collections.is_empty()
                && changes.pose_references.is_empty()
                && !changes.categories.contains(&ChangeCategory::EditorState)
                && changes
                    .modified
                    .iter()
                    .all(|e| matches!(e.kind, SdkKind::Pose | SdkKind::PoseGroup))
            {
                let previous_revision = self.projection_revision;
                self.invalidate_projection(false);
                let mut cache = self.object_index.lock().unwrap();
                if let Some((revision, index)) = cache.as_mut() {
                    if *revision == previous_revision
                        && changes.modified.iter().all(|e| {
                            Arc::make_mut(index).refresh_pose_properties(
                                self.model(),
                                &ObjectRef::new(
                                    if e.kind == SdkKind::Pose {
                                        ObjectKind::Pose
                                    } else {
                                        ObjectKind::PoseGroup
                                    },
                                    &e.id,
                                ),
                            )
                        })
                    {
                        *revision = self.projection_revision;
                    }
                }
                self.session.drain_events();
            } else {
                self.after_history(false);
            }
            if !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
            {
                self.animation_input_revision = previous;
            }
            return;
        }
        let physics_only = !changes.full_invalidation
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .any(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Physics
                            | SdkKind::PhysicsGroup
                            | SdkKind::PhysicsInput
                            | SdkKind::PhysicsOutput
                            | SdkKind::PhysicsNode
                    )
                })
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .all(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Physics
                            | SdkKind::PhysicsGroup
                            | SdkKind::PhysicsInput
                            | SdkKind::PhysicsOutput
                            | SdkKind::PhysicsNode
                    )
                });
        if physics_only {
            let previous = self.animation_input_revision;
            if changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.collections.is_empty()
                && changes.physics_references.is_empty()
                && !changes.categories.contains(&ChangeCategory::EditorState)
            {
                let owners = changes
                    .modified
                    .iter()
                    .filter_map(|e| {
                        if e.kind == SdkKind::PhysicsGroup {
                            Some(e.id.clone())
                        } else {
                            self.model()
                                .physics_owner(&e.id)
                                .filter(|id| self.model().get_physics_group(id).is_some())
                                .map(str::to_owned)
                        }
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let previous_revision = self.projection_revision;
                self.invalidate_projection(false);
                let mut cache = self.object_index.lock().unwrap();
                if let Some((revision, index)) = cache.as_mut() {
                    if *revision == previous_revision
                        && Arc::make_mut(index).refresh_physics_properties(self.model(), &owners)
                    {
                        *revision = self.projection_revision;
                    }
                }
                self.session.drain_events();
            } else {
                self.after_history(false);
            }
            if !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
            {
                self.animation_input_revision = previous;
            }
            return;
        }
        let motion_only = changes.references.is_empty()
            && changes
                .collections
                .iter()
                .all(|c| matches!(c, kasane_sdk::CollectionRef::Motion { .. }))
            && !changes.full_invalidation
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .all(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Motion
                            | SdkKind::MotionGroup
                            | SdkKind::MotionRegistration
                            | SdkKind::MotionTrack
                            | SdkKind::MotionKey
                            | SdkKind::MotionEvent
                            | SdkKind::Attachment
                    )
                })
            && (!changes.created.is_empty()
                || !changes.deleted.is_empty()
                || !changes.modified.is_empty()
                || !changes.collections.is_empty()
                || !changes.motion_references.is_empty());
        if motion_only {
            let previous = self.animation_input_revision;
            if changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.collections.is_empty()
                && changes.motion_references.is_empty()
                && changes.modified.iter().all(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Motion | SdkKind::MotionGroup | SdkKind::MotionRegistration
                    )
                })
            {
                let previous_revision = self.projection_revision;
                self.invalidate_projection(false);
                let mut cache = self.object_index.lock().unwrap();
                if let Some((revision, index)) = cache.as_mut() {
                    if *revision == previous_revision
                        && changes.modified.iter().all(|e| {
                            let kind = match e.kind {
                                SdkKind::Motion => ObjectKind::Motion,
                                SdkKind::MotionGroup => ObjectKind::MotionGroup,
                                SdkKind::MotionRegistration => ObjectKind::MotionRegistration,
                                _ => unreachable!(),
                            };
                            Arc::make_mut(index).refresh_motion_properties(
                                self.model(),
                                &ObjectRef::new(kind, &e.id),
                            )
                        })
                    {
                        *revision = self.projection_revision;
                    }
                }
                self.session.drain_events();
            } else {
                self.after_history(false);
            }
            if !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
            {
                self.animation_input_revision = previous;
            }
            return;
        }
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
