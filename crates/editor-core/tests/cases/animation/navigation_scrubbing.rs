use super::common::*;
use editor_core::*;

#[test]
fn navigation_presents_inflight_frames_in_order_but_rejects_wrong_surface_sizes()
-> anyhow::Result<()> {
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 0)?;
    let document = editor.active_id().unwrap();
    let mut evaluator = AnimationEvaluator::default();
    for mode in [AnimationPreviewMode::Editing, AnimationPreviewMode::Result] {
        editor.set_animation_preview_mode(document, mode)?;
        editor.seek_timeline(document, 0.4)?;
        let content = evaluate(&mut editor, &mut evaluator)?;
        let binding = editor.primary_viewport_binding().unwrap();
        let canvas = content.scene.viewport_frame.model_space;
        let (mut pending, _) = editor
            .animation_viewport_request(binding, content.identity, canvas)
            .unwrap();
        let first = pending;
        for step in 0..24 {
            if step % 2 == 0 {
                editor.pan_by(ViewportPoint { x: 4., y: -2. });
            } else {
                editor.set_zoom_at(editor.view().zoom * 1.01, None);
            }
            assert!(
                editor.animation_request().is_none(),
                "navigation must reuse content"
            );
            let (newer, _) = editor
                .animation_viewport_request(binding, content.identity, canvas)
                .unwrap();
            assert!(newer.viewport_revision > pending.viewport_revision);
            assert!(editor.present_animation_viewport(pending, content.snapshot.clone()));
            pending = newer;
        }
        assert!(editor.present_animation_viewport(pending, content.snapshot.clone()));
        assert!(!editor.present_animation_viewport(first, content.snapshot.clone()));
        for metrics in [
            ViewportMetrics {
                width: 720.,
                height: 480.,
                scale_factor: 1.,
            },
            ViewportMetrics {
                width: 720.,
                height: 480.,
                scale_factor: 2.,
            },
        ] {
            editor.pan_by(ViewportPoint { x: 1., y: 0. });
            let (old_size, _) = editor
                .animation_viewport_request(binding, content.identity, canvas)
                .unwrap();
            editor.set_canvas_metrics(metrics);
            assert!(editor.animation_content_valid(content.identity));
            assert!(!editor.present_animation_viewport(old_size, content.snapshot.clone()));
            assert!(editor.animation_request().is_none());
            let (resized, _) = editor
                .animation_viewport_request(binding, content.identity, canvas)
                .unwrap();
            assert_eq!(resized.metrics, metrics);
            let mut future = resized;
            future.viewport_revision += 1;
            assert!(!editor.present_animation_viewport(future, content.snapshot.clone()));
            assert!(editor.present_animation_viewport(resized, content.snapshot.clone()));
        }
    }
    Ok(())
}

#[test]
fn navigation_reuses_completed_pose_but_keeps_frame_acceptance_and_cancellation()
-> anyhow::Result<()> {
    use std::sync::Arc;
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 2)?;
    let document = editor.active_id().unwrap();
    let mut evaluator = AnimationEvaluator::default();
    for mode in [AnimationPreviewMode::Editing, AnimationPreviewMode::Result] {
        editor.set_animation_preview_mode(document, mode)?;
        editor.seek_timeline(document, 0.35)?;
        let original = evaluate(&mut editor, &mut evaluator)?;
        let binding = editor.primary_viewport_binding().unwrap();
        let canvas = original.scene.viewport_frame.model_space;
        let (first, _) = editor
            .animation_viewport_request(binding, original.identity, canvas)
            .unwrap();
        assert!(editor.present_animation_viewport(first, original.snapshot.clone()));
        for step in 0..16 {
            match step % 3 {
                0 => editor.pan_by(ViewportPoint { x: 3., y: -2. }),
                1 => editor.set_zoom_at(editor.view().zoom * 1.01, None),
                _ => editor.set_canvas_metrics(ViewportMetrics {
                    width: 512. + step as f32,
                    height: 512.,
                    scale_factor: 1.,
                }),
            }
            assert!(editor.animation_request().is_none());
            let (projection, _) = editor
                .animation_viewport_request(binding, original.identity, canvas)
                .unwrap();
            assert_eq!(projection.content, original.identity);
            assert!(!editor.present_animation_viewport(first, original.snapshot.clone()));
            assert!(editor.present_animation_viewport(projection, original.snapshot.clone()));
        }
        editor.invalidate_animation();
        let request = editor.animation_request().unwrap();
        assert!(evaluator.evaluate(&request, |_, _| false).is_err());
        let frame = evaluator.evaluate(&request, |_, _| true)?;
        assert!(Arc::ptr_eq(&original.snapshot, &frame.snapshot));
        assert_eq!(
            frame.identity.acceptance_floor,
            request.identity.acceptance_floor
        );
        assert!(editor.present_animation(frame.identity, frame.snapshot));
        editor.seek_timeline(document, 0.4)?;
        let later = evaluate(&mut editor, &mut evaluator)?;
        assert!(!Arc::ptr_eq(&original.snapshot, &later.snapshot));
        editor.select_motion(
            document,
            editor.document().unwrap().motion_resources()[0]
                .selection
                .clone(),
        )?;
        let selected = evaluate(&mut editor, &mut evaluator)?;
        assert!(!Arc::ptr_eq(&later.snapshot, &selected.snapshot));
    }
    Ok(())
}

#[test]
fn completed_pose_cache_does_not_hide_expression_recipe_changes() -> anyhow::Result<()> {
    use std::sync::Arc;
    let (mut editor, _assets) = setup()?;
    select(&mut editor, 2)?;
    editor.seek_timeline(editor.active_id().unwrap(), 0.5)?;
    let mut evaluator = AnimationEvaluator::default();
    let before = evaluate(&mut editor, &mut evaluator)?;
    expression(&mut editor, 0)?;
    let request = editor.animation_request().unwrap();
    let after = evaluator.evaluate(&request, |_, _| true)?;
    let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
    assert!(!Arc::ptr_eq(&before.snapshot, &after.snapshot));
    assert_eq!(after.snapshot, cold.snapshot);
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
    assert!(editor.present_animation(zero.identity, zero.snapshot.clone()));
    let request = editor.animation_request();
    assert!(request.is_none());
    editor.pan_by(ViewportPoint { x: 5., y: 2. });
    assert!(editor.animation_request().is_none());
    let binding = editor.primary_viewport_binding().unwrap();
    let (old_view, _) = editor
        .animation_viewport_request(
            binding,
            zero.identity,
            zero.scene.viewport_frame.model_space,
        )
        .unwrap();
    editor.pan_by(ViewportPoint { x: 1., y: 0. });
    assert!(editor.present_animation_viewport(old_view, zero.snapshot.clone()));
    assert!(!editor.present_animation_viewport(old_view, zero.snapshot));
    Ok(())
}
