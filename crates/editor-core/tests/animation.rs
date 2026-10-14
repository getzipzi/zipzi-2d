use editor_core::*;
#[allow(dead_code)]
mod support;

fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (document, assets) = support::animation_document()?;
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    Ok((editor, assets))
}
fn select(editor: &mut Editor, index: usize) -> anyhow::Result<()> {
    let selection = editor.document().unwrap().motion_resources()[index]
        .selection
        .clone();
    editor.select_motion(editor.active_id().unwrap(), selection)
}

fn expression(editor: &mut Editor, index: usize) -> anyhow::Result<()> {
    let id = editor.document().unwrap().expression_resources()[index]
        .id
        .clone();
    editor.select_expression(editor.active_id().unwrap(), id)
}
fn evaluate(
    editor: &mut Editor,
    evaluator: &mut AnimationEvaluator,
) -> anyhow::Result<AnimationEvaluated> {
    evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)
}

#[test]
fn live_expression_switch_replays_only_nearby_steps_and_matches_cold_replay() -> anyhow::Result<()>
{
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 2)?;
    editor.toggle_animation()?;
    let mut evaluator = AnimationEvaluator::default();
    editor.advance_animation_clock(1.);
    evaluate(&mut editor, &mut evaluator)?;
    editor.advance_animation_clock(60.5);
    evaluate(&mut editor, &mut evaluator)?;
    for index in [0, 2, 1] {
        expression(&mut editor, index)?;
        let request = editor.animation_request().unwrap();
        let mut steps = 0;
        let start = std::time::Instant::now();
        let frame = evaluator.evaluate(&request, |_, total| {
            steps = total;
            true
        })?;
        eprintln!(
            "expression switch: {steps} replay steps, {:?}",
            start.elapsed()
        );
        assert!(steps <= 60, "switch replayed {steps} steps of old history");
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(frame.snapshot, cold.snapshot);
        assert_eq!(frame.scene.runtime_frame, cold.scene.runtime_frame);
        editor.advance_animation_clock(editor.animation().unwrap().time + 0.5);
        evaluate(&mut editor, &mut evaluator)?;
    }
    Ok(())
}

#[test]
fn invalid_replacement_recipe_preserves_the_running_evaluator() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 2)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(5.);
    let mut request = editor.animation_request().unwrap();
    let mut evaluator = AnimationEvaluator::default();
    let before = evaluator.evaluate(&request, |_, _| true)?;
    let stats = evaluator.seek_cache_stats();
    request.identity.model_generation += 1;
    request.expressions.push(ExpressionTrigger {
        expression_id: "missing".into(),
        time: 0.,
        transition: kasane_sdk::ExpressionTransition::Fade,
    });
    assert!(evaluator.evaluate(&request, |_, _| true).is_err());
    assert_eq!(evaluator.seek_cache_stats(), stats);
    request.identity.model_generation -= 1;
    request.expressions.clear();
    let after = evaluator.evaluate(&request, |_, total| {
        assert_eq!(
            total, 0,
            "invalid input must not discard completed playback"
        );
        true
    })?;
    assert_eq!(before.snapshot, after.snapshot);
    Ok(())
}

#[test]
fn expression_only_preview_switches_without_playing_and_clear_preserves_clock() -> anyhow::Result<()>
{
    let (mut editor, _assets) = setup()?;
    let baseline = (
        editor.document().unwrap().history_lengths(),
        editor.document().unwrap().modified(),
    );
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    let mut evaluator = AnimationEvaluator::default();
    expression(&mut editor, 0)?;
    assert!(!editor.animation().unwrap().playing);
    assert!(editor.command_available(editor.command(CommandId::PlayPause)));
    assert_eq!(
        evaluate(&mut editor, &mut evaluator)?.snapshot.parameters[&parameter],
        0.3
    );
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.5);
    let add = evaluate(&mut editor, &mut evaluator)?;
    assert!((add.snapshot.parameters[&parameter] - 0.3).abs() < 1e-6);
    expression(&mut editor, 2)?;
    assert!(editor.animation().unwrap().playing);
    assert_eq!(editor.animation().unwrap().expressions[1].time, 0.5);
    expression(&mut editor, 2)?;
    assert_eq!(editor.animation().unwrap().expressions.len(), 2);
    editor.advance_animation_clock(1.);
    let overwrite = evaluate(&mut editor, &mut evaluator)?;
    assert!((overwrite.snapshot.parameters[&parameter] - 0.8).abs() < 1e-6);
    editor.dispatch(editor.command(CommandId::ClearExpression))?;
    assert!(editor.animation().unwrap().playing);
    assert_eq!(editor.animation().unwrap().time, 1.);
    let clear = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(clear.snapshot.parameters[&parameter], 0.);
    assert!(clear.snapshot.active_expressions.is_empty());
    editor.pause_animation();
    expression(&mut editor, 1)?;
    assert!(!editor.animation().unwrap().playing);
    assert_eq!(editor.animation().unwrap().time, 1.);
    editor.rewind_animation()?;
    assert_eq!(editor.animation().unwrap().expressions[0].time, 1.);
    let rewind = evaluate(&mut editor, &mut evaluator)?;
    assert!(rewind.snapshot.active_expressions.is_empty());
    editor.dispatch(editor.command(CommandId::ResetAnimation))?;
    assert!(editor.animation().unwrap().expression.is_none());
    assert!(editor.animation().unwrap().motion.is_none());
    assert!(editor.animation().unwrap().expressions.is_empty());
    assert_eq!(editor.animation().unwrap().time, 0.);
    assert_eq!(
        (
            editor.document().unwrap().history_lengths(),
            editor.document().unwrap().modified()
        ),
        baseline
    );
    Ok(())
}

#[test]
fn seeks_replay_ordered_expressions_branch_in_the_past_and_clear_never_resurrects()
-> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    select(&mut editor, 0)?;
    expression(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.5);
    expression(&mut editor, 1)?;
    expression(&mut editor, 2)?; // Same-time calls retain their order.
    editor.advance_animation_clock(1.);
    let mut evaluator = AnimationEvaluator::default();
    let combined = evaluate(&mut editor, &mut evaluator)?;
    assert!((combined.snapshot.parameters[&parameter] - 0.8).abs() < 1e-6);
    let recipe = editor.animation().unwrap().expressions.clone();
    for time in [0.25, 1., 0.501, 0., 1.5, 0.9] {
        editor.seek_animation(id, time)?;
        let request = editor.animation_request().unwrap();
        let cached = evaluator.evaluate(&request, |_, _| true)?;
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(cached.snapshot, cold.snapshot, "time={time}");
        assert_eq!(cached.scene.viewport_frame, cold.scene.viewport_frame);
        assert!(editor.present_animation(cached.identity, cached.snapshot));
        assert_eq!(editor.animation().unwrap().expressions, recipe);
        assert!(!editor.animation().unwrap().playing);
    }
    editor.seek_animation(id, 0.25)?;
    expression(&mut editor, 1)?;
    let branched = &editor.animation().unwrap().expressions;
    assert_eq!(branched.len(), 2);
    assert_eq!(branched[1].time, 0.25);
    editor.seek_animation(id, 1.)?;
    let multiply = evaluate(&mut editor, &mut evaluator)?;
    assert!((multiply.snapshot.parameters[&parameter] - 59. / 240.).abs() < 1e-6);
    editor.clear_expression()?;
    for time in [0., 0.5, 1.5] {
        editor.seek_animation(id, time)?;
        assert!(
            evaluate(&mut editor, &mut evaluator)?
                .snapshot
                .active_expressions
                .is_empty()
        );
    }
    assert!(!editor.document().unwrap().modified());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn loop_seek_preserves_expression_schedule_and_rewind_retains_current_run() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    expression(&mut editor, 0)?;
    select(&mut editor, 2)?;
    assert_eq!(editor.animation().unwrap().expressions.len(), 1);
    assert_eq!(editor.animation().unwrap().expressions[0].time, 0.);
    editor.toggle_animation()?;
    editor.advance_animation_clock(3.5);
    expression(&mut editor, 2)?;
    let mut evaluator = AnimationEvaluator::default();
    let old = evaluate(&mut editor, &mut evaluator)?;
    editor.rewind_animation()?;
    assert_eq!(editor.animation().unwrap().expressions.len(), 2);
    editor.seek_animation(id, 0.5)?;
    assert!(!editor.accepts_animation(old.identity));
    assert_eq!(editor.animation().unwrap().time, 0.5);
    assert_eq!(editor.animation().unwrap().expressions.len(), 2);
    assert_eq!(editor.animation().unwrap().expressions[1].time, 3.5);
    assert_eq!(
        editor.animation().unwrap().expressions[1].expression_id,
        editor.animation().unwrap().expression.clone().unwrap()
    );
    editor.seek_animation(id, 100.)?;
    assert_eq!(editor.animation().unwrap().time, 1.);
    assert!(editor.animation().unwrap().loop_position);
    assert!(!editor.animation().unwrap().playing);
    let current = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(current.identity, current.snapshot));
    editor.toggle_animation()?;
    assert!(!editor.animation().unwrap().loop_position);
    select(&mut editor, 0)?;
    assert_eq!(editor.animation().unwrap().expressions[0].time, 0.);
    for time in [-1., f32::NAN, f32::INFINITY] {
        assert!(editor.seek_animation(id, time).is_err());
        assert_eq!(editor.animation().unwrap().time, 0.);
    }
    editor.reset_animation()?;
    assert!(editor.seek_animation(id, 0.5).is_err());
    Ok(())
}

#[test]
fn rapid_seeks_and_recipe_replacements_reject_every_obsolete_frame() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    select(&mut editor, 0)?;
    expression(&mut editor, 0)?;
    let mut evaluator = AnimationEvaluator::default();
    let mut obsolete = Vec::new();
    for time in [1., 0.25, 1.5, 0.8] {
        editor.seek_animation(id, time)?;
        obsolete.push(evaluate(&mut editor, &mut evaluator)?);
    }
    editor.clear_expression()?;
    for frame in obsolete {
        assert!(!editor.present_animation(frame.identity, frame.snapshot));
    }
    editor.seek_animation(id, 0.1)?;
    let latest = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(latest.identity, latest.snapshot));
    assert_eq!(
        editor.animation().unwrap().presented.as_ref().unwrap().time,
        0.1
    );
    Ok(())
}

#[test]
fn combined_physics_pose_expression_replay_matches_continuous_and_cancelled_seek()
-> anyhow::Result<()> {
    let (document, _assets) = support::combined_animation_document()?;
    let physics = document
        .parameters()
        .iter()
        .find(|p| p.name == "Physics output")
        .unwrap()
        .id
        .clone();
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 256.,
        height: 256.,
        scale_factor: 1.,
    });
    let id = editor.active_id().unwrap();
    select(&mut editor, 0)?;
    expression(&mut editor, 0)?;
    let mut evaluator = AnimationEvaluator::default();
    editor.toggle_animation()?;
    for time in [0.25, 0.5, 0.75, 1.] {
        editor.advance_animation_clock(time);
        if time == 0.5 {
            expression(&mut editor, 2)?;
        }
        let request = editor.animation_request().unwrap();
        let continuous = evaluator.evaluate(&request, |_, _| true)?;
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(continuous.snapshot, cold.snapshot);
        assert_eq!(continuous.scene.runtime_frame, cold.scene.runtime_frame);
        assert!(!continuous.snapshot.part_opacities.is_empty());
        if time == 1. {
            assert_ne!(continuous.snapshot.parameters[&physics], 0.);
        }
    }
    for time in [0.1, 0.75, 0.503, 1.5] {
        editor.seek_animation(id, time)?;
        let request = editor.animation_request().unwrap();
        let cached = evaluator.evaluate(&request, |_, _| true)?;
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(cached.snapshot, cold.snapshot);
        assert_eq!(cached.scene.runtime_frame, cold.scene.runtime_frame);
    }
    editor.seek_animation(id, 0.2)?;
    let mut cancelled = editor.animation_request().unwrap();
    cancelled.identity.time = 100.;
    assert!(evaluator.evaluate(&cancelled, |step, _| step < 20).is_err());
    editor.seek_animation(id, 0.3)?;
    let request = editor.animation_request().unwrap();
    assert_eq!(
        evaluator.evaluate(&request, |_, _| true)?.snapshot,
        AnimationEvaluator::default()
            .evaluate(&request, |_, _| true)?
            .snapshot
    );
    editor.clear_expression()?;
    let no_expression = evaluate(&mut editor, &mut evaluator)?;
    assert!(no_expression.snapshot.active_expressions.is_empty());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

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
fn selecting_a_future_expression_after_seeking_back_branches_at_the_current_time()
-> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let id = editor.active_id().unwrap();
    select(&mut editor, 0)?;
    expression(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(1.);
    expression(&mut editor, 2)?;
    let selected = editor.animation().unwrap().expression.clone().unwrap();
    editor.seek_animation(id, 0.25)?;
    expression(&mut editor, 2)?;
    let animation = editor.animation().unwrap();
    assert_eq!(animation.expressions.len(), 2);
    assert_eq!(animation.expressions[1].time, 0.25);
    assert_eq!(animation.expressions[1].expression_id, selected);
    let generation = animation.preview_generation;
    // Clicking again without moving before the trigger must remain a no-op.
    expression(&mut editor, 2)?;
    assert_eq!(editor.animation().unwrap().preview_generation, generation);
    editor.seek_animation(id, 0.75)?;
    let frame = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    assert!((frame.snapshot.parameters[&parameter] - 0.8).abs() < 1e-6);
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
    assert!(resources[2].selection.entry.is_none());
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
fn scrubbing_presents_completed_samples_and_release_rejects_old_targets() -> anyhow::Result<()> {
    for motion in [0, 2] {
        let (mut editor, _assets) = setup()?;
        select(&mut editor, motion)?;
        let document = editor.active_id().unwrap();
        let mut evaluator = AnimationEvaluator::default();
        editor.scrub_animation(document, 0.8)?;
        let first = evaluate(&mut editor, &mut evaluator)?;
        editor.scrub_animation(document, 0.2)?;
        let backward = evaluate(&mut editor, &mut evaluator)?;
        assert_eq!(
            first.identity.preview_generation,
            backward.identity.preview_generation
        );
        assert_eq!(
            first.identity.acceptance_floor,
            backward.identity.acceptance_floor
        );
        // CPU and GPU completion can both lag behind the pointer.
        assert!(editor.accepts_animation(first.identity));
        assert!(editor.present_animation(first.identity, first.snapshot));
        editor.scrub_animation(document, 0.6)?;
        let pending = editor.animation_request().unwrap();
        assert!(editor.present_animation(backward.identity, backward.snapshot));
        assert_eq!(
            editor.animation().unwrap().presented.as_ref().unwrap().time,
            0.2
        );
        assert!(!editor.accepts_animation(first.identity));
        editor.seek_animation(document, 0.4)?;
        assert!(!editor.accepts_animation(pending.identity));
        let released = evaluate(&mut editor, &mut evaluator)?;
        assert!(editor.present_animation(released.identity, released.snapshot));
        assert_eq!(
            editor.animation().unwrap().presented.as_ref().unwrap().time,
            0.4
        );
        assert!(!editor.animation().unwrap().playing);
    }
    Ok(())
}

#[test]
fn interrupting_a_backward_scrub_does_not_block_the_final_lower_frame() -> anyhow::Result<()> {
    for resume in [false, true] {
        let (mut editor, _assets) = setup()?;
        select(&mut editor, 0)?;
        let document = editor.active_id().unwrap();
        let mut evaluator = AnimationEvaluator::default();
        editor.scrub_animation(document, 1.5)?;
        let shown = evaluate(&mut editor, &mut evaluator)?;
        assert!(editor.present_animation(shown.identity, shown.snapshot));
        editor.scrub_animation(document, 0.2)?;
        let obsolete = editor.animation_request().unwrap();
        if resume {
            editor.toggle_animation()?;
        } else {
            editor.pause_animation();
        }
        assert!(!editor.accepts_animation(obsolete.identity));
        let lower = evaluate(&mut editor, &mut evaluator)?;
        assert!(editor.present_animation(lower.identity, lower.snapshot));
        assert_eq!(
            editor.animation().unwrap().presented.as_ref().unwrap().time,
            0.2
        );
    }
    Ok(())
}

#[test]
fn continuous_frames_do_not_starve_and_barriers_reject_late_frames() -> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 2)?;
    editor.toggle_animation()?;
    let mut evaluator = AnimationEvaluator::default();
    editor.advance_animation_clock(0.5);
    let older = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    editor.advance_animation_clock(0.6);
    let newer = editor.animation_request().unwrap();
    assert!(editor.present_animation(older.identity, older.snapshot.clone()));
    assert!(!editor.present_animation(older.identity, older.snapshot));
    editor.pause_animation();
    assert!(!editor.accepts_animation(newer.identity));
    let paused = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    assert!(editor.present_animation(paused.identity, paused.snapshot));
    assert_eq!(
        editor.animation().unwrap().presented.as_ref().unwrap().time,
        0.5
    );
    editor.rewind_animation()?;
    let zero = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    assert!(editor.present_animation(zero.identity, zero.snapshot));
    let request = editor.animation_request();
    assert!(request.is_none());
    editor.pan_by(CanvasPoint { x: 5., y: 2. });
    let old_view = editor.animation_request().unwrap();
    let result = evaluator.evaluate(&old_view, |_, _| true)?;
    editor.pan_by(CanvasPoint { x: 1., y: 0. });
    assert!(!editor.present_animation(result.identity, result.snapshot));
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
fn local_model3_live_expression_switch_reuses_physics_history() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao/runtime/mao_pro.model3.json");
    if !path.exists() {
        eprintln!("Skipping live expression timing: local Mao model is unavailable");
        return Ok(());
    }
    let (document, _) = Document::import_model3(&path)?;
    let catalog = document.animation_catalog();
    let motion = catalog
        .motions
        .iter()
        .find(|motion| motion.looping && motion.playable())
        .unwrap();
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    });
    editor.select_motion(editor.active_id().unwrap(), motion.selection.clone())?;
    editor.toggle_animation()?;
    let mut evaluator = AnimationEvaluator::default();
    editor.advance_animation_clock(1.);
    evaluate(&mut editor, &mut evaluator)?;
    editor.advance_animation_clock(60.5);
    evaluate(&mut editor, &mut evaluator)?;
    for expression in catalog
        .expressions
        .iter()
        .filter(|expression| expression.unresolved_targets.is_empty())
        .take(2)
    {
        editor.select_expression(editor.active_id().unwrap(), expression.id.clone())?;
        let request = editor.animation_request().unwrap();
        let mut steps = 0;
        let start = std::time::Instant::now();
        let frame = evaluator.evaluate(&request, |_, total| {
            steps = total;
            true
        })?;
        let incremental = start.elapsed();
        let start = std::time::Instant::now();
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        eprintln!(
            "Mao expression switch at {}s: {steps} steps, {incremental:?}; cold replay {:?}",
            request.identity.time,
            start.elapsed()
        );
        assert!(steps <= 60);
        assert_eq!(frame.snapshot, cold.snapshot);
        assert_eq!(frame.scene.runtime_frame, cold.scene.runtime_frame);
        editor.advance_animation_clock(editor.animation().unwrap().time + 0.5);
        evaluate(&mut editor, &mut evaluator)?;
    }
    Ok(())
}

#[test]
fn local_model3_motion_uses_imported_physics_and_default_runtime_baseline() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao/runtime/mao_pro.model3.json");
    if !path.exists() {
        return Ok(());
    }
    let (document, _) = Document::import_model3(&path)?;
    let modified = document.modified();
    let history = document.history_lengths();
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    });
    let resources = editor.document().unwrap().motion_resources();
    assert!(!resources.is_empty());
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    let mut evaluator = AnimationEvaluator::default();
    editor.advance_animation_clock(0.5);
    let first = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    editor.advance_animation_clock(1.);
    let next = editor.animation_request().unwrap();
    let continuous = evaluator.evaluate(&next, |_, _| true)?;
    let cold = AnimationEvaluator::default().evaluate(&next, |_, _| true)?;
    assert_eq!(continuous.snapshot, cold.snapshot);
    assert_eq!(continuous.scene.viewport_frame, cold.scene.viewport_frame);
    assert!(
        continuous
            .scene
            .viewport_frame
            .drawables
            .iter()
            .any(|d| d.visible)
    );
    assert_ne!(first.snapshot.parameters, continuous.snapshot.parameters);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert_eq!(editor.document().unwrap().modified(), modified);
    Ok(())
}

#[test]
fn catalog_preserves_registration_fades_and_reuses_only_the_current_model_revision()
-> anyhow::Result<()> {
    use std::sync::Arc;
    let (mut editor, _assets) = setup()?;
    let first = editor.document().unwrap().animation_catalog();
    assert_eq!(first.motions.len(), 3);
    assert_eq!(
        first.motions[0].selection.clip_id,
        first.motions[1].selection.clip_id
    );
    assert_ne!(first.motions[0].selection, first.motions[1].selection);
    assert_eq!(first.motions[0].fade_in, 0.);
    assert_eq!(first.motions[1].fade_in, 1.);
    assert_eq!(
        (
            first.motions[0].fps,
            first.motions[0].tracks,
            first.motions[0].events
        ),
        (60., 1, 0)
    );
    assert_eq!(
        (first.expressions[0].fade_in, first.expressions[0].fade_out),
        (0.2, 0.3)
    );
    assert_eq!(first.expressions[0].parameters, 1);
    assert_eq!(
        first.expressions[0].blends,
        vec![kasane_core::document::ExpressionBlend::Add]
    );
    assert_eq!(
        first.parameters.len(),
        editor.document().unwrap().parameters().len()
    );
    expression(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.75);
    assert!(Arc::ptr_eq(
        &first,
        &editor.document().unwrap().animation_catalog()
    ));
    editor.set_workspace(WorkspaceKind::Modeling)?;
    editor.execute(EditorCommand::SetObjectProperty {
        object: ObjectRef::new(ObjectKind::Expression, first.expressions[0].id.clone()),
        field: "fade_in".into(),
        value: serde_json::json!(0.7),
    })?;
    let changed = editor.document().unwrap().animation_catalog();
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_eq!(changed.expressions[0].fade_in, 0.7);
    editor.dispatch(editor.command(CommandId::Undo))?;
    let undone = editor.document().unwrap().animation_catalog();
    assert!(!Arc::ptr_eq(&changed, &undone));
    assert_eq!(undone.expressions[0].fade_in, 0.2);
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(
        editor.document().unwrap().animation_catalog().expressions[0].fade_in,
        0.7
    );
    Ok(())
}

#[test]
fn diagnostics_are_available_before_play_and_do_not_duplicate_sdk_coverage() -> anyhow::Result<()> {
    let (document, _assets) = support::diagnostic_animation_document()?;
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let catalog = editor.document().unwrap().animation_catalog();
    assert_eq!(catalog.diagnostics.len(), 5); // 2 distinct registrations of Sound + 3 target issues.
    assert!(catalog.diagnostics.iter().any(
        |d| matches!(d, AnimationDiagnostic::SoundUnsupported { file, .. } if file == "test.wav")
    ));
    let invalid = &catalog.expressions[3];
    assert_eq!((invalid.fade_in, invalid.fade_out), (1., 1.));
    assert_eq!(
        invalid.blends,
        vec![kasane_core::document::ExpressionBlend::Add]
    );
    assert!(
        editor
            .select_expression(editor.active_id().unwrap(), invalid.id.clone())
            .is_err()
    );
    assert!(editor.animation().unwrap().expression.is_none());
    editor.select_motion(
        editor.active_id().unwrap(),
        catalog.motions[3].selection.clone(),
    )?;
    let mut evaluator = AnimationEvaluator::default();
    let evaluated = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(evaluated.snapshot.coverage.len(), 2);
    assert_eq!(
        catalog.diagnostics_for(Some(&evaluated.snapshot)),
        catalog.diagnostics
    );
    let mut snapshot = (*evaluated.snapshot).clone();
    snapshot.coverage.extend([
        "Physics: step limit reached".into(),
        "Future coverage".into(),
        "Future coverage".into(),
    ]);
    let diagnostics = catalog.diagnostics_for(Some(&snapshot));
    assert_eq!(diagnostics.len(), 7);
    assert!(diagnostics.contains(&AnimationDiagnostic::PhysicsStepLimit));
    assert!(diagnostics.contains(&AnimationDiagnostic::RuntimeCoverage {
        detail: "Future coverage".into()
    }));
    let index = editor.document().unwrap().object_index();
    for object in &index.objects {
        if object.reference.kind == ObjectKind::Motion {
            assert!(object.properties.get("state").is_none());
        }
    }
    for relation in &index.relations {
        assert!(!relation.detail.contains("not playing"));
        assert!(!relation.detail.contains("not applied"));
    }
    assert!(!editor.document().unwrap().modified());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn runtime_failures_remain_typed_for_localized_display() {
    let error: anyhow::Error = kasane_sdk::AnimationError::SeekLimit.into();
    assert_eq!(
        AnimationFailure::from_error(&error),
        AnimationFailure::Runtime(kasane_sdk::AnimationError::SeekLimit)
    );
    assert_eq!(
        AnimationFailure::from_error(&anyhow::anyhow!("GPU unavailable")),
        AnimationFailure::Other("GPU unavailable".into())
    );
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

#[test]
fn paused_expression_switches_cut_immediately_without_advancing_motion_or_accumulating()
-> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    let authoring = editor.document().unwrap().parameters();
    let parameter = authoring[0].id.clone();
    select(&mut editor, 2)?;
    expression(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(41.591);
    let mut evaluator = AnimationEvaluator::default();
    evaluate(&mut editor, &mut evaluator)?;
    editor.pause_animation();
    for index in [2, 1, 0, 2] {
        expression(&mut editor, index)?;
        let animation = editor.animation().unwrap();
        assert!(!animation.playing);
        assert_eq!(animation.time, 41.591);
        assert_eq!(
            animation.expressions.len(),
            2,
            "only one cut at the paused time"
        );
        let selected = animation.expression.clone().unwrap();
        let request = editor.animation_request().unwrap();
        let frame = evaluator.evaluate(&request, |_, _| true)?;
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(frame.snapshot, cold.snapshot);
        assert_eq!(frame.scene.runtime_frame, cold.scene.runtime_frame);
        assert_eq!(frame.snapshot.time, 41.591);
        assert_eq!(frame.snapshot.active_expressions, vec![selected]);
        if index == 2 {
            assert!((frame.snapshot.parameters[&parameter] - 0.8).abs() < 1e-6);
        }
        assert!(editor.present_animation(frame.identity, frame.snapshot));
    }
    let generation = editor.animation().unwrap().preview_generation;
    expression(&mut editor, 2)?;
    assert_eq!(editor.animation().unwrap().preview_generation, generation);
    // Resuming does not undo the cut; subsequent choices still crossfade.
    editor.toggle_animation()?;
    expression(&mut editor, 0)?;
    let started = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(started.snapshot.active_expressions.len(), 2);
    editor.advance_animation_clock(42.);
    let finished = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(
        finished.snapshot.active_expressions,
        vec![editor.animation().unwrap().expression.clone().unwrap()]
    );
    assert_eq!(editor.document().unwrap().parameters(), authoring);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn paused_click_on_the_selected_fading_expression_settles_it_and_rejects_old_frames()
-> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    expression(&mut editor, 2)?;
    let stale = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    editor.pause_animation();
    expression(&mut editor, 2)?;
    assert!(!editor.present_animation(stale.identity, stale.snapshot));
    let frame = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    assert_eq!(frame.snapshot.time, 0.);
    assert!((frame.snapshot.parameters[&parameter] - 0.8).abs() < 1e-6);
    assert_eq!(frame.snapshot.active_expressions.len(), 1);
    Ok(())
}
