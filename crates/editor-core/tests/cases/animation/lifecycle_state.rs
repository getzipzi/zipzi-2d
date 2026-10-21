use super::common::*;
use editor_core::*;

#[test]
fn changed_resources_pause_revalidate_and_rebuild_expression_recipe() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    expression(&mut editor, 0)?;
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(1.);
    let before = editor.animation_request().unwrap();
    let expression_id = editor.animation().unwrap().expression.clone().unwrap();
    editor.set_workspace(WorkspaceKind::Modeling)?;
    editor.execute(EditorCommand::SetObjectProperty {
        object: ObjectRef::new(ObjectKind::Expression, expression_id.clone()),
        field: "fade_in".into(),
        value: serde_json::json!(0.7),
    })?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    let changed = editor.animation_request().unwrap();
    assert!(changed.identity.model_generation > before.identity.model_generation);
    assert_eq!(
        editor.animation().unwrap().expression.as_ref(),
        Some(&expression_id)
    );
    assert_eq!(editor.animation().unwrap().expressions.len(), 1);
    assert_eq!(editor.animation().unwrap().expressions[0].time, 0.);
    assert!(!editor.animation().unwrap().playing);
    editor.dispatch(editor.command(CommandId::Undo))?;
    editor.select_expression(editor.active_id().unwrap(), expression_id)?;
    assert_eq!(editor.animation().unwrap().expressions[0].time, 0.);
    assert!(!editor.animation().unwrap().playing);
    let restored = editor.animation_request().unwrap();
    assert!(restored.identity.model_generation > changed.identity.model_generation);
    assert!(!editor.accepts_animation(changed.identity));
    let mut evaluator = AnimationEvaluator::default();
    let frame = evaluator.evaluate(&restored, |_, _| true)?;
    assert!(editor.present_animation(frame.identity, frame.snapshot));
    Ok(())
}

#[test]
fn model_changes_preserve_the_absolute_loop_time_and_expression_schedule() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 2)?;
    expression(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(3.5);
    expression(&mut editor, 2)?;
    editor.advance_animation_clock(4.25);
    let schedule = editor.animation().unwrap().expressions.clone();
    let expression_id = editor.animation().unwrap().expression.clone().unwrap();

    editor.set_workspace(WorkspaceKind::Modeling)?;
    editor.execute(EditorCommand::SetObjectProperty {
        object: ObjectRef::new(ObjectKind::Expression, expression_id),
        field: "fade_in".into(),
        value: serde_json::json!(0.7),
    })?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    assert_eq!(editor.animation().unwrap().time, 4.25);
    assert_eq!(editor.animation().unwrap().expressions, schedule);
    assert!(!editor.animation().unwrap().playing);
    let evaluated = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    assert_eq!(evaluated.snapshot.time, 4.25);
    assert!(
        evaluated
            .snapshot
            .active_expressions
            .contains(editor.animation().unwrap().expression.as_ref().unwrap())
    );
    Ok(())
}

#[test]
fn detached_runtime_uses_defaults_restores_hidden_objects_and_preserves_authoring_state()
-> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    editor.set_workspace(WorkspaceKind::Modeling)?;
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    let mesh = editor.document().unwrap().mesh_tree()[0].children[0]
        .id
        .clone();
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.clone(),
        value: 0.8,
    })?;
    editor.execute(EditorCommand::SetObjectVisible {
        object: ObjectRef::new(ObjectKind::Mesh, mesh),
        visible: false,
    })?;
    let history = editor.document().unwrap().history_lengths();
    let modified = editor.document().unwrap().modified();
    let mut evaluator = AnimationEvaluator::default();
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    let baseline = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    assert_eq!(baseline.snapshot.parameters[&parameter], 0.);
    assert!(
        baseline
            .scene
            .viewport_frame
            .drawables
            .iter()
            .all(|d| d.visible)
    );
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(1.);
    let frame = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    assert!((frame.snapshot.parameters[&parameter] - 59. / 120.).abs() < 1e-6);
    assert!(editor.present_animation(frame.identity, frame.snapshot));
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert_eq!(editor.document().unwrap().parameters()[0].value, 0.8);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert_eq!(editor.document().unwrap().modified(), modified);
    assert!(!editor.preview()?.scene.viewport_frame.drawables[0].visible);
    Ok(())
}

#[test]
fn registrations_keep_distinct_fades_and_unregistered_clips_remain_playable() -> anyhow::Result<()>
{
    let (mut editor, _assets) = setup()?;
    let resources = editor.document().unwrap().motion_resources();
    assert_eq!(resources.len(), 3);
    assert_eq!(
        resources[0].selection.clip_id,
        resources[1].selection.clip_id
    );
    assert_ne!(resources[0].selection.entry, resources[1].selection.entry);
    assert!(resources[2].selection.entry.is_some());
    assert!(resources[2].group.is_none());
    let mut evaluator = AnimationEvaluator::default();
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.5);
    let first = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    select(&mut editor, 1)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.5);
    let second = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    assert!(first.snapshot.parameters[&parameter] > second.snapshot.parameters[&parameter]);
    for duration in [0., -1., f32::NAN, f32::INFINITY, 20_000.] {
        let mut invalid = resources[0].clone();
        invalid.duration = duration;
        assert!(!invalid.playable());
    }
    Ok(())
}

#[test]
fn transport_finishes_once_replays_and_keeps_loop_clock_absolute() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    assert!(!editor.command_available(editor.command(CommandId::PlayPause)));
    select(&mut editor, 0)?;
    editor.dispatch(editor.command(CommandId::PlayPause))?;
    editor.advance_animation_clock(3.);
    assert_eq!(editor.animation().unwrap().time, 2.);
    assert!(!editor.animation().unwrap().playing);
    editor.dispatch(editor.command(CommandId::PlayPause))?;
    assert_eq!(editor.animation().unwrap().time, 0.);
    editor.advance_animation_clock(0.5);
    editor.dispatch(editor.command(CommandId::PlayPause))?;
    editor.advance_animation_clock(100.);
    assert_eq!(editor.animation().unwrap().time, 0.5);
    editor.dispatch(editor.command(CommandId::RewindAnimation))?;
    assert_eq!(editor.animation().unwrap().time, 0.);
    select(&mut editor, 2)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(3.5);
    let mut evaluator = AnimationEvaluator::default();
    let frame = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    assert_eq!(frame.snapshot.time, 3.5);
    assert!(editor.animation().unwrap().playing);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn recipe_changes_keep_the_displayed_image_until_a_replacement_is_presented() -> anyhow::Result<()>
{
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 2)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.5);
    let mut evaluator = AnimationEvaluator::default();
    let shown = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(shown.identity, shown.snapshot.clone()));

    expression(&mut editor, 0)?;
    assert!(editor.animation().unwrap().playing);
    assert!(editor.can_display_animation_image(shown.identity));
    assert!(!editor.accepts_animation(shown.identity));
    let obsolete = evaluate(&mut editor, &mut evaluator)?;
    expression(&mut editor, 2)?;
    select(&mut editor, 0)?;
    assert!(editor.can_display_animation_image(shown.identity));
    assert!(!editor.present_animation(obsolete.identity, obsolete.snapshot));
    assert!(!editor.present_animation(shown.identity, shown.snapshot));

    let replacement = evaluate(&mut editor, &mut evaluator)?;
    assert!(!editor.can_display_animation_image(replacement.identity));
    assert!(editor.can_display_animation_image(shown.identity));
    assert!(editor.present_animation(replacement.identity, replacement.snapshot));
    assert!(editor.can_display_animation_image(replacement.identity));
    assert!(!editor.can_display_animation_image(shown.identity));
    Ok(())
}

#[test]
fn retained_animation_images_cannot_cross_document_workspace_or_model_changes() -> anyhow::Result<()>
{
    for change in 0..3 {
        let (mut editor, _assets) = setup()?;
        if change == 2 {
            editor.set_workspace(WorkspaceKind::Modeling)?;
            let motion = editor.document().unwrap().animation_catalog().motions[2]
                .selection
                .clip_id
                .clone();
            editor.execute(EditorCommand::SetObjectProperty {
                object: ObjectRef::new(ObjectKind::Motion, motion),
                field: "looping".into(),
                value: serde_json::json!(false),
            })?;
            editor.set_workspace(WorkspaceKind::Animation)?;
            editor.set_animation_preview_mode(
                editor.active_id().unwrap(),
                AnimationPreviewMode::Result,
            )?;
        }
        select(&mut editor, 2)?;
        let frame = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
        assert!(editor.present_animation(frame.identity, frame.snapshot));
        assert!(editor.can_display_animation_image(frame.identity));
        match change {
            0 => {
                editor.new_document()?;
            }
            1 => {
                editor.set_workspace(WorkspaceKind::Modeling)?;
            }
            _ => {
                editor.dispatch(editor.command(CommandId::Undo))?;
            }
        }
        assert!(!editor.can_display_animation_image(frame.identity));
    }
    Ok(())
}

#[test]
fn document_workspace_save_and_history_changes_pause_and_invalidate() -> anyhow::Result<()> {
    let (mut editor, assets) = setup()?;
    let first = editor.active_id().unwrap();
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.5);
    let pending = editor.animation_request().unwrap();
    let save = editor.prepare_save(first, &assets.path().join("saved.kasane"), false)?;
    assert!(!editor.accepts_animation(pending.identity));
    editor.finish_save(&mut save.run())?;
    assert!(!editor.animation().unwrap().playing);
    assert_eq!(editor.animation().unwrap().time, 0.5);
    assert!(editor.animation().unwrap().motion.is_some());
    editor.toggle_animation()?;
    editor.new_document()?;
    editor.activate(first)?;
    assert!(!editor.animation().unwrap().playing);
    editor.toggle_animation()?;
    editor.set_workspace(WorkspaceKind::Modeling)?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    assert!(!editor.animation().unwrap().playing);
    assert!(!editor.accepts_animation(pending.identity));
    editor.set_workspace(WorkspaceKind::Modeling)?;
    let mesh = editor.document().unwrap().mesh_tree()[0].children[0]
        .id
        .clone();
    editor.execute(EditorCommand::SetObjectVisible {
        object: ObjectRef::new(ObjectKind::Mesh, mesh),
        visible: false,
    })?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    let before = editor.animation_request().unwrap();
    editor.toggle_animation()?;
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert!(!editor.animation().unwrap().playing);
    let after = editor.animation_request().unwrap();
    assert!(after.identity.model_generation > before.identity.model_generation);
    assert!(!editor.accepts_animation(before.identity));
    editor.dispatch(editor.command(CommandId::Redo))?;
    let redo = editor.animation_request().unwrap();
    assert!(redo.identity.model_generation > after.identity.model_generation);
    editor.close_document(first, true)?;
    assert!(!editor.accepts_animation(redo.identity));
    Ok(())
}

#[test]
fn returning_to_a_paused_document_and_closing_an_active_tab_reissue_the_gpu_target()
-> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let first = editor.active_id().unwrap();
    select(&mut editor, 0)?;
    expression(&mut editor, 0)?;
    editor.seek_animation(first, 0.75)?;
    let mut evaluator = AnimationEvaluator::default();
    let shown = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(shown.identity, shown.snapshot));
    assert!(editor.animation_request().is_none());
    let generation = editor.animation().unwrap().preview_generation;
    let triggers = editor.animation().unwrap().expressions.clone();
    let second = editor.new_document()?;
    editor.activate(first)?;
    let returned = editor
        .animation_request()
        .expect("paused tab needs a replacement GPU image");
    assert_eq!(returned.identity.time, 0.75);
    assert_eq!(returned.identity.preview_generation, generation);
    assert_eq!(returned.expressions, triggers);
    assert!(!editor.accepts_animation(shown.identity));
    let shown = evaluator.evaluate(&returned, |_, _| true)?;
    assert!(editor.present_animation(shown.identity, shown.snapshot));
    editor.activate(second)?;
    editor.close_document(second, true)?;
    assert_eq!(editor.active_id(), Some(first));
    assert!(!editor.accepts_animation(returned.identity));
    assert!(editor.animation_request().is_some());
    assert!(!editor.animation().unwrap().playing);
    assert!(!editor.document().unwrap().modified());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
