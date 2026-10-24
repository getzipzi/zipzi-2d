use zipzi_editor_core::{Document, ObjectKind, ObjectRef, WorkspaceKind};
use zipzi_sdk::{
    SceneCollection as C, SceneCopyScope as Scope, SceneCreate as New, SceneEdit as E,
    SceneField as F, SceneFieldValue as V,
};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn run(d: &mut Document, e: E) -> anyhow::Result<zipzi_editor_core::OperationResult> {
    let p = d.plan_scene_authoring(e)?;
    d.execute_scene_authoring(&p)
}
fn create(d: &mut Document, object: New) -> anyhow::Result<String> {
    let id = id();
    run(
        d,
        E::Create {
            id: id.clone(),
            name: String::new(),
            object,
            part_id: None,
            parent_id: None,
            before: None,
            deformation_before: None,
        },
    )?;
    Ok(id)
}
#[test]
fn pending_scene_objects_remain_visible_inspectable_and_persistent() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let mesh = create(
        &mut d,
        New::Mesh {
            texture: Default::default(),
        },
    )?;
    let glue = create(
        &mut d,
        New::Glue {
            a: mesh.clone().into(),
            b: Default::default(),
        },
    )?;
    let warp = create(&mut d, New::Warp { bounds: None })?;
    let index = d.object_index();
    for (kind, id) in [(ObjectKind::Mesh, &mesh), (ObjectKind::Glue, &glue)] {
        let object = index.get(&ObjectRef::new(kind, id)).unwrap();
        assert!(object.diagnostic.is_some());
    }
    let tree = d.mesh_tree();
    assert!(
        tree.iter()
            .flat_map(|n| n.children.iter())
            .any(|n| n.id == mesh)
    );
    assert!(
        tree.iter()
            .flat_map(|n| n.children.iter())
            .any(|n| n.id == warp)
    );
    let snapshot = d.snapshot()?;
    assert_eq!(snapshot.viewport_frame.drawables.len(), 1);
    assert!(!snapshot.viewport_frame.drawables[0].renderable);
    let canvas = d.geometry(&snapshot);
    assert!(canvas.mesh(&mesh).is_some());
    let root = tempfile::tempdir()?;
    let saved = d.save(&root.path().join("pending.zipzi"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert!(
        reopened
            .object_index()
            .get(&ObjectRef::new(ObjectKind::Glue, glue))
            .unwrap()
            .diagnostic
            .is_some()
    );
    Ok(())
}
#[test]
fn scene_copy_and_delete_refresh_canvas_topology_and_keep_context() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let source = d.mesh_tree()[0].children[0].id.clone();
    let selected = d.context().selections.clone();
    let before = d.snapshot()?;
    let retained = d.geometry(&before);
    let p = d.plan_scene_authoring(E::Duplicate {
        ids: vec![source.clone()],
        scope: Scope::Selected,
        destination: None,
        before: None,
    })?;
    let copy = p.copied_ids()[&source].clone();
    let r = d.execute_scene_authoring(&p)?;
    assert!(
        r.suggested_selection
            .contains(&ObjectRef::new(ObjectKind::Mesh, &copy))
    );
    assert_eq!(d.context().selections, selected);
    let after = d.snapshot()?;
    let canvas = d.geometry(&after);
    assert!(canvas.mesh(&copy).is_some());
    assert!(retained.mesh(&copy).is_none());
    assert_eq!(
        after.viewport_frame.drawables.len(),
        before.viewport_frame.drawables.len() + 1
    );
    run(
        &mut d,
        E::Delete {
            ids: vec![copy.clone()],
            scope: Scope::Selected,
        },
    )?;
    let snapshot = d.snapshot()?;
    assert!(d.geometry(&snapshot).mesh(&copy).is_none());
    d.undo()?;
    let snapshot = d.snapshot()?;
    assert!(d.geometry(&snapshot).mesh(&copy).is_some());
    d.undo()?;
    let snapshot = d.snapshot()?;
    assert!(d.geometry(&snapshot).mesh(&copy).is_none());
    Ok(())
}
#[test]
fn scene_label_patch_reuses_relations_and_keeps_retained_canvas_labels() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let mesh = d.mesh_tree()[0].children[0].reference().unwrap();
    let index = d.object_index();
    let old_name = index.name(&mesh).to_owned();
    let snapshot = d.snapshot()?;
    let geometry = d.geometry(&snapshot);
    let address = d.scene_field_address(&mesh.id, F::Label)?;
    let p = d.plan_scene_field_patch(&address, V::Label("After".into()), false)?;
    d.execute_scene_authoring(&p)?;
    let current = d.object_index();
    assert!(std::ptr::eq(&index.relations[0], &current.relations[0]));
    assert_eq!(current.name(&mesh), "After");
    assert_eq!(index.name(&mesh), old_name);
    let after = d.snapshot()?;
    assert_eq!(
        snapshot.runtime_frame.source_revision,
        after.runtime_frame.source_revision
    );
    let current_geometry = d.geometry(&after);
    assert_eq!(current_geometry.index.name(&mesh), "After");
    assert_eq!(geometry.index.name(&mesh), old_name);
    d.undo()?;
    assert!(
        d.plan_scene_field_patch(&address, V::Label("Other".into()), false)
            .is_err()
    );
    assert_eq!(d.object_index().name(&mesh), old_name);
    Ok(())
}
#[test]
fn drawing_roles_are_independent_objects_and_survive_part_organization() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_parts()?;
    let owner = uuid::Uuid::from_u128(101).to_string();
    let object = ObjectRef::new(ObjectKind::DrawOrderGroup, &owner);
    assert!(d.object_index().get(&object).is_some());
    d.navigate(object.clone())?;
    run(
        &mut d,
        E::DissolveDrawingGroup {
            owner: owner.clone(),
        },
    )?;
    assert!(d.object_index().get(&object).is_none());
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Part, &owner))
            .is_some()
    );
    d.undo()?;
    assert!(d.object_index().get(&object).is_some());
    let address = d.drawing_group_address(&owner)?;
    let p = d.plan_drawing_range_patch(&address, [-5, 5])?;
    d.execute_scene_authoring(&p)?;
    assert!(d.object_index().get(&object).is_some());
    Ok(())
}
#[test]
fn glue_management_changes_keep_active_animation_and_preview_clock() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::combined_animation_document()?;
    let a = create(
        &mut d,
        New::Glue {
            a: Default::default(),
            b: Default::default(),
        },
    )?;
    let b = create(
        &mut d,
        New::Glue {
            a: Default::default(),
            b: Default::default(),
        },
    )?;
    let mut editor = super::support::editor_from_document(d);
    let doc = editor.active_id().unwrap();
    editor.set_workspace(WorkspaceKind::Animation)?;
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(doc, motion.clone())?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.4);
    let generation = editor.animation().unwrap().preview_generation;
    let p = editor.plan_scene_authoring(
        doc,
        E::Move {
            ids: vec![b],
            destination: C::Glues,
            before: Some(a),
        },
    )?;
    editor.execute_scene_authoring(doc, &p)?;
    assert_eq!(editor.animation().unwrap().motion.as_ref(), Some(&motion));
    assert_eq!(editor.animation().unwrap().time, 0.4);
    assert!(editor.animation().unwrap().playing);
    assert_eq!(editor.animation().unwrap().preview_generation, generation);
    Ok(())
}
