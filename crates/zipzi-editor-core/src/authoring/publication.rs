//! Receipts drive the same projection updates for commands and history.
use super::*;
use zipzi_core::state::{RecordDomain, RootDomain};

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

    /// Published writes are the shared input for native edits, history and
    /// remote clients. They describe persistent records, not obsolete field logs.
    pub(in crate::document) fn consume_authoring_receipt(
        &mut self,
        receipt: &Publication,
        _fallback_topology: bool,
    ) {
        if receipt.kind == zipzi_sdk::PublicationKind::Unchanged {
            return;
        }
        if receipt.kind == zipzi_sdk::PublicationKind::Replace {
            self.animation_catalog.get_mut().unwrap().take();
            self.after_history(true);
            return;
        }
        let writes = &receipt.writes;
        let editor_only = writes.root_changed(RootDomain::EditorState)
            && writes.records().is_empty()
            && !writes.setup_changed()
            && !ALL_RECORD_DOMAINS.iter().any(|d| writes.order_changed(*d))
            && !ALL_CONTENT_ROOTS.iter().any(|d| writes.root_changed(*d));
        if editor_only {
            let input_revision = self.animation_input_revision;
            self.invalidate_editor_state_projection();
            self.animation_input_revision = input_revision;
            self.session.acknowledge_receipts();
            return;
        }
        let scene = writes.records().iter().any(|r| scene_record(r.domain))
            || ALL_RECORD_DOMAINS
                .iter()
                .any(|d| scene_record(*d) && writes.order_changed(*d))
            || writes.root_changed(RootDomain::DrawOrderGroups)
            || writes.root_changed(RootDomain::HierarchyOrder);
        if scene {
            // The topology builder compares captured immutable records and
            // reuses unchanged geometry, including label-only replacements.
            let meshes = writes
                .records()
                .iter()
                .filter(|r| r.domain == RecordDomain::Mesh)
                .map(|r| r.id.clone())
                .collect();
            self.invalidate_canvas_topology(Some(meshes));
        }
        if writes
            .records()
            .iter()
            .any(|r| matches!(r.domain, RecordDomain::Expression | RecordDomain::Motion))
            || writes.order_changed(RecordDomain::Expression)
            || writes.order_changed(RecordDomain::Motion)
            || writes.root_changed(RootDomain::MotionGroups)
            || writes.root_changed(RootDomain::MotionRoots)
        {
            self.animation_catalog.get_mut().unwrap().take();
        }
        self.after_history(false);
    }
}

const ALL_RECORD_DOMAINS: &[RecordDomain] = &[
    RecordDomain::Asset,
    RecordDomain::Parameter,
    RecordDomain::Part,
    RecordDomain::Transform,
    RecordDomain::Mesh,
    RecordDomain::Binding,
    RecordDomain::SceneBinding,
    RecordDomain::BlendKeyTable,
    RecordDomain::BlendConstraint,
    RecordDomain::BlendBinding,
    RecordDomain::Glue,
    RecordDomain::Offscreen,
    RecordDomain::Expression,
    RecordDomain::Motion,
];
const ALL_CONTENT_ROOTS: &[RootDomain] = &[
    RootDomain::DrawOrderGroups,
    RootDomain::DisplayInfo,
    RootDomain::HierarchyOrder,
    RootDomain::MotionGroups,
    RootDomain::MotionRoots,
    RootDomain::Pose,
    RootDomain::Physics,
    RootDomain::MissingAttachments,
    RootDomain::Model3Settings,
    RootDomain::PackageAttachments,
];
fn scene_record(domain: RecordDomain) -> bool {
    matches!(
        domain,
        RecordDomain::Mesh
            | RecordDomain::Part
            | RecordDomain::Transform
            | RecordDomain::Binding
            | RecordDomain::SceneBinding
            | RecordDomain::BlendBinding
            | RecordDomain::BlendKeyTable
            | RecordDomain::BlendConstraint
            | RecordDomain::Glue
            | RecordDomain::Offscreen
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zipzi_contract::document::DocumentAuthoringEdit;
    use zipzi_contract::document::DocumentValue;
    use zipzi_contract::document::SceneEdit;
    use zipzi_contract::document::SceneFieldValue;
    use zipzi_core::Vec2;

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
        let mut mesh = document.session.document().mesh(&id).unwrap().clone();
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
                .mesh(&id)
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
                .mesh(&id)
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
            edit.edit_organization(zipzi_contract::document::OrganizationEdit::Move {
                ids: vec![mesh.clone()],
                destination: zipzi_contract::document::OrganizationCollection::PartMembers {
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
