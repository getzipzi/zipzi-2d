use crate::support;
use editor_core::{
    BusinessAction as A, Document, ObjectKind as K, ObjectRef as R, PropertyPatch,
    PropertyValue as V,
};
fn execute(d: &mut Document, a: A) -> anyhow::Result<editor_core::OperationResult> {
    let p = d.plan_business(&a, "Group".into())?;
    d.execute_business(&p)
}
fn create(d: &mut Document, kind: K, at: Option<R>) -> anyhow::Result<R> {
    Ok(execute(d, A::Create(kind, at))?
        .suggested_selection
        .into_iter()
        .find(|o| o.kind == kind)
        .unwrap())
}
fn patch(d: &mut Document, o: &R, key: editor_core::PropertyKey, value: V) -> anyhow::Result<()> {
    d.set_object_properties(
        o,
        &PropertyPatch {
            scope: d.property_scope(o),
            expected: d.property_values(o)?,
            changes: [(key, value)].into(),
        },
    )
}
#[test]
fn pose_creation_owns_defaults_singleton_empty_groups_and_stable_member_positions()
-> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let group = create(&mut d, K::PoseGroup, None)?;
    let root = d.pose_definition().unwrap().id.clone();
    assert!(d.pose_definition().unwrap().groups[0].entries.is_empty());
    let a = create(&mut d, K::PoseEntry, Some(group.clone()))?;
    let b = create(&mut d, K::PoseEntry, Some(group.clone()))?;
    let c = create(&mut d, K::PoseEntry, Some(a.clone()))?;
    let l = create(&mut d, K::PoseLink, Some(c.clone()))?;
    assert_eq!(
        d.pose_definition().unwrap().groups[0]
            .entries
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec![a.id.as_str(), c.id.as_str(), b.id.as_str()]
    );
    assert_eq!(
        d.pose_definition().unwrap().groups[0].entries[1]
            .links
            .as_ref()
            .unwrap()[0]
            .id,
        l.id
    );
    assert_eq!(d.history_lengths(), (5, 0));
    d.undo()?;
    d.redo()?;
    assert!(d.object_index().get(&l).is_some());
    create(&mut d, K::PoseGroup, Some(group))?;
    assert_eq!(d.pose_definition().unwrap().id, root);
    Ok(())
}
#[test]
fn pose_fields_preserve_reference_states_and_reject_stale_fields_after_undo() -> anyhow::Result<()>
{
    use editor_core::property::business as f;
    let (mut d, _assets) = support::combined_animation_document()?;
    let group = R::new(K::PoseGroup, &d.pose_definition().unwrap().groups[0].id);
    let entry = create(&mut d, K::PoseEntry, Some(group.clone()))?;
    patch(
        &mut d,
        &entry,
        f::POSE_PART.key,
        V::Text("runtime:MissingPart".into()),
    )?;
    assert_eq!(
        d.property_values(&entry)?[&f::POSE_PART.key],
        V::Text("runtime:MissingPart".into())
    );
    patch(&mut d, &entry, f::POSE_PART.key, V::Text(String::new()))?;
    let stale = PropertyPatch {
        scope: d.property_scope(&entry),
        expected: d.property_values(&entry)?,
        changes: [(f::POSE_PART.key, V::Text("runtime:old".into()))].into(),
    };
    create(&mut d, K::PoseEntry, Some(group))?;
    d.undo()?;
    let history = d.history_lengths();
    assert!(d.set_object_properties(&entry, &stale).is_err());
    assert_eq!(d.history_lengths(), history);
    let root = R::new(K::Pose, &d.pose_definition().unwrap().id);
    patch(&mut d, &root, f::POSE_FADE.key, V::OptionalFloat(Some(0.6)))?;
    assert_eq!(d.pose_definition().unwrap().fade_in, Some(0.6));
    patch(&mut d, &root, f::POSE_FADE.key, V::OptionalFloat(None))?;
    assert_eq!(d.pose_definition().unwrap().fade_in, None);
    Ok(())
}
#[test]
fn pose_migration_review_freezes_actual_removal_and_keeps_members_links_and_parts()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::combined_animation_document()?;
    let entry = R::new(
        K::PoseEntry,
        &d.pose_definition().unwrap().groups[0].entries[0].id,
    );
    create(&mut d, K::PoseLink, Some(entry))?;
    let old = d.pose_definition().unwrap().groups[0].clone();
    let from = R::new(K::PoseGroup, &old.id);
    let destination = create(&mut d, K::PoseGroup, None)?;
    let plan = d.plan_business(
        &A::MigratePoseGroup(from.clone(), destination.id.clone()),
        String::new(),
    )?;
    assert_eq!(plan.removed, vec![from.clone()]);
    assert_eq!(plan.moved.len(), old.entries.len());
    assert!(d.object_index().get(&from).is_some());
    let history = d.history_lengths();
    d.execute_business(&plan)?;
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    assert_eq!(d.pose_definition().unwrap().groups[0].entries, old.entries);
    d.undo()?;
    assert_eq!(d.pose_definition().unwrap().groups[0], old);
    d.redo()?;
    for e in old.entries {
        assert!(d.object_index().get(&R::new(K::PoseEntry, &e.id)).is_some());
        for l in e.links.iter().flatten() {
            assert!(d.object_index().get(&R::new(K::PoseLink, &l.id)).is_some());
        }
    }
    Ok(())
}
#[test]
fn group_duplication_rebuilds_owned_identity_and_deletion_reports_subtree() -> anyhow::Result<()> {
    let (mut d, _assets) = support::combined_animation_document()?;
    let entry = R::new(
        K::PoseEntry,
        &d.pose_definition().unwrap().groups[0].entries[0].id,
    );
    create(&mut d, K::PoseLink, Some(entry))?;
    let group = d.pose_definition().unwrap().groups[0].clone();
    let r = execute(&mut d, A::Copy(R::new(K::PoseGroup, &group.id)))?;
    let copy = r
        .suggested_selection
        .into_iter()
        .find(|o| o.kind == K::PoseGroup)
        .unwrap();
    let actual = &d.pose_definition().unwrap().groups[1];
    assert_ne!(actual.entries[0].id, group.entries[0].id);
    assert_eq!(actual.entries[0].part, group.entries[0].part);
    let plan = d.plan_business(&A::Delete(copy), String::new())?;
    assert!(plan.removed.iter().any(|o| o.kind == K::PoseEntry));
    assert!(plan.removed.iter().any(|o| o.kind == K::PoseLink));
    let history = d.history_lengths();
    d.execute_business(&plan)?;
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    assert_eq!(d.pose_definition().unwrap().groups[0], group);
    Ok(())
}
#[test]
fn physics_output_moves_require_explicit_receiving_node_and_keep_target_identity()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::combined_animation_document()?;
    let old = d.physics_definition().unwrap().data.settings[0].clone();
    let output = R::new(K::PhysicsOutput, &old.outputs[0].id);
    let destination = create(&mut d, K::PhysicsGroup, None)?;
    let destination_node = d.physics_definition().unwrap().data.settings[1].vertices[1]
        .id
        .clone();
    assert!(
        d.plan_business(
            &A::Move(output.clone(), destination.id.clone(), None),
            String::new()
        )
        .is_err()
    );
    let history = d.history_lengths();
    let plan = d.plan_business(
        &A::Move(
            output.clone(),
            destination.id.clone(),
            Some(destination_node.clone()),
        ),
        String::new(),
    )?;
    d.execute_business(&plan)?;
    let moved = &d.physics_definition().unwrap().data.settings[1].outputs[0];
    assert_eq!(moved.id, output.id);
    assert_eq!(moved.destination, old.outputs[0].destination);
    assert_eq!(moved.node_id, destination_node);
    assert_eq!(d.history_lengths(), (history.0 + 1, 0));
    d.undo()?;
    assert_eq!(d.physics_definition().unwrap().data.settings[0], old);
    Ok(())
}
#[test]
fn physics_root_fps_is_editable_without_groups_and_group_actions_preserve_singleton()
-> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let group = create(&mut d, K::PhysicsGroup, None)?;
    let root = R::new(K::Physics, &d.physics_definition().unwrap().id);
    execute(&mut d, A::Delete(group))?;
    assert!(d.physics_definition().unwrap().data.settings.is_empty());
    patch(
        &mut d,
        &root,
        editor_core::property::business::PHYSICS_FPS.key,
        V::OptionalFloat(None),
    )?;
    assert_eq!(d.physics_definition().unwrap().data.meta.fps, None);
    patch(
        &mut d,
        &root,
        editor_core::property::business::PHYSICS_FPS.key,
        V::OptionalFloat(Some(45.)),
    )?;
    assert_eq!(d.physics_definition().unwrap().data.meta.fps, Some(45.));
    create(&mut d, K::PhysicsGroup, None)?;
    assert_eq!(d.physics_definition().unwrap().id, root.id);
    Ok(())
}
#[test]
fn no_op_steps_preserve_redo_and_stale_business_plans_cannot_write() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let group = create(&mut d, K::PoseGroup, None)?;
    let plan = d.plan_business(&A::Delete(group.clone()), String::new())?;
    create(&mut d, K::PoseGroup, None)?;
    d.undo()?;
    let history = d.history_lengths();
    assert!(d.execute_business(&plan).is_err());
    let boundary = d.plan_business(&A::Step(group, false), String::new())?;
    assert!(boundary.noop);
    assert!(!d.execute_business(&boundary)?.receipt.changed);
    assert_eq!(d.history_lengths(), history);
    Ok(())
}

#[test]
fn physics_chain_topology_edits_keep_output_node_references_and_root_constraints()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::combined_animation_document()?;
    let rig = d.physics_definition().unwrap().data.settings[0].clone();
    let source = R::new(K::PhysicsNode, &rig.vertices[1].id);
    let result = execute(&mut d, A::Copy(source.clone()))?;
    let copy = result
        .suggested_selection
        .iter()
        .find(|o| o.kind == K::PhysicsNode)
        .unwrap()
        .clone();
    assert_eq!(
        d.physics_definition().unwrap().data.settings[0].vertices[2].id,
        copy.id
    );
    execute(&mut d, A::Step(copy.clone(), false))?;
    let after = &d.physics_definition().unwrap().data.settings[0];
    assert_eq!(after.vertices[1].id, copy.id);
    assert_eq!(after.outputs, rig.outputs);
    let root = R::new(K::PhysicsNode, &rig.vertices[0].id);
    assert!(matches!(
        d.business_capability(&A::Step(root, false))?,
        kasane_sdk::CapabilityState::Unsupported { .. }
    ));
    d.undo()?;
    d.undo()?;
    assert_eq!(d.physics_definition().unwrap().data.settings[0], rig);
    Ok(())
}
#[test]
fn physics_execution_order_rebuilds_manual_trial_without_changing_selection() -> anyhow::Result<()>
{
    let (d, _assets) = support::combined_animation_document()?;
    let mut editor = support::editor_from_document(d);
    editor.set_workspace(editor_core::WorkspaceKind::Animation)?;
    let document = editor.active_id().unwrap();
    let plan = editor
        .document()
        .unwrap()
        .plan_business(&A::Create(K::PhysicsGroup, None), "Second".into())?;
    let result = editor.execute_business(document, &plan)?;
    let source = result
        .suggested_selection
        .into_iter()
        .find(|o| o.kind == K::PhysicsGroup)
        .unwrap();
    editor.inspect_authoring_object(document, source.clone())?;
    editor.set_physics_manual(document, true)?;
    let generation = editor.animation().unwrap().preview_generation;
    let plan = editor
        .document()
        .unwrap()
        .plan_business(&A::Step(source.clone(), false), String::new())?;
    editor.execute_business(document, &plan)?;
    assert_eq!(
        editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .settings[0]
            .id,
        source.id
    );
    assert_eq!(editor.active_inspection().unwrap().object(), Some(source));
    assert!(editor.animation().unwrap().physics_preview.manual);
    assert!(editor.animation().unwrap().preview_generation > generation);
    assert!(editor.animation().unwrap().motion.is_none());
    Ok(())
}

#[test]
fn removing_physics_singleton_retires_manual_trial_and_undo_does_not_restart_it()
-> anyhow::Result<()> {
    let (d, _assets) = support::combined_animation_document()?;
    let mut editor = support::editor_from_document(d);
    editor.set_workspace(editor_core::WorkspaceKind::Animation)?;
    let document = editor.active_id().unwrap();
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(document, motion.clone())?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.4);
    let root = R::new(
        K::Physics,
        &editor.document().unwrap().physics_definition().unwrap().id,
    );
    editor.set_physics_manual(document, true)?;
    let plan = editor
        .document()
        .unwrap()
        .plan_business(&A::Delete(root.clone()), String::new())?;
    assert!(plan.removed.len() > 1);
    editor.execute_business(document, &plan)?;
    assert!(!editor.animation().unwrap().physics_preview.manual);
    assert!(!editor.animation().unwrap().playing);
    assert_eq!(editor.animation().unwrap().time, 0.4);
    assert_eq!(editor.animation().unwrap().motion.as_ref(), Some(&motion));
    editor.execute(editor_core::EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().physics_definition().unwrap().id,
        root.id
    );
    assert!(!editor.animation().unwrap().physics_preview.manual);
    assert!(!editor.animation().unwrap().playing);
    Ok(())
}
