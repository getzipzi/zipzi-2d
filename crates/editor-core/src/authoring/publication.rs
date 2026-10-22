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

    /// Patch a cached projection only at its current revision. A failed patch
    /// leaves the old revision in place so the next read rebuilds the index.
    fn refresh_authoring_projection(
        &mut self,
        refresh: impl FnOnce(&mut crate::ObjectIndex, &kasane_core::Document) -> bool,
    ) {
        let previous = self.projection_revision;
        self.invalidate_projection(false);
        let mut cache = self.object_index.lock().unwrap();
        if let Some((revision, index)) = cache.as_mut() {
            if *revision == previous && refresh(Arc::make_mut(index), self.model()) {
                *revision = self.projection_revision;
            }
        }
        drop(cache);
        self.session.drain_events();
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
        let entities = || {
            changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
        };
        let only_entities = |matches: fn(SdkKind) -> bool| {
            let mut entities = entities().peekable();
            entities.peek().is_some() && entities.all(|e| matches(e.kind))
        };
        let resources_only = only_entities(|kind| kind == SdkKind::Asset);
        let document_only = changes
            .document_fields
            .iter()
            .any(|field| *field != kasane_sdk::DocumentField::DisplayInfo)
            || (!changes.document_fields.is_empty()
                && changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.modified.is_empty());
        if document_only || resources_only {
            let previous = self.animation_input_revision;
            let labels = resources_only
                && changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.collections.is_empty()
                && changes
                    .categories
                    .iter()
                    .all(|c| *c == ChangeCategory::Labels);
            let document_fields_only = document_only
                && changes.collections.is_empty()
                && changes
                    .categories
                    .iter()
                    .all(|c| *c == ChangeCategory::Content)
                && changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.modified.is_empty()
                && changes.document_fields.iter().all(|f| {
                    matches!(
                        f,
                        kasane_sdk::DocumentField::Layout
                            | kasane_sdk::DocumentField::ExportSettings
                            | kasane_sdk::DocumentField::SourceCanvas
                            | kasane_sdk::DocumentField::OpaqueMetadata
                    )
                });
            if labels || document_fields_only {
                let ids = changes
                    .modified
                    .iter()
                    .map(|e| e.id.clone())
                    .collect::<Vec<_>>();
                self.refresh_authoring_projection(|index, model| {
                    if labels {
                        index.refresh_resource_labels(model, &ids)
                    } else {
                        index.refresh_document_metadata(model)
                    }
                });
            } else {
                self.after_history(changes.categories.contains(&ChangeCategory::Topology));
            }
            if !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
            {
                self.animation_input_revision = previous;
            }
            return;
        }
        let scene_kinds = |kind| {
            matches!(
                kind,
                SdkKind::Part
                    | SdkKind::Mesh
                    | SdkKind::Transform
                    | SdkKind::Offscreen
                    | SdkKind::Glue
                    | SdkKind::GlueVertexPair
                    | SdkKind::DrawOrderGroup
                    | SdkKind::MeshBinding
                    | SdkKind::SceneBinding
                    | SdkKind::BlendBinding
                    | SdkKind::BindingAxis
                    | SdkKind::BindingSample
            )
        };
        let scene_only = !changes.full_invalidation && only_entities(scene_kinds);
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
                self.refresh_authoring_projection(|index, model| {
                    references
                        .iter()
                        .all(|r| index.refresh_scene_label(model, r))
                });
                if let Some(topology) = self.canvas_topology.get_mut().unwrap().as_mut() {
                    for (object, name) in names {
                        Arc::make_mut(topology).rename(&object, &name);
                    }
                }
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
        let reference_kinds = |k| {
            matches!(
                k,
                SdkKind::ModelParameterGroup
                    | SdkKind::ModelParameterMember
                    | SdkKind::ModelHitArea
                    | SdkKind::CdiCombinedSet
                    | SdkKind::CdiCombinedMember
            )
        };
        let reference_only = !changes.full_invalidation && only_entities(reference_kinds);
        if reference_only {
            let previous_input = self.animation_input_revision;
            let labels = changes.created.is_empty()
                && changes.deleted.is_empty()
                && changes.collections.is_empty()
                && !changes.reference_fields.is_empty()
                && changes
                    .categories
                    .iter()
                    .all(|c| *c == ChangeCategory::Labels);
            if labels {
                let ids = changes
                    .reference_fields
                    .iter()
                    .map(|f| f.entity.id.clone())
                    .collect::<Vec<_>>();
                self.refresh_authoring_projection(|index, model| {
                    index.refresh_reference_labels(model, &ids)
                });
            } else {
                self.after_history(false);
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
                || entities().any(|e| {
                    matches!(
                        e.kind,
                        SdkKind::Parameter | SdkKind::CdiParameterGroup | SdkKind::Part
                    )
                }))
            && entities().all(|e| {
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
                self.refresh_authoring_projection(|index, model| {
                    changes
                        .modified
                        .iter()
                        .all(|e| index.refresh_organization_properties(model, e.kind, &e.id))
                        && index.refresh_organization_document(model)
                });
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
            && only_entities(|kind| {
                matches!(
                    kind,
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
                self.refresh_authoring_projection(|index, model| {
                    changes.modified.iter().all(|e| {
                        index.refresh_pose_properties(
                            model,
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
                });
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
            && only_entities(|kind| {
                matches!(
                    kind,
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
                self.refresh_authoring_projection(|index, model| {
                    index.refresh_physics_properties(model, &owners)
                });
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
            && entities().all(|e| {
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
                self.refresh_authoring_projection(|index, model| {
                    changes.modified.iter().all(|e| {
                        let kind = match e.kind {
                            SdkKind::Motion => ObjectKind::Motion,
                            SdkKind::MotionGroup => ObjectKind::MotionGroup,
                            SdkKind::MotionRegistration => ObjectKind::MotionRegistration,
                            _ => unreachable!(),
                        };
                        index.refresh_motion_properties(model, &ObjectRef::new(kind, &e.id))
                    })
                });
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
            && entities().all(|entity| {
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
