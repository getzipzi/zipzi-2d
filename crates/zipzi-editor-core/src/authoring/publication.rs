//! Receipts drive the same projection updates for commands and history.
use super::*;
use zipzi_sdk::{ChangeCategory, ObjectKind as SdkKind};

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
                    .all(|id| Arc::make_mut(index).refresh_expression(&self.model(), id))
            {
                *revision = self.projection_revision;
            }
        }
    }

    /// Patch a cached projection only at its current revision. A failed patch
    /// leaves the old revision in place so the next read rebuilds the index.
    fn refresh_authoring_projection(
        &mut self,
        refresh: impl FnOnce(&mut crate::ObjectIndex, &zipzi_core::Document) -> bool,
    ) {
        let previous = self.projection_revision;
        self.invalidate_projection(false);
        let mut cache = self.object_index.lock().unwrap();
        if let Some((revision, index)) = cache.as_mut() {
            if *revision == previous && refresh(Arc::make_mut(index), &self.model()) {
                *revision = self.projection_revision;
            }
        }
        drop(cache);
        self.session.acknowledge_receipts();
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
        if changes.full_invalidation {
            self.after_history(true);
            return;
        }
        // Canvas structure and labels can change alongside document metadata or
        // another domain. Handle them before choosing a projection fast path.
        let scene_kind = |kind| {
            matches!(
                kind,
                SdkKind::Part
                    | SdkKind::Mesh
                    | SdkKind::Transform
                    | SdkKind::Offscreen
                    | SdkKind::Glue
            )
        };
        let structure_changed = (changes.categories.contains(&ChangeCategory::Topology)
            && changes
                .created
                .iter()
                .chain(&changes.deleted)
                .chain(&changes.modified)
                .any(|entity| scene_kind(entity.kind)))
            || changes.scene_fields.iter().any(|field| {
                field.field == zipzi_sdk::SceneField::Context && scene_kind(field.entity.kind)
            })
            || changes
                .organization_references
                .iter()
                .any(|edge| scene_kind(edge.entity.kind));
        if structure_changed {
            let changed = changes
                .modified
                .iter()
                .chain(&changes.created)
                .chain(&changes.deleted)
                .filter(|entity| entity.kind == SdkKind::Mesh)
                .map(|entity| entity.id.clone())
                .collect();
            self.invalidate_canvas_topology(Some(changed));
        } else if changes.categories.contains(&ChangeCategory::Labels)
            && self.canvas_topology.get_mut().unwrap().is_some()
        {
            let names: Vec<_> = changes
                .modified
                .iter()
                .filter_map(|entity| {
                    let kind = match entity.kind {
                        SdkKind::Part => ObjectKind::Part,
                        SdkKind::Mesh => ObjectKind::Mesh,
                        SdkKind::Transform => ObjectKind::Deformer,
                        SdkKind::Offscreen => ObjectKind::Offscreen,
                        SdkKind::Glue => ObjectKind::Glue,
                        _ => return None,
                    };
                    self.session
                        .document()
                        .scene_field(&entity.id, zipzi_sdk::SceneField::Label)
                        .ok()
                        .and_then(|value| match value {
                            zipzi_sdk::SceneFieldValue::Label(name) => {
                                Some((ObjectRef::new(kind, &entity.id), name))
                            }
                            _ => None,
                        })
                })
                .collect();
            if let Some(topology) = self.canvas_topology.get_mut().unwrap().as_mut() {
                for (object, name) in names {
                    Arc::make_mut(topology).rename(&object, &name);
                }
            }
        }
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
            .any(|field| *field != zipzi_sdk::DocumentField::DisplayInfo)
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
                        zipzi_sdk::DocumentField::Layout
                            | zipzi_sdk::DocumentField::ExportSettings
                            | zipzi_sdk::DocumentField::SourceCanvas
                            | zipzi_sdk::DocumentField::OpaqueMetadata
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
                self.after_history(
                    changes.categories.contains(&ChangeCategory::Topology) && !structure_changed,
                );
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
                    .all(|f| f.field == zipzi_sdk::SceneField::Label)
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
                self.refresh_authoring_projection(|index, model| {
                    references
                        .iter()
                        .all(|r| index.refresh_scene_label(model, r))
                });
            } else {
                let structural = changes
                    .created
                    .iter()
                    .chain(&changes.deleted)
                    .any(|e| e.kind != SdkKind::DrawOrderGroup)
                    || changes.scene_fields.iter().any(|f| {
                        f.field == zipzi_sdk::SceneField::Context
                            && f.entity.kind != SdkKind::DrawOrderGroup
                    })
                    || changes.categories.contains(&ChangeCategory::Topology);
                self.after_history(structural && !structure_changed);
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
                .any(|c| matches!(c, zipzi_sdk::CollectionRef::Organization { .. }))
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
                self.after_history(scene_changed && !structure_changed);
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
                .all(|c| matches!(c, zipzi_sdk::CollectionRef::Motion { .. }))
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
        let previous_input = self.animation_input_revision;
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
            self.session.acknowledge_receipts();
        } else {
            // Other slices retain their existing invalidation policy. Expression
            // structural edits rebuild relations, but never canvas topology.
            self.after_history(!expression_only && fallback_topology && !structure_changed);
        }
        // Library order and labels change presentation, while entry order/content
        // carries EvaluationInputs and must still invalidate the running preview.
        if expression_only
            && !changes
                .categories
                .contains(&ChangeCategory::EvaluationInputs)
        {
            self.animation_input_revision = previous_input;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zipzi_core::Vec2;
    use zipzi_sdk::{DocumentAuthoringEdit, DocumentValue, SceneEdit, SceneFieldValue};

    #[test]
    fn manual_mesh_constraint_only_receipt_invalidates_canvas_topology_through_history()
    -> anyhow::Result<()> {
        let mut document = Document::new_blank()?;
        let id = Uuid::new_v4().to_string();
        let other = Uuid::new_v4().to_string();
        document.session.edit("Mesh", None, |edit| {
            edit.create_mesh(zipzi_core::Mesh {
                id: id.clone(),
                vertex_ids: vec![0, 1, 2, 3],
                base_positions: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(1., 1.),
                    Vec2::new(0., 1.),
                ],
                uvs: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(1., 1.),
                    Vec2::new(0., 1.),
                ],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                meshing: Some(zipzi_core::MeshMeshing {
                    boundary: vec![0, 1, 2, 3],
                    constraints: vec![],
                }),
                ..Default::default()
            })?;
            edit.create_mesh(zipzi_core::Mesh {
                id: other.clone(),
                vertex_ids: vec![10, 11, 12],
                base_positions: vec![Vec2::new(2., 0.), Vec2::new(3., 0.), Vec2::new(2., 1.)],
                uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                triangles: vec![[10, 11, 12]],
                ..Default::default()
            })
        })?;
        document.after_history(true);
        let before = document.canvas_topology();
        assert_eq!(before.meshes[&id].automatic_edges.len(), 1);
        let evaluation = document.session.evaluation_revision();
        assert!(
            document
                .validate_mesh_grid(
                    &ObjectRef::new(ObjectKind::Mesh, &id),
                    crate::MeshGridOptions {
                        columns: 1,
                        rows: 1
                    },
                )
                .unwrap_err()
                .to_string()
                .contains("modeling topology")
        );
        let mut mesh = document.session.document().get_mesh(&id).unwrap().clone();
        mesh.meshing
            .as_mut()
            .unwrap()
            .constraints
            .push(zipzi_core::MeshConstraint {
                endpoints: [0, 2],
                kind: zipzi_core::MeshConstraintKind::User,
            });
        let (_, receipt) = document
            .session
            .edit("Constrain", None, |edit| edit.replace_mesh(mesh))?;
        document.consume_authoring_receipt(&receipt, false);
        let constrained = document.canvas_topology();
        assert!(!Arc::ptr_eq(&before, &constrained));
        assert!(!Arc::ptr_eq(&before.meshes[&id], &constrained.meshes[&id]));
        assert!(Arc::ptr_eq(
            &before.meshes[&other],
            &constrained.meshes[&other]
        ));
        assert_eq!(constrained.meshes[&id].user_edges.len(), 1);
        assert!(constrained.meshes[&id].automatic_edges.is_empty());
        assert_eq!(document.session.evaluation_revision(), evaluation);
        document.undo()?;
        assert!(!Arc::ptr_eq(&constrained, &document.canvas_topology()));
        assert!(
            document
                .model()
                .get_mesh(&id)
                .unwrap()
                .meshing
                .as_ref()
                .unwrap()
                .constraints
                .is_empty()
        );
        document.redo()?;
        assert_eq!(
            document
                .model()
                .get_mesh(&id)
                .unwrap()
                .meshing
                .as_ref()
                .unwrap()
                .constraints
                .len(),
            1
        );
        assert_eq!(document.session.evaluation_revision(), evaluation);
        let before_position = document.canvas_topology();
        let before_source = *document.canvas_topology_source.lock().unwrap();
        let (_, receipt) = document.session.edit("Position", None, |edit| {
            edit.update_positions(&id, &[0], &[Vec2::new(0.05, 0.05)])
        })?;
        document.consume_authoring_receipt(&receipt, false);
        let after_position = document.canvas_topology();
        assert_ne!(
            before_source,
            *document.canvas_topology_source.lock().unwrap()
        );
        assert!(Arc::ptr_eq(
            &before_position.meshes[&id],
            &after_position.meshes[&id]
        ));
        Ok(())
    }

    #[test]
    fn mixed_document_and_scene_labels_refresh_canvas_through_history() -> anyhow::Result<()> {
        let mut document = Document::new_blank()?;
        let id = Uuid::new_v4().to_string();
        document.session.edit("Mesh", None, |edit| {
            edit.create_mesh(zipzi_core::Mesh {
                id: id.clone(),
                name: "Before".into(),
                ..Default::default()
            })
        })?;
        document.after_history(true);
        let object = ObjectRef::new(ObjectKind::Mesh, &id);
        let before = document.canvas_topology();
        let index = document.object_index();
        let (_, receipt) = document.session.edit("Layout and label", None, |edit| {
            edit.edit_document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::Layout(None),
                value: DocumentValue::Layout(Some(std::collections::BTreeMap::from([(
                    "CenterX".into(),
                    3.,
                )]))),
            })?;
            edit.edit_scene(SceneEdit::Patch {
                id: id.clone(),
                expected: SceneFieldValue::Label("Before".into()),
                value: SceneFieldValue::Label("After".into()),
                clear_glue_pairs: false,
            })
        })?;
        document.consume_authoring_receipt(&receipt, false);
        assert_eq!(document.canvas_topology().index.name(&object), "After");
        assert_eq!(document.object_index().get(&object).unwrap().name, "After");
        assert_eq!(before.index.name(&object), "Before");
        assert_eq!(index.get(&object).unwrap().name, "Before");
        assert!(before.same_geometry(&document.canvas_topology()));
        document.undo()?;
        assert_eq!(document.canvas_topology().index.name(&object), "Before");
        document.redo()?;
        assert_eq!(document.canvas_topology().index.name(&object), "After");
        assert!(before.same_geometry(&document.canvas_topology()));
        assert!(document.session.acknowledge_receipts().is_empty());
        Ok(())
    }

    #[test]
    fn mixed_metadata_and_organization_move_refresh_canvas_parentage() -> anyhow::Result<()> {
        let mut document = Document::new_blank()?;
        let mesh = Uuid::new_v4().to_string();
        let part = Uuid::new_v4().to_string();
        document.session.edit("Scene", None, |edit| {
            edit.create_part(zipzi_core::Part {
                id: part.clone(),
                ..Default::default()
            })?;
            edit.create_mesh(zipzi_core::Mesh {
                id: mesh.clone(),
                ..Default::default()
            })
        })?;
        document.after_history(true);
        let object = ObjectRef::new(ObjectKind::Mesh, &mesh);
        let previous = document.canvas_topology();
        let (_, receipt) = document.session.edit("Move and layout", None, |edit| {
            edit.edit_document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::Layout(None),
                value: DocumentValue::Layout(Some(Default::default())),
            })?;
            edit.edit_organization(zipzi_sdk::OrganizationEdit::Move {
                ids: vec![mesh.clone()],
                destination: zipzi_sdk::OrganizationCollection::PartMembers {
                    part_id: Some(part.clone()),
                },
                before: None,
            })
        })?;
        document.consume_authoring_receipt(&receipt, false);
        assert_eq!(
            document
                .canvas_topology()
                .index
                .get(&object)
                .unwrap()
                .part_id,
            part
        );
        assert!(previous.index.get(&object).unwrap().part_id.is_empty());
        document.undo()?;
        assert!(
            document
                .canvas_topology()
                .index
                .get(&object)
                .unwrap()
                .part_id
                .is_empty()
        );
        document.redo()?;
        assert_eq!(
            document
                .canvas_topology()
                .index
                .get(&object)
                .unwrap()
                .part_id,
            part
        );
        Ok(())
    }

    #[test]
    fn physics_topology_does_not_rebuild_canvas_geometry() -> anyhow::Result<()> {
        let mut document = Document::new_blank()?;
        let topology = document.canvas_topology();
        let plan =
            document.plan_physics_authoring(zipzi_core::document::PhysicsEdit::CreateGroup {
                asset_id: Uuid::new_v4().to_string(),
                id: Uuid::new_v4().to_string(),
                name: "Physics".into(),
                before: None,
            })?;
        let result = document.execute_physics_authoring(&plan)?;
        assert!(
            result
                .receipt
                .changes
                .categories
                .contains(&ChangeCategory::Topology)
        );
        assert!(Arc::ptr_eq(&topology, &document.canvas_topology()));
        document.undo()?;
        assert!(Arc::ptr_eq(&topology, &document.canvas_topology()));
        document.redo()?;
        assert!(Arc::ptr_eq(&topology, &document.canvas_topology()));
        Ok(())
    }
}
