use super::common::*;
use zipzi_editor_core::*;

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
        transition: zipzi_sdk::ExpressionTransition::Fade,
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
