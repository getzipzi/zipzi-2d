use crate::support;
use zipzi_core::document::{MotionContainer as C, MotionEdit as E, MotionEvent};
use zipzi_editor_core::{
    AnimationRemoval as D, Document, MotionCopyKind as Copy, ObjectKind as K, ObjectRef as R,
};
fn apply(d: &mut Document, intent: E) -> anyhow::Result<()> {
    let plan = d.plan_motion_authoring(intent)?;
    d.execute_motion_authoring(&plan)?;
    Ok(())
}
#[test]
fn content_copy_regenerates_owned_identity_and_keeps_external_references() -> anyhow::Result<()> {
    let (mut d, _assets) = support::animation_document()?;
    let group = d.motion_groups()[0].clone();
    let source = R::new(K::MotionRegistration, &group.entries[0].id);
    let clip_id = group.entries[0].clip_id.clone();
    apply(
        &mut d,
        E::SetEvent {
            clip: clip_id.clone(),
            event: MotionEvent {
                id: uuid::Uuid::new_v4().to_string(),
                time: 0.5,
                value: "Event".into(),
                extensions: Default::default(),
            },
        },
    )?;
    let plan = d.plan_motion_copy(&source, Copy::Content, "Copy".into())?;
    let before = d.history_lengths();
    let index = d.object_index();
    let result = d.execute_motion_copy(&plan)?;
    let copy = d.motion_definition(&plan.selection.id).unwrap();
    let original = d.motion_definition(&clip_id).unwrap();
    assert_ne!(copy.id, original.id);
    assert_eq!(copy.tracks[0].target, original.tracks[0].target);
    assert_ne!(copy.tracks[0].id, original.tracks[0].id);
    assert!(
        copy.tracks[0]
            .keys()
            .zip(original.tracks[0].keys())
            .all(|(a, b)| a.id != b.id && a.time == b.time && a.value == b.value)
    );
    assert_ne!(copy.events[0].id, original.events[0].id);
    assert_eq!(copy.events[0].value, "Event");
    assert_eq!(d.motion_groups()[0].entries[0].id, source.id);
    assert_eq!(d.motion_groups()[0].entries[1].clip_id, copy.id);
    assert_eq!(d.motion_groups()[0].entries[2].id, group.entries[1].id);
    assert_eq!(d.history_lengths(), (before.0 + 1, 0));
    let created = result.receipt.changes.created.clone();
    let registration = d.motion_groups()[0].entries[1].clone();
    d.undo()?;
    assert!(d.motion_definition(&plan.selection.id).is_none());
    assert_eq!(d.motion_groups()[0], group);
    d.redo()?;
    assert_eq!(d.motion_groups()[0].entries[1], registration);
    for object in created {
        assert!(
            d.object_index()
                .get(&R::new(K::from_sdk(object.kind), object.id))
                .is_some()
        );
    }
    assert_eq!(
        index
            .objects
            .iter()
            .filter(|o| o.reference.kind == K::Parameter)
            .count(),
        d.object_index()
            .objects
            .iter()
            .filter(|o| o.reference.kind == K::Parameter)
            .count()
    );
    Ok(())
}
#[test]
fn group_copy_keeps_shared_clip_and_overrides_but_rebuilds_registration_ids() -> anyhow::Result<()>
{
    let (mut d, _assets) = support::animation_document()?;
    let group = d.motion_groups()[0].clone();
    let source = R::new(K::MotionGroup, &group.id);
    let plan = d.plan_motion_copy(&source, Copy::Group, "Copy".into())?;
    assert_eq!(d.history_lengths(), (0, 0));
    d.execute_motion_copy(&plan)?;
    let copy = d.motion_groups()[1].clone();
    assert_eq!(copy.id, plan.selection.id);
    assert_eq!(
        d.motion_resources()
            .iter()
            .map(|r| &r.selection.clip_id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2
    );
    for (a, b) in copy.entries.iter().zip(&group.entries) {
        assert_ne!(a.id, b.id);
        assert_eq!(a.clip_id, b.clip_id);
        assert_eq!(a.fade_in, b.fade_in);
        assert_eq!(a.sound, b.sound);
    }
    assert_eq!(d.history_lengths(), (1, 0));
    d.undo()?;
    d.redo()?;
    assert_eq!(d.motion_groups()[1], copy);
    Ok(())
}
#[test]
fn references_inherit_defaults_use_sibling_position_and_respect_locked_destination()
-> anyhow::Result<()> {
    let (mut d, assets) = support::animation_document()?;
    let group = d.motion_groups()[0].clone();
    let at = R::new(K::MotionRegistration, &group.entries[0].id);
    let plan = d.plan_motion_reference(&group.entries[0].clip_id, Some(&at))?;
    d.execute_motion_copy(&plan)?;
    let entry = &d.motion_groups()[0].entries[1];
    assert_eq!(entry.id, plan.selection.id);
    assert_eq!(entry.clip_id, group.entries[0].clip_id);
    assert_eq!(
        (entry.fade_in, entry.fade_out, entry.sound.as_ref()),
        (None, None, None)
    );
    assert_eq!(d.motion_groups()[0].entries[2], group.entries[1]);
    assert_eq!(d.history_lengths(), (1, 0));
    let path = d.save(&assets.path().join("locked.zipzi"))?.manifest;
    let mut session = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(256., 256., zipzi_core::Vec2::default(), 1.),
    )?;
    session.open_project(&path, None)?;
    let mut state = session.document().editor_state().clone();
    state.objects.entry(group.id.clone()).or_default().locked = true;
    session.edit("Lock group", None, |edit| edit.replace_editor_state(state))?;
    session.save_project(&path, None)?;
    let d = Document::open(&path)?;
    assert!(
        d.plan_motion_reference(&group.entries[0].clip_id, Some(&at))
            .is_err()
    );
    assert_eq!(
        d.motion_copy_capability(&at, Copy::Content)?,
        zipzi_sdk::CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    );
    assert!(
        d.plan_motion_copy(&at, Copy::Content, "Copy".into())
            .is_err()
    );
    Ok(())
}
#[test]
fn deleting_group_reports_actual_last_registration_migration_and_undo_restores_overrides()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::animation_document()?;
    let group = d.motion_groups()[0].clone();
    let source = R::new(K::MotionGroup, &group.id);
    let plan = d.plan_animation_deletion(&source, D::Group)?;
    assert!(plan.requires_review());
    assert!(!plan.is_noop());
    assert!(plan.removed.contains(&source));
    assert_eq!(
        plan.removed
            .iter()
            .filter(|o| o.kind == K::MotionRegistration)
            .count(),
        1
    );
    assert_eq!(plan.migrated_to_root.len(), 1);
    assert!(
        plan.preserved
            .contains(&R::new(K::Motion, &group.entries[0].clip_id))
    );
    assert!(plan.blocked_by.is_empty());
    assert_eq!(d.history_lengths(), (0, 0));
    assert_eq!(d.motion_groups()[0], group);
    d.execute_animation_deletion(&plan)?;
    assert!(d.motion_groups().is_empty());
    let (container, registration) = d.motion_registration(&plan.migrated_to_root[0].id).unwrap();
    assert_eq!(container, C::Root);
    assert_eq!(
        &registration,
        group
            .entries
            .iter()
            .find(|e| e.id == registration.id)
            .unwrap()
    );
    assert!(d.motion_definition(&group.entries[0].clip_id).is_some());
    assert_eq!(d.history_lengths(), (1, 0));
    d.undo()?;
    assert_eq!(d.motion_groups()[0], group);
    d.redo()?;
    assert!(
        d.motion_registration(&plan.migrated_to_root[0].id)
            .is_some()
    );
    Ok(())
}
#[test]
fn dissolve_keeps_every_registration_and_last_root_removal_is_protected_noop() -> anyhow::Result<()>
{
    let (mut d, _assets) = support::animation_document()?;
    let group = d.motion_groups()[0].clone();
    let source = R::new(K::MotionGroup, &group.id);
    let plan = d.plan_animation_deletion(&source, D::DissolveGroup)?;
    assert_eq!(plan.removed, vec![source]);
    assert_eq!(plan.migrated_to_root.len(), 2);
    d.execute_animation_deletion(&plan)?;
    for entry in &group.entries {
        let (owner, actual) = d.motion_registration(&entry.id).unwrap();
        assert_eq!(owner, C::Root);
        assert_eq!(&actual, entry);
    }
    let root = d
        .motion_resources()
        .into_iter()
        .find(|r| r.name == "Loop")
        .unwrap()
        .selection
        .entry
        .unwrap();
    let plan = d.plan_animation_deletion(&R::new(K::MotionRegistration, &root), D::Registration)?;
    assert!(plan.is_noop());
    assert!(plan.requires_review());
    assert!(plan.removed.is_empty());
    let before = d.history_lengths();
    d.execute_animation_deletion(&plan)?;
    assert_eq!(d.history_lengths(), before);
    d.undo()?;
    let plan = d.plan_animation_deletion(&R::new(K::MotionRegistration, &root), D::Registration)?;
    let before = d.history_lengths();
    d.execute_animation_deletion(&plan)?;
    assert_eq!(d.history_lengths(), before);
    Ok(())
}
#[test]
fn content_and_expression_deletion_report_owned_children_and_retain_parameters()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::animation_document()?;
    let group = d.motion_groups()[0].clone();
    let clip = R::new(K::Motion, &group.entries[0].clip_id);
    let plan = d.plan_animation_deletion(&clip, D::Motion)?;
    assert!(plan.removed.contains(&clip));
    assert_eq!(
        plan.removed
            .iter()
            .filter(|o| o.kind == K::MotionRegistration)
            .count(),
        2
    );
    assert_eq!(
        plan.removed
            .iter()
            .filter(|o| o.kind == K::MotionTrack)
            .count(),
        1
    );
    assert_eq!(
        plan.removed
            .iter()
            .filter(|o| o.kind == K::MotionKey)
            .count(),
        2
    );
    assert!(
        plan.retained_resources
            .iter()
            .any(|o| o.kind == K::Parameter)
    );
    assert!(
        plan.retained_resources
            .iter()
            .any(|o| o.kind == K::Unresolved)
    );
    assert!(plan.blocked_by.is_empty());
    d.execute_animation_deletion(&plan)?;
    for object in &plan.removed {
        assert!(d.object_index().get(object).is_none());
    }
    for object in plan
        .retained_resources
        .iter()
        .filter(|o| o.kind != K::Unresolved)
    {
        assert!(d.object_index().get(object).is_some());
    }
    assert!(d.motion_groups()[0].entries.is_empty());
    d.undo()?;
    assert_eq!(d.motion_groups()[0], group);
    let expression = R::new(K::Expression, &d.expression_resources()[0].id);
    let plan = d.plan_animation_deletion(&expression, D::Expression)?;
    assert_eq!(plan.removed.len(), 2);
    assert!(plan.removed.iter().any(|o| o.kind == K::ExpressionEntry));
    assert!(
        plan.retained_resources
            .iter()
            .any(|o| o.kind == K::Parameter)
    );
    d.execute_animation_deletion(&plan)?;
    assert!(d.expression_definition(&expression.id).is_none());
    Ok(())
}
#[test]
fn frozen_copy_and_deletion_plans_reject_history_changes_without_consuming_redo()
-> anyhow::Result<()> {
    let (mut d, _assets) = support::animation_document()?;
    let source = R::new(K::MotionGroup, &d.motion_groups()[0].id);
    let copy = d.plan_motion_copy(&source, Copy::Group, "Copy".into())?;
    let deletion = d.plan_animation_deletion(&source, D::Group)?;
    apply(
        &mut d,
        E::RenameGroup {
            id: source.id.clone(),
            name: "Changed".into(),
        },
    )?;
    d.undo()?;
    assert_eq!(d.history_lengths(), (0, 1));
    assert!(d.execute_motion_copy(&copy).is_err());
    assert!(d.execute_animation_deletion(&deletion).is_err());
    assert_eq!(d.history_lengths(), (0, 1));
    Ok(())
}

#[test]
fn content_copy_keeps_current_playback_and_history_never_activates_the_copy() -> anyhow::Result<()>
{
    let (d, _assets) = support::animation_document()?;
    let mut editor = support::editor_from_document(d);
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Animation)?;
    let document = editor.active_id().unwrap();
    let selection = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(document, selection.clone())?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.4);
    let generation = editor.animation().unwrap().preview_generation;
    let plan = editor.document().unwrap().plan_motion_copy(
        &R::new(K::MotionRegistration, selection.entry.as_ref().unwrap()),
        Copy::Content,
        "Copy".into(),
    )?;
    let copy = plan.selection.clone();
    editor.execute_motion_copy(document, &plan)?;
    let a = editor.animation().unwrap();
    assert!(a.playing);
    assert_eq!(a.time, 0.4);
    assert_eq!(a.motion.as_ref(), Some(&selection));
    assert_eq!(a.preview_generation, generation);
    let copied_registration = editor.document().unwrap().motion_groups()[0].entries[1].clone();
    editor.execute(zipzi_editor_core::EditorCommand::Undo)?;
    assert!(
        editor
            .document()
            .unwrap()
            .motion_definition(&copy.id)
            .is_none()
    );
    editor.execute(zipzi_editor_core::EditorCommand::Redo)?;
    assert_eq!(
        editor.document().unwrap().motion_groups()[0].entries[1],
        copied_registration
    );
    assert_eq!(
        editor.animation().unwrap().motion.as_ref(),
        Some(&selection)
    );
    assert_eq!(editor.animation().unwrap().time, 0.4);
    // Keep ordinary history and active-content invalidation policies.
    if !editor.animation().unwrap().playing {
        editor.toggle_animation()?;
    }
    let plan = editor.plan_motion_authoring(
        document,
        E::SetEvent {
            clip: selection.clip_id,
            event: MotionEvent {
                id: uuid::Uuid::new_v4().to_string(),
                time: 0.5,
                value: "Changed".into(),
                extensions: Default::default(),
            },
        },
    )?;
    editor.execute_motion_authoring(document, &plan)?;
    assert!(!editor.animation().unwrap().playing);
    assert!(editor.animation().unwrap().preview_generation > generation);
    Ok(())
}
