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
    let tree = d.part_tree();
    assert!(tree
        .iter()
        .flat_map(|n| n.children.iter())
        .any(|n| n.id == mesh));
    assert!(tree
        .iter()
        .flat_map(|n| n.children.iter())
        .any(|n| n.id == warp));
    let snapshot = d.snapshot()?;
    assert_eq!(snapshot.viewport_frame.drawables.len(), 1);
    assert!(!snapshot.viewport_frame.drawables[0].renderable);
    let canvas = d.geometry(&snapshot);
    assert!(canvas.mesh(&mesh).is_some());
    let root = tempfile::tempdir()?;
    let saved = d.save(&root.path().join("pending.zipzi"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert!(reopened
        .object_index()
        .get(&ObjectRef::new(ObjectKind::Glue, glue))
        .unwrap()
        .diagnostic
        .is_some());
    Ok(())
}
#[test]
fn scene_copy_and_delete_refresh_canvas_topology_and_keep_context() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let source = d.part_tree()[0].children[0].id.clone();
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
    assert!(r
        .suggested_selection
        .contains(&ObjectRef::new(ObjectKind::Mesh, &copy)));
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
    let mesh = d.part_tree()[0].children[0].reference().unwrap();
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
    assert!(d
        .plan_scene_field_patch(&address, V::Label("Other".into()), false)
        .is_err());
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
    assert!(d
        .object_index()
        .get(&ObjectRef::new(ObjectKind::Part, &owner))
        .is_some());
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

#[test]
fn tree_deletion_uses_native_scope_and_is_atomic_and_undoable() -> anyhow::Result<()> {
    use zipzi_editor_core::{PaletteProjection as P, SceneObjectType as T};
    let mut d = Document::new_blank()?;
    let mut add = |kind, selection: &[ObjectRef]| -> anyhow::Result<ObjectRef> {
        let p = d.plan_scene_object_creation(kind, "Object".into(), selection)?;
        Ok(d.execute_scene_object_creation(&p)?.suggested_selection[0].clone())
    };
    let part = add(T::Part, &[])?;
    let warp = add(T::Warp, std::slice::from_ref(&part))?;
    let mesh_id = id();
    run(
        &mut d,
        E::Create {
            id: mesh_id.clone(),
            name: "Child".into(),
            object: New::Mesh {
                texture: Default::default(),
            },
            part_id: Some(part.id.clone()),
            parent_id: Some(warp.id.clone()),
            before: None,
            deformation_before: None,
        },
    )?;
    let mesh = ObjectRef::new(ObjectKind::Mesh, &mesh_id);
    d.select_object(mesh.clone())?;
    let organization = d.plan_tree_deletion(P::Organization, std::slice::from_ref(&part))?;
    assert!(organization.removed_ids().contains(&part.id));
    assert!(organization.removed_ids().contains(&warp.id));
    assert!(organization.removed_ids().contains(&mesh.id));
    let deformation = d.plan_tree_deletion(P::Deformation, std::slice::from_ref(&warp))?;
    assert!(!deformation.removed_ids().contains(&part.id));
    assert!(deformation.removed_ids().contains(&mesh.id));
    let mut editor = super::support::editor_from_document(d);
    let document = editor.active_id().unwrap();
    let history = editor.document().unwrap().history_lengths();
    editor.execute_tree_deletion(document, &deformation)?;
    let d = editor.document().unwrap();
    assert!(d.object_index().get(&part).is_some());
    assert!(d.object_index().get(&warp).is_none());
    assert!(d.object_index().get(&mesh).is_none());
    assert_eq!(d.context().selections, vec![part.clone()]);
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    editor.execute(zipzi_editor_core::EditorCommand::Undo)?;
    assert!(editor
        .document()
        .unwrap()
        .object_index()
        .get(&mesh)
        .is_some());
    assert!(editor
        .execute_tree_deletion(document, &deformation)
        .is_err());
    Ok(())
}

#[test]
fn tree_deletion_reviews_locked_descendants_and_cleans_external_references() -> anyhow::Result<()> {
    use zipzi_editor_core::{PaletteProjection as P, SceneObjectType as T};
    let mut d = Document::new_blank()?;
    let p = d.plan_scene_object_creation(T::Part, "Owner".into(), &[])?;
    let part = d.execute_scene_object_creation(&p)?.suggested_selection[0].clone();
    let p = d.plan_scene_object_creation(T::Mesh, "Mesh".into(), std::slice::from_ref(&part))?;
    let mesh = d.execute_scene_object_creation(&p)?.suggested_selection[0].clone();
    d.set_objects_locked(std::slice::from_ref(&mesh), true)?;
    let blocked = d.plan_tree_deletion(P::Organization, std::slice::from_ref(&part))?;
    assert_eq!(
        blocked.summary().state,
        zipzi_sdk::DeletionReviewState::Blocked
    );
    assert!(d.execute_tree_deletion(&blocked).is_err());
    d.set_objects_locked(std::slice::from_ref(&mesh), false)?;
    let glue = create(
        &mut d,
        New::Glue {
            a: mesh.id.clone().into(),
            b: Default::default(),
        },
    )?;
    let history = d.history_lengths();
    let plan = d.plan_tree_deletion(P::Organization, std::slice::from_ref(&part))?;
    assert_eq!(
        plan.summary().state,
        zipzi_sdk::DeletionReviewState::NeedsConfirmation
    );
    assert!(plan.removed_ids().contains(&glue));
    assert_eq!(d.history_lengths(), history);
    run(
        &mut d,
        E::Delete {
            ids: vec![glue],
            scope: Scope::Selected,
        },
    )?;
    assert!(d
        .plan_tree_deletion(P::Organization, std::slice::from_ref(&part))
        .is_ok());
    Ok(())
}

#[test]
fn tree_deletion_scope_is_projection_independent_and_preview_context_expires() -> anyhow::Result<()>
{
    use zipzi_editor_core::{NewParameter, PaletteProjection as P, SceneObjectType as T};
    let mut d = Document::new_blank()?;
    d.create_parameter(NewParameter {
        name: "Driver".into(),
        runtime_id: "Driver".into(),
        minimum: 0.,
        maximum: 1.,
        ..Default::default()
    })?;
    let parameter = d.parameters()[0].id.clone();
    let create = d.plan_scene_object_creation(T::Mesh, "Mesh".into(), &[])?;
    let mesh = d
        .execute_scene_object_creation(&create)?
        .suggested_selection[0]
        .clone();
    let a = d.plan_tree_deletion(P::Organization, std::slice::from_ref(&mesh))?;
    let b = d.plan_tree_deletion(P::Deformation, std::slice::from_ref(&mesh))?;
    assert_eq!(a.removed_ids(), b.removed_ids());
    d.set_preview_parameter(&parameter, 0.3)?;
    assert!(a.valid_for(&d));
    let mut intent = d.deletion_intent(std::slice::from_ref(&mesh))?;
    intent.request.reparent.insert(
        mesh.id.clone(),
        zipzi_sdk::ReparentStrategy::PreserveEditState,
    );
    let task = d.start_tree_deletion(intent)?;
    let current = task.run().finish()?;
    assert!(current.valid_for(&d));
    d.set_preview_parameter(&parameter, 0.8)?;
    assert!(!current.valid_for(&d));
    assert!(d.execute_tree_deletion(&current).is_err());
    let host = d.runtime_host();
    let plans = host.read().modeling_plan_count();
    let task = d.start_tree_deletion(d.deletion_intent(std::slice::from_ref(&mesh))?)?;
    task.cancellation().cancel();
    assert!(task.run().finish().is_err());
    assert_eq!(host.read().modeling_plan_count(), plans);
    let task = d.start_tree_deletion(d.deletion_intent(std::slice::from_ref(&mesh))?)?;
    drop(task);
    assert_eq!(host.read().modeling_plan_count(), plans);
    assert!(d.object_index().get(&mesh).is_some());
    Ok(())
}

#[test]
fn tree_deletion_background_planning_can_overlap_runtime_reads() -> anyhow::Result<()> {
    let mut document = Document::new_blank()?;
    let mesh = ObjectRef::new(
        ObjectKind::Mesh,
        create(
            &mut document,
            New::Mesh {
                texture: Default::default(),
            },
        )?,
    );
    let host = document.runtime_host();
    let task = document.start_tree_deletion(document.deletion_intent(&[mesh.clone()])?)?;
    // Rendering keeps a read alive while the worker finishes its frozen plan.
    let runtime = host.read();
    let finished = std::thread::spawn(move || task.run()).join();
    assert_eq!(runtime.modeling_plan_count(), 0);
    let jobs = runtime.list_jobs(host.client_id())?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].state, zipzi_runtime::JobState::Running);
    drop(runtime);
    let plan = finished
        .expect("background deletion must not write to the Runtime")
        .finish()?;
    assert!(plan.valid_for(&document));
    assert_eq!(plan.removed_ids(), vec![mesh.id]);
    assert!(host.read().list_jobs(host.client_id())?.is_empty());
    assert_eq!(host.read().modeling_plan_count(), 1);
    drop(plan);
    assert_eq!(host.read().modeling_plan_count(), 0);
    Ok(())
}

#[test]
fn tree_deletion_completion_releases_jobs_after_source_view_is_dropped() -> anyhow::Result<()> {
    for cancel in [false, true] {
        let mut document = Document::new_blank()?;
        let id = document.create_parameter(zipzi_editor_core::NewParameter::default())?;
        let object = ObjectRef::new(ObjectKind::Parameter, id);
        let host = document.runtime_host();
        let task = document.start_tree_deletion(document.deletion_intent(&[object])?)?;
        let cancellation = task.cancellation();
        let completion = std::thread::spawn(move || task.run())
            .join()
            .expect("background deletion must finish");
        drop(document);
        if cancel {
            cancellation.cancel();
        }
        let plan = completion.finish();
        assert_eq!(plan.is_err(), cancel);
        assert!(host.read().list_jobs(host.client_id())?.is_empty());
        drop(plan);
        assert_eq!(host.read().modeling_plan_count(), 0);
    }
    Ok(())
}

#[test]
fn tree_deletion_parameter_uses_document_default_and_is_independent_of_preview_revision(
) -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let parameter = d.create_parameter(zipzi_editor_core::NewParameter::default())?;
    let target = ObjectRef::new(ObjectKind::Parameter, &parameter);
    d.set_preview_parameter(&parameter, 0.8)?;
    let plan = d.plan_tree_deletion(
        zipzi_editor_core::PaletteProjection::Organization,
        &[target.clone()],
    )?;
    d.set_preview_parameter(&parameter, 0.2)?;
    assert!(plan.valid_for(&d));
    d.execute_tree_deletion(&plan)?;
    assert!(d.object_index().get(&target).is_none());
    d.undo()?;
    assert!(d.object_index().get(&target).is_some());
    assert!(!plan.valid_for(&d));
    Ok(())
}
#[test]
fn tree_deletion_business_resource_can_execute_in_animation_workspace_without_switching(
) -> anyhow::Result<()> {
    let (d, _assets) = super::support::combined_animation_document()?;
    let mut editor = super::support::editor_from_document(d);
    let doc = editor.active_id().unwrap();
    editor.set_workspace(WorkspaceKind::Animation)?;
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(doc, motion.clone())?;
    let target = ObjectRef::new(ObjectKind::Motion, &motion.clip_id);
    let plan = editor.document().unwrap().plan_tree_deletion(
        zipzi_editor_core::PaletteProjection::Organization,
        &[target.clone()],
    )?;
    editor.execute_tree_deletion(doc, &plan)?;
    assert_eq!(
        editor.editing_context(),
        Some(zipzi_editor_core::EditingContext::Animation)
    );
    assert!(editor
        .document()
        .unwrap()
        .object_index()
        .get(&target)
        .is_none());
    editor.execute(zipzi_editor_core::EditorCommand::Undo)?;
    assert!(editor
        .document()
        .unwrap()
        .object_index()
        .get(&target)
        .is_some());
    Ok(())
}

#[test]
fn tree_deletion_unlocks_only_a_reported_business_source_and_expires_its_plan() -> anyhow::Result<()>
{
    let assets = tempfile::tempdir()?;
    let asset = id();
    let mut sdk = zipzi_sdk::AuthoringSession::new(&id(), zipzi_core::Canvas::default())?;
    sdk.edit("locked resource", None, |e| {
        e.create_asset(zipzi_core::ImageAsset {
            id: asset.clone(),
            name: "Locked".into(),
            content: None,
            unresolved_source: None,
        })?;
        let mut state = e.candidate_document().editor_state().clone();
        state.objects.insert(
            asset.clone(),
            zipzi_core::document::ObjectEditorState {
                locked: true,
                hidden_in_editor: false,
            },
        );
        e.replace_editor_state(state)
    })?;
    let saved = sdk.save_project(&assets.path().join("locked.zipzi"), None)?;
    let mut d = Document::open(&saved.manifest)?;
    let target = ObjectRef::new(ObjectKind::Texture, &asset);
    let plan = d.plan_tree_deletion(
        zipzi_editor_core::PaletteProjection::Organization,
        &[target.clone()],
    )?;
    assert_eq!(
        plan.summary().state,
        zipzi_sdk::DeletionReviewState::Blocked
    );
    assert!(d
        .unlock_deletion_source(&plan, "unreported-source")
        .is_err());
    assert_eq!(d.history_lengths(), (0, 0));
    d.unlock_deletion_source(&plan, &asset)?;
    assert!(!plan.valid_for(&d));
    assert_eq!(d.history_lengths(), (1, 0));
    let fresh = d.plan_tree_deletion(
        zipzi_editor_core::PaletteProjection::Organization,
        &[target.clone()],
    )?;
    assert_eq!(fresh.summary().state, zipzi_sdk::DeletionReviewState::Ready);
    d.execute_tree_deletion(&fresh)?;
    assert!(d.object_index().get(&target).is_none());
    d.undo()?;
    assert!(d.object_index().get(&target).is_some());
    d.undo()?;
    let restored = d.plan_tree_deletion(
        zipzi_editor_core::PaletteProjection::Organization,
        &[target],
    )?;
    assert_eq!(
        restored.summary().state,
        zipzi_sdk::DeletionReviewState::Blocked
    );
    Ok(())
}
