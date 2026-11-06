//! Receipts drive the same projection updates for commands and history.
use super::*;
use zipzi_core::state::{DocumentState, RecordChange, RecordDomain, RootDomain, WriteSet};

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
        before: Option<&DocumentState>,
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
            self.invalidate_editor_state_projection();
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
        if let Some(before) = before {
            let model = self.model();
            if let Some((revision, index)) = self.object_index.get_mut().unwrap().as_mut() {
                if *revision == self.projection_revision
                    && refresh_index(Arc::make_mut(index), writes, before, &model)
                {
                    *revision += 1;
                }
            }
        }
        let animation_revision = self.animation_input_revision;
        self.after_history(false);
        if !writes.animation_inputs_changed() {
            self.animation_input_revision = animation_revision;
        }
    }
}

/// Refresh only the rows whose published records changed. Unsupported writes
/// leave the old cache revision in place, so the next read rebuilds it.
fn refresh_index(
    index: &mut crate::ObjectIndex,
    writes: &WriteSet,
    before: &DocumentState,
    after: &DocumentState,
) -> bool {
    if ALL_RECORD_DOMAINS
        .iter()
        .any(|domain| writes.order_changed(*domain))
        || writes.root_changed(RootDomain::EditorState)
    {
        return false;
    }

    for root in ALL_CONTENT_ROOTS
        .iter()
        .copied()
        .filter(|root| writes.root_changed(*root))
    {
        if !refresh_root(index, root, before, after) {
            return false;
        }
    }
    if writes.setup_changed() && !index.refresh_document_metadata(after) {
        return false;
    }

    macro_rules! scene_label {
        ($getter:ident, $kind:ident, $id:expr) => {{
            let (Some(old), Some(new)) = (before.$getter($id), after.$getter($id)) else {
                return false;
            };
            let mut original = old.clone();
            original.name.clone_from(&new.name);
            original == *new
                && index.refresh_scene_label(after, &ObjectRef::new(ObjectKind::$kind, $id))
        }};
    }
    writes.records().iter().all(|record| {
        if record.change != RecordChange::Replaced {
            return false;
        }
        let id = &record.id;
        match record.domain {
            RecordDomain::Asset => {
                before
                    .asset(id)
                    .zip(after.asset(id))
                    .is_some_and(|(old, new)| old.content == new.content)
                    && index.refresh_resource_labels(after, std::slice::from_ref(id))
            }
            RecordDomain::Expression => {
                before
                    .expression(id)
                    .zip(after.expression(id))
                    .is_some_and(|(old, new)| {
                        old.entries.len() == new.entries.len()
                            && old
                                .entries
                                .iter()
                                .zip(&new.entries)
                                .all(|(a, b)| a.id == b.id && a.target == b.target)
                    })
                    && index.refresh_expression(after, id)
            }
            RecordDomain::Part => {
                let group = ObjectRef::new(ObjectKind::DrawOrderGroup, id);
                scene_label!(part, Part, id)
                    && (index.get(&group).is_none() || index.refresh_scene_label(after, &group))
            }
            RecordDomain::Mesh => scene_label!(mesh, Mesh, id),
            RecordDomain::Transform => scene_label!(transform, Deformer, id),
            RecordDomain::Offscreen => scene_label!(offscreen, Offscreen, id),
            RecordDomain::Glue => scene_label!(glue, Glue, id),
            _ => false,
        }
    })
}

fn refresh_root(
    index: &mut crate::ObjectIndex,
    root: RootDomain,
    before: &DocumentState,
    after: &DocumentState,
) -> bool {
    match root {
        RootDomain::DisplayInfo => {
            let mut old = before.display_info().clone();
            let new = after.display_info();
            for (a, b) in old
                .parameter_groups
                .iter_mut()
                .flatten()
                .zip(new.parameter_groups.iter().flatten())
            {
                if a.name != b.name {
                    a.name.clone_from(&b.name);
                    if !index.refresh_organization_properties(
                        after,
                        zipzi_contract::authoring::ObjectKind::CdiParameterGroup,
                        &b.id,
                    ) {
                        return false;
                    }
                }
            }
            old == *new && index.refresh_organization_document(after)
        }
        RootDomain::MotionGroups => {
            let old = before.motion_groups();
            let new = after.motion_groups();
            old.len() == new.len()
                && old.iter().zip(new).all(|(a, b)| {
                    a.id == b.id
                        && a.entries == b.entries
                        && (a.name == b.name
                            || index.refresh_motion_properties(
                                after,
                                &ObjectRef::new(ObjectKind::MotionGroup, &b.id),
                            ))
                })
        }
        RootDomain::Pose => {
            let (Some(old), Some(new)) = (before.pose().as_ref(), after.pose().as_ref()) else {
                return false;
            };
            let mut old = old.as_ref().clone();
            for (a, b) in old.groups.iter_mut().zip(&new.groups) {
                if a.name != b.name {
                    a.name.clone_from(&b.name);
                    if !index.refresh_pose_properties(
                        after,
                        &ObjectRef::new(ObjectKind::PoseGroup, &b.id),
                    ) {
                        return false;
                    }
                }
            }
            old == **new
        }
        RootDomain::Model3Settings => {
            let mut old = before.model3_settings().clone();
            let new = after.model3_settings();
            let mut labels = Vec::new();
            for (a, b) in old
                .groups
                .iter_mut()
                .flatten()
                .zip(new.groups.iter().flatten())
            {
                if a.name != b.name {
                    a.name.clone_from(&b.name);
                    labels.push(b.id.clone());
                }
            }
            for (a, b) in old
                .hit_areas
                .iter_mut()
                .flatten()
                .zip(new.hit_areas.iter().flatten())
            {
                if a.name != b.name {
                    a.name.clone_from(&b.name);
                    labels.push(b.id.clone());
                }
            }
            old.layout.clone_from(&new.layout);
            old == *new && index.refresh_reference_labels(after, &labels)
        }
        _ => false,
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
            crate::test_sdk::insert_fixture_mesh(
                edit,
                zipzi_core::Mesh {
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
                },
            )?;
            crate::test_sdk::insert_fixture_mesh(
                edit,
                zipzi_core::Mesh {
                    id: other.clone(),
                    vertex_ids: vec![10, 11, 12],
                    base_positions: vec![Vec2::new(2., 0.), Vec2::new(3., 0.), Vec2::new(2., 1.)],
                    uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    triangles: vec![[10, 11, 12]],
                    ..Default::default()
                },
            )
        })?;
        document.after_history(true);
        let before = document.canvas_topology();
        assert_eq!(before.meshes[&id].automatic_edges.len(), 1);
        let before_frame = document.snapshot()?.viewport_frame;
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
        document.consume_authoring_receipt(&receipt, None);
        let constrained = document.canvas_topology();
        assert!(!Arc::ptr_eq(&before, &constrained));
        assert!(!Arc::ptr_eq(&before.meshes[&id], &constrained.meshes[&id]));
        assert!(Arc::ptr_eq(
            &before.meshes[&other],
            &constrained.meshes[&other]
        ));
        assert_eq!(constrained.meshes[&id].user_edges.len(), 1);
        assert!(constrained.meshes[&id].automatic_edges.is_empty());
        assert_eq!(
            document.snapshot()?.viewport_frame.drawables,
            before_frame.drawables
        );
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
        assert_eq!(
            document.snapshot()?.viewport_frame.drawables,
            before_frame.drawables
        );
        let before_position = document.canvas_topology();
        let before_source = *document.canvas_topology_source.lock().unwrap();
        let (_, receipt) = document.session.edit("Position", None, |edit| {
            edit.set_vertex_positions(&id, &[(0, Vec2::new(0.05, 0.05))])
        })?;
        document.consume_authoring_receipt(&receipt, None);
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
            crate::test_sdk::insert_fixture_mesh(
                edit,
                zipzi_core::Mesh {
                    id: id.clone(),
                    name: "Before".into(),
                    ..Default::default()
                },
            )
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
            edit.edit_scene(
                SceneEdit::Patch {
                    id: id.clone(),
                    expected: SceneFieldValue::Label("Before".into()),
                    value: SceneFieldValue::Label("After".into()),
                    clear_glue_pairs: false,
                },
                Default::default(),
                &Default::default(),
            )
            .map(|_| ())
        })?;
        document.consume_authoring_receipt(&receipt, None);
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
            crate::test_sdk::insert_fixture_part(
                edit,
                zipzi_core::Part {
                    id: part.clone(),
                    ..Default::default()
                },
            )?;
            crate::test_sdk::insert_fixture_mesh(
                edit,
                zipzi_core::Mesh {
                    id: mesh.clone(),
                    ..Default::default()
                },
            )
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
        document.consume_authoring_receipt(&receipt, None);
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
        assert!(result.receipt.writes.root_changed(RootDomain::Physics));
        assert!(Arc::ptr_eq(&topology, &document.canvas_topology()));
        document.undo()?;
        assert!(Arc::ptr_eq(&topology, &document.canvas_topology()));
        document.redo()?;
        assert!(Arc::ptr_eq(&topology, &document.canvas_topology()));
        Ok(())
    }
}
