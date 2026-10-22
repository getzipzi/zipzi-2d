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
        let scene_kinds = |kind| {
            matches!(
                kind,
                SdkKind::Part
                    | SdkKind::Mesh
                    | SdkKind::Transform
                    | SdkKind::Offscreen
                    | SdkKind::Glue
                    | SdkKind::DrawOrderGroup
                    | SdkKind::MeshBinding
                    | SdkKind::SceneBinding
                    | SdkKind::BlendBinding
                    | SdkKind::BindingAxis
                    | SdkKind::BindingSample
            )
        };
        let scene_only = !changes.full_invalidation
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .any(|e| scene_kinds(e.kind))
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .all(|e| scene_kinds(e.kind));
        if scene_only {
            let previous_input = self.animation_input_revision;
            let labels = !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
                && !changes.categories.contains(&ChangeCategory::Content)
                && changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.collections.is_empty()
                && !changes.scene_fields.is_empty()
                && changes
                    .scene_fields
                    .iter()
                    .all(|f| f.field == kasane_sdk::SceneField::Label)
                && !changes.categories.contains(&ChangeCategory::EditorState);
            if labels {
                let previous = self.projection_revision;
                self.invalidate_projection(false);
                let references: Vec<_> = changes
                    .modified
                    .iter()
                    .filter_map(|e| {
                        Some(ObjectRef::new(
                            match e.kind {
                                SdkKind::Part => ObjectKind::Part,
                                SdkKind::Mesh => ObjectKind::Mesh,
                                SdkKind::Transform => ObjectKind::Deformer,
                                SdkKind::Offscreen => ObjectKind::Offscreen,
                                SdkKind::Glue => ObjectKind::Glue,
                                SdkKind::DrawOrderGroup => ObjectKind::DrawOrderGroup,
                                _ => return None,
                            },
                            &e.id,
                        ))
                    })
                    .collect();
                let names: Vec<_> = references
                    .iter()
                    .filter_map(|r| {
                        if r.kind == ObjectKind::Part {
                            self.model()
                                .get_part(&r.id)
                                .map(|p| (r.clone(), p.name.clone()))
                        } else {
                            self.session
                                .document()
                                .scene_field(&r.id, kasane_sdk::SceneField::Label)
                                .ok()
                                .and_then(|v| {
                                    if let kasane_sdk::SceneFieldValue::Label(name) = v {
                                        Some((r.clone(), name))
                                    } else {
                                        None
                                    }
                                })
                        }
                    })
                    .collect();
                let mut cache = self.object_index.lock().unwrap();
                if let Some((revision, index)) = cache.as_mut() {
                    if *revision == previous
                        && references
                            .iter()
                            .all(|r| Arc::make_mut(index).refresh_scene_label(self.model(), r))
                    {
                        *revision = self.projection_revision;
                    }
                }
                drop(cache);
                if let Some(topology) = self.canvas_topology.get_mut().unwrap().as_mut() {
                    for (object, name) in names {
                        Arc::make_mut(topology).rename(&object, &name);
                    }
                }
                self.session.drain_events();
            } else {
                let structural = changes
                    .created
                    .iter()
                    .chain(&changes.deleted)
                    .any(|e| e.kind != SdkKind::DrawOrderGroup)
                    || changes.scene_fields.iter().any(|f| {
                        f.field == kasane_sdk::SceneField::Context
                            && f.entity.kind != SdkKind::DrawOrderGroup
                    })
                    || changes.categories.contains(&ChangeCategory::Topology);
                self.after_history(structural);
            }
            if !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
            {
                self.animation_input_revision = previous_input;
            }
            return;
        }
        let organization_only = !changes.full_invalidation
            && (changes
                .collections
                .iter()
                .any(|c| matches!(c, kasane_sdk::CollectionRef::Organization { .. }))
                || !changes.organization_references.is_empty()
                || changes
                    .created
                    .iter()
                    .chain(&changes.deleted)
                    .chain(&changes.modified)
                    .any(|e| {
                        matches!(
                            e.kind,
                            SdkKind::Parameter | SdkKind::CdiParameterGroup | SdkKind::Part
                        )
                    }))
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .all(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Parameter
                            | SdkKind::CdiParameterGroup
                            | SdkKind::Part
                            | SdkKind::Mesh
                            | SdkKind::Transform
                            | SdkKind::Offscreen
                    )
                });
        if organization_only {
            let previous = self.animation_input_revision;
            let payload_only = changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.collections.is_empty()
                && changes.organization_references.is_empty()
                && !changes.categories.contains(&ChangeCategory::EditorState);
            if payload_only
                && changes.modified.iter().all(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Parameter | SdkKind::CdiParameterGroup | SdkKind::Part
                    )
                })
            {
                let previous_revision = self.projection_revision;
                self.invalidate_projection(false);
                let mut cache = self.object_index.lock().unwrap();
                if let Some((revision, index)) = cache.as_mut() {
                    if *revision == previous_revision
                        && changes.modified.iter().all(|e| {
                            Arc::make_mut(index).refresh_organization_properties(
                                self.model(),
                                e.kind,
                                &e.id,
                            )
                        })
                        && Arc::make_mut(index).refresh_organization_document(self.model())
                    {
                        *revision = self.projection_revision;
                    }
                }
                drop(cache);
                let part_names: Vec<_> = changes
                    .modified
                    .iter()
                    .filter(|e| e.kind == SdkKind::Part)
                    .filter_map(|e| {
                        self.model()
                            .get_part(&e.id)
                            .map(|p| (e.id.clone(), p.name.clone()))
                    })
                    .collect();
                if let Some(topology) = self.canvas_topology.get_mut().unwrap().as_mut() {
                    for (id, name) in part_names {
                        Arc::make_mut(topology)
                            .rename(&ObjectRef::new(ObjectKind::Part, &id), &name);
                    }
                }
                self.session.drain_events();
            } else {
                let scene_changed = changes.created.iter().chain(&changes.deleted).any(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Part | SdkKind::Mesh | SdkKind::Transform | SdkKind::Offscreen
                    )
                }) || changes.organization_references.iter().any(|r| {
                    matches!(
                        r.entity.kind,
                        SdkKind::Part | SdkKind::Mesh | SdkKind::Transform | SdkKind::Offscreen
                    )
                });
                self.after_history(scene_changed);
            }
            if !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
            {
                self.animation_input_revision = previous;
            }
            return;
        }
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
