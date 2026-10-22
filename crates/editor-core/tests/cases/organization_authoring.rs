use editor_core::{Document, ObjectKind, ObjectRef, WorkspaceKind};
use kasane_core::document::CdiParameterGroup;
use kasane_sdk::{
    OrganizationCollection as C, OrganizationDeletePolicy as D, OrganizationEdit as E,
};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn run(d: &mut Document, e: E) -> anyhow::Result<editor_core::OperationResult> {
    let p = d.plan_organization_authoring(e)?;
    d.execute_organization_authoring(&p)
}
fn group(d: &mut Document, parent: Option<&str>) -> anyhow::Result<String> {
    let g = id();
    let result = run(
        d,
        E::CreateGroup {
            group: CdiParameterGroup {
                id: g.clone(),
                runtime_id: g.clone(),
                name: "Empty".into(),
                parent_id: parent.map(str::to_owned),
                extensions: Default::default(),
            },
            before: None,
        },
    )?;
    assert!(
        result
            .suggested_selection
            .contains(&ObjectRef::new(ObjectKind::ParameterGroup, &g))
    );
    Ok(g)
}
#[test]
fn nested_organization_and_copy_project_stable_ids_through_save_and_history() -> anyhow::Result<()>
{
    let mut d = Document::new_blank()?;
    let a = group(&mut d, None)?;
    let b = group(&mut d, Some(&a))?;
    let c = group(&mut d, None)?;
    let parameter = id();
    run(
        &mut d,
        E::CreateParameter {
            parameter: kasane_core::Parameter {
                id: parameter.clone(),
                runtime_id: "Param".into(),
                ..Default::default()
            },
            group_id: Some(b.clone()),
            before: None,
        },
    )?;
    d.set_preview_parameter(&parameter, 0.7)?;
    let context = d.context().selections.clone();
    let result = run(
        &mut d,
        E::Move {
            ids: vec![b.clone()],
            destination: C::ParameterGroups {
                parent_id: Some(c.clone()),
            },
            before: None,
        },
    )?;
    assert!(
        result
            .locations
            .contains(&ObjectRef::new(ObjectKind::ParameterGroup, &c))
    );
    assert_eq!(d.context().selections, context);
    let p = d.plan_organization_authoring(E::Duplicate {
        ids: vec![b.clone()],
        destination: C::ParameterGroups { parent_id: Some(a) },
        before: None,
    })?;
    let copied = p.copied_ids()[&parameter].clone();
    d.execute_organization_authoring(&p)?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Parameter, &copied))
            .is_some()
    );
    let root = tempfile::tempdir()?;
    let saved = d.save(&root.path().join("organization.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(reopened.parameter_groups(), d.parameter_groups());
    d.undo()?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Parameter, &copied))
            .is_none()
    );
    d.redo()?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Parameter, &copied))
            .is_some()
    );
    Ok(())
}
#[test]
fn group_labels_reuse_relations_and_preserve_retained_snapshot() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let parent = group(&mut d, None)?;
    let g = group(&mut d, Some(&parent))?;
    let retained = d.object_index();
    let expected = d.parameter_group_draft(&g)?;
    let mut draft = expected.clone();
    draft.name = "After".into();
    d.update_parameter_group(&g, draft, d.parameter_model_revision())?;
    let current = d.object_index();
    assert!(std::ptr::eq(&current.relations[0], &retained.relations[0]));
    let object = ObjectRef::new(ObjectKind::ParameterGroup, &g);
    assert_eq!(retained.name(&object), "Empty");
    assert_eq!(current.name(&object), "After");
    d.undo()?;
    assert_eq!(d.object_index().name(&object), "Empty");
    d.redo()?;
    assert_eq!(d.object_index().name(&object), "After");
    Ok(())
}
#[test]
fn copied_and_deleted_parts_refresh_canvas_geometry_through_history() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let root = ObjectRef::new(ObjectKind::Part, id());
    run(
        &mut d,
        E::CreatePart {
            part: kasane_core::Part {
                id: root.id.clone(),
                ..Default::default()
            },
            before: None,
        },
    )?;
    let mesh_ids = d
        .object_index()
        .objects
        .iter()
        .filter(|o| o.reference.kind == ObjectKind::Mesh)
        .map(|o| o.reference.id.clone())
        .collect();
    run(
        &mut d,
        E::Move {
            ids: mesh_ids,
            destination: C::PartMembers {
                part_id: Some(root.id.clone()),
            },
            before: None,
        },
    )?;
    let initial = d.snapshot()?;
    let geometry = d.geometry(&initial);
    let count = initial.viewport_frame.drawables.len();
    let p = d.plan_organization_authoring(E::Duplicate {
        ids: vec![root.id],
        destination: C::PartMembers { part_id: None },
        before: None,
    })?;
    let copied: Vec<_> = p.copied_ids().values().cloned().collect();
    d.execute_organization_authoring(&p)?;
    let after = d.snapshot()?;
    assert_eq!(after.viewport_frame.drawables.len(), count * 2);
    let current_geometry = d.geometry(&after);
    assert_eq!(current_geometry.meshes.len(), count * 2);
    assert_eq!(geometry.source.viewport_frame.drawables.len(), count);
    for id in &copied {
        assert!(
            d.object_index()
                .objects
                .iter()
                .any(|o| o.reference.id == *id)
        );
    }
    d.undo()?;
    assert_eq!(d.snapshot()?.viewport_frame.drawables.len(), count);
    d.redo()?;
    assert_eq!(d.snapshot()?.viewport_frame.drawables.len(), count * 2);
    Ok(())
}
#[test]
fn collection_edits_keep_active_animation_selection_and_clock() -> anyhow::Result<()> {
    let (d, _assets) = super::support::combined_animation_document()?;
    let mut editor = super::support::editor_from_document(d);
    let doc = editor.active_id().unwrap();
    editor.set_workspace(WorkspaceKind::Animation)?;
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(doc, motion.clone())?;
    let expression = editor.document().unwrap().expression_resources()[0]
        .id
        .clone();
    editor.select_expression(doc, expression.clone())?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.4);
    let generation = editor.animation().unwrap().preview_generation;
    let g = id();
    let p = editor.plan_organization_authoring(
        doc,
        E::CreateGroup {
            group: CdiParameterGroup {
                id: g.clone(),
                runtime_id: g.clone(),
                name: "Folder".into(),
                parent_id: None,
                extensions: Default::default(),
            },
            before: None,
        },
    )?;
    editor.execute_organization_authoring(doc, &p)?;
    let animation = editor.animation().unwrap();
    assert_eq!(animation.motion.as_ref(), Some(&motion));
    assert_eq!(animation.expression.as_deref(), Some(expression.as_str()));
    assert_eq!(animation.time, 0.4);
    assert!(animation.playing);
    assert_eq!(animation.preview_generation, generation);
    let p = editor.plan_organization_authoring(
        doc,
        E::Delete {
            ids: vec![g],
            policy: D::Dissolve,
        },
    )?;
    editor.execute_organization_authoring(doc, &p)?;
    assert_eq!(editor.animation().unwrap().time, 0.4);
    Ok(())
}

#[test]
fn root_part_sort_anchors_stay_in_the_part_palette_with_unassigned_scene_objects()
-> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_parts()?;
    let a = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(101).to_string());
    let b = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(102).to_string());
    let mesh = d
        .object_index()
        .objects
        .iter()
        .find(|o| o.reference.kind == ObjectKind::Mesh)
        .unwrap()
        .reference
        .id
        .clone();
    run(
        &mut d,
        E::Move {
            ids: vec![mesh.clone()],
            destination: C::PartMembers { part_id: None },
            before: None,
        },
    )?;
    d.reorder_tree(false, &a, &b, editor_core::ReorderPosition::After)?;
    let tree = d.mesh_tree();
    let parts: Vec<_> = tree
        .iter()
        .filter(|node| node.kind == editor_core::TreeNodeKind::Part)
        .collect();
    assert_eq!(
        parts.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        vec![b.id.clone(), a.id.clone()]
    );
    let snapshot = d.snapshot()?;
    let canvas = d.geometry(&snapshot);
    assert!(
        canvas
            .ancestors(&ObjectRef::new(ObjectKind::Mesh, mesh))
            .is_empty()
    );
    d.undo()?;
    Ok(())
}
