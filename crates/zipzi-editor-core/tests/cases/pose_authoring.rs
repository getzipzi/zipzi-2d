use zipzi_editor_core::{AnimationDiagnostic, Document, ObjectKind, ObjectRef};
use zipzi_sdk::{PoseCollection, PoseEdit as E, PoseFields};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn run(d: &mut Document, e: E) -> anyhow::Result<zipzi_editor_core::OperationResult> {
    let p = d.plan_pose_authoring(e)?;
    d.execute_pose_authoring(&p)
}
#[test]
fn pose_projects_native_identities_pending_rows_and_history() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let (p, g, e, l) = (id(), id(), id(), id());
    run(
        &mut d,
        E::CreateGroup {
            pose_id: p,
            id: g.clone(),
            name: "Empty".into(),
            before: None,
        },
    )?;
    let r = run(
        &mut d,
        E::CreateEntry {
            group_id: g.clone(),
            id: e.clone(),
            before: None,
        },
    )?;
    assert_eq!(
        r.suggested_selection,
        vec![ObjectRef::new(ObjectKind::PoseEntry, &e)]
    );
    assert!(d.selected_id().is_none());
    run(
        &mut d,
        E::CreateLink {
            entry_id: e.clone(),
            id: l.clone(),
            before: None,
        },
    )?;
    let index = d.object_index();
    for (kind, id) in [
        (ObjectKind::PoseGroup, &g),
        (ObjectKind::PoseEntry, &e),
        (ObjectKind::PoseLink, &l),
    ] {
        assert!(index.get(&ObjectRef::new(kind, id)).is_some());
    }
    assert!(
        index
            .get(&ObjectRef::new(ObjectKind::PoseEntry, &e))
            .unwrap()
            .diagnostic
            .is_some()
    );
    assert!(
        d.animation_catalog().diagnostics.iter().any(
            |v| matches!(v,AnimationDiagnostic::UnconfiguredPoseItem { item_id } if item_id==&e)
        )
    );
    d.undo()?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::PoseLink, &l))
            .is_none()
    );
    d.redo()?;
    let path = tempfile::tempdir()?;
    d.save(path.path())?;
    Ok(())
}
#[test]
fn pose_group_rename_refreshes_payload_without_rebuilding_relations() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let (p, g) = (id(), id());
    run(
        &mut d,
        E::CreateGroup {
            pose_id: p,
            id: g.clone(),
            name: "Before".into(),
            before: None,
        },
    )?;
    let retained = d.object_index();
    let address = d.pose_field_address(&g)?;
    let plan = d.plan_pose_field_patch(
        &address,
        PoseFields::Group {
            name: "After".into(),
        },
    )?;
    d.execute_pose_authoring(&plan)?;
    let current = d.object_index();
    assert!(std::ptr::eq(&current.relations[0], &retained.relations[0]));
    assert_eq!(
        current.name(&ObjectRef::new(ObjectKind::PoseGroup, &g)),
        "After"
    );
    assert_eq!(
        retained.name(&ObjectRef::new(ObjectKind::PoseGroup, &g)),
        "Before"
    );
    d.undo()?;
    assert_eq!(
        d.object_index()
            .name(&ObjectRef::new(ObjectKind::PoseGroup, &g)),
        "Before"
    );
    d.redo()?;
    assert_eq!(
        d.object_index()
            .name(&ObjectRef::new(ObjectKind::PoseGroup, &g)),
        "After"
    );
    Ok(())
}
#[test]
fn pose_structure_updates_do_not_activate_motion_or_expression() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::combined_animation_document()?;
    let group = d.pose_definition().unwrap().groups[0].clone();
    let e = group.entries[0].id.clone();
    let address = d.pose_field_address(&e)?;
    let h = id();
    let pose_id = d.pose_definition().unwrap().id.clone();
    run(
        &mut d,
        E::CreateGroup {
            pose_id,
            id: h.clone(),
            name: "New".into(),
            before: None,
        },
    )?;
    run(
        &mut d,
        E::Move {
            ids: vec![e.clone()],
            destination: PoseCollection::Entries {
                group_id: h.clone(),
            },
            before: None,
        },
    )?;
    assert!(
        d.plan_pose_field_patch(&address, address.expected().clone())
            .is_err()
    );
    let mut editor = super::support::editor_from_document(d);
    let doc = editor.active_id().unwrap();
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Animation)?;
    let plan = editor.plan_pose_authoring(
        doc,
        E::CreateEntry {
            group_id: h,
            id: id(),
            before: None,
        },
    )?;
    editor.execute_pose_authoring(doc, &plan)?;
    assert!(editor.animation().unwrap().motion.is_none());
    assert!(editor.animation().unwrap().expression.is_none());
    Ok(())
}
