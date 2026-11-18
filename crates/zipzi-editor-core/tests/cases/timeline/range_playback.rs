use super::common::*;
use zipzi_editor_core::*;

fn bind(editor: &mut Editor) -> TimelineBinding {
    let panel = TimelineId::new();
    editor.set_timeline_visible(panel, true);
    editor.timeline_binding(panel).unwrap()
}

#[test]
fn range_playback_preserves_fractional_remainder_and_session_state() -> anyhow::Result<()> {
    let (mut editor, temp) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    let binding = bind(&mut editor);
    editor.update_timeline_view(document, |view| view.snap = true)?;
    editor.set_timeline_loop_range(binding, 24. / 30., 41. / 30.)?;
    let original = editor.timeline_view().unwrap().loop_range().unwrap();
    let mut evaluator = AnimationEvaluator::default();
    for head in [12, 41, 30] {
        editor.seek_timeline(document, head as f32 / 30.)?;
        editor.toggle_animation()?;
        let first = if head == 30 { head } else { 24 };
        for frame in first..=40 {
            editor.advance_animation_clock_precise(f64::from(frame - first) / 30.);
            let request = editor.animation_request().unwrap();
            let progress = request.identity.range_progress.unwrap();
            assert_eq!(progress.cycle, 0);
            assert!((progress.local - frame as f32 / 30.).abs() < 1e-6);
            let sample = evaluator.evaluate(&request, |_, _| true)?;
            assert!(editor.present_animation(sample.identity, sample.snapshot));
        }
        editor.advance_animation_clock_precise((41.25 - f64::from(first)) / 30.);
        let unshown = editor.animation_request().unwrap();
        let progress = unshown.identity.range_progress.unwrap();
        assert_eq!(progress.cycle, 1);
        assert!((progress.local - 24.25 / 30.).abs() < 1e-6);
        editor.pause_animation();
        assert!((editor.animation().unwrap().edit_time - 40. / 30.).abs() < 1e-6);
        assert!(!editor.accepts_animation(unshown.identity));
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    editor.enable_timeline_loop_range(binding, false)?;
    editor.seek_timeline(document, 1.5)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock_precise(1.9);
    let request = editor.animation_request().unwrap();
    assert_eq!(request.identity.time, 1.9);
    assert_eq!(request.identity.range_progress, None);
    editor.enable_timeline_loop_range(binding, true)?;

    let other = editor.create_timeline_motion(document)?;
    assert!(editor.timeline_view().unwrap().loop_range().is_none());
    editor.set_timeline_loop_range(binding, 0., 0.5)?;
    editor.select_motion(
        document,
        MotionSelection {
            clip_id: id(4),
            entry: None,
        },
    )?;
    assert_eq!(editor.timeline_view().unwrap().loop_range(), Some(original));
    assert!(editor.set_timeline_loop_range(binding, 1., 0.5).is_err());
    assert!(
        editor
            .set_timeline_loop_range(binding, 0., f64::INFINITY)
            .is_err()
    );
    assert!(
        editor
            .set_timeline_loop_range(binding, -0.001, 0.5)
            .is_err()
    );
    assert_eq!(editor.timeline_view().unwrap().loop_range(), Some(original));
    editor.select_motion(
        document,
        MotionSelection {
            clip_id: other,
            entry: None,
        },
    )?;
    assert_eq!(
        editor.timeline_view().unwrap().loop_range().unwrap().end,
        0.5
    );
    let reopened = Document::open(&temp.path().join("fixture.zipzi"))?;
    let mut fresh = Editor::from_document(reopened);
    fresh.set_workspace(WorkspaceKind::Animation)?;
    fresh.select_motion(
        fresh.active_id().unwrap(),
        MotionSelection {
            clip_id: id(4),
            entry: None,
        },
    )?;
    assert!(fresh.timeline_view().unwrap().loop_range().is_none());
    Ok(())
}

#[test]
fn range_playback_full_effect_reuses_sdk_cycle_and_observes_only_presented_passages()
-> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    let binding = bind(&mut editor);
    for (time, value) in [(24. / 30., "24"), (26. / 30., "26"), (28. / 30., "28")] {
        editor.execute_timeline(
            document,
            TimelineCommand::Event {
                id: None,
                time,
                value: value.into(),
            },
        )?;
    }
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Result)?;
    editor.update_timeline_view(document, |view| view.snap = true)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(5.);
    let mut evaluator = AnimationEvaluator::default();
    let frame = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(frame.identity, frame.snapshot));
    let sdk_cycle = editor
        .animation()
        .unwrap()
        .result_playback_position()
        .unwrap()
        .loop_index;
    assert_eq!(sdk_cycle, 2);
    editor.set_timeline_loop_range(binding, 24. / 30., 28. / 30.)?;
    editor.seek_timeline(document, 25. / 30.)?;
    editor.toggle_animation()?;
    let frame = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(frame.identity, frame.snapshot));
    assert_eq!(editor.animation().unwrap().range_events().count(), 0);
    editor.advance_animation_clock_precise(4. / 30.);
    let older = evaluate(&mut editor, &mut evaluator)?;
    let bindings = {
        let other = ViewportId::new();
        editor.register_viewport(other, ViewportKind::Animation)?;
        editor.set_viewport_metrics(other, editor.canvas_metrics());
        editor
            .visible_viewport_bindings()
            .into_iter()
            .filter(|binding| binding.kind == ViewportKind::Animation)
            .collect::<Vec<_>>()
    };
    assert_eq!(bindings.len(), 2);
    let stale = editor
        .animation_viewport_request(bindings[1], older.identity)
        .unwrap()
        .0;
    editor.advance_animation_clock_precise(9. / 30.);
    let newest = evaluate(&mut editor, &mut evaluator)?;
    let requests: Vec<_> = bindings
        .iter()
        .map(|&binding| {
            editor
                .animation_viewport_request(binding, newest.identity)
                .unwrap()
                .0
        })
        .collect();
    assert!(editor.present_animation_viewport(requests[0], newest.snapshot.clone()));
    assert!(!editor.accepts_animation(older.identity));
    assert!(!editor.present_animation_viewport(stale, older.snapshot));
    assert!(editor.present_animation_viewport(requests[1], newest.snapshot.clone()));
    let events = editor.animation().unwrap().range_events();
    assert_eq!(events.count(), 5);
    assert_eq!(events.full_cycles, 1);
    assert_eq!(
        events
            .tail
            .iter()
            .chain(&events.cycle_events)
            .chain(&events.head)
            .map(|e| e.value.as_str())
            .collect::<Vec<_>>(),
        ["26", "24", "26", "24", "26"]
    );
    assert_eq!(newest.snapshot.motion_positions[0].loop_index, sdk_cycle);
    editor.advance_animation_clock_precise((9. + 4_000_000.) / 30.);
    let skipped = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(skipped.identity.time, newest.identity.time);
    assert!(editor.present_animation(skipped.identity, skipped.snapshot));
    assert_eq!(
        editor.animation().unwrap().range_events().count(),
        2_000_000
    );
    let local = newest.identity.range_progress.unwrap().local;
    editor.pause_animation();
    assert_eq!(editor.animation().unwrap().range_events().count(), 0);
    editor.toggle_animation()?;
    let resumed = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(resumed.identity, resumed.snapshot));
    assert_eq!(editor.animation().unwrap().range_events().count(), 0);

    editor.seek_timeline(document, local)?;
    let direct = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    assert_eq!(direct.snapshot.parameters, newest.snapshot.parameters);
    assert_eq!(direct.snapshot.physics, newest.snapshot.physics);
    assert_eq!(direct.snapshot.model_opacity, newest.snapshot.model_opacity);
    editor.set_timeline_loop_range(binding, 24. / 30., 25. / 30.)?;
    editor.toggle_animation()?;
    let first = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(first.identity, first.snapshot));
    assert_eq!(editor.animation().unwrap().range_events().count(), 1);
    editor.advance_animation_clock_precise(1.25 / 30.);
    let wrapped = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(wrapped.identity.range_progress.unwrap().cycle, 1);
    assert!(editor.present_animation(wrapped.identity, wrapped.snapshot));
    assert_eq!(
        editor.animation().unwrap().range_events().head[0].value,
        "24"
    );
    editor.advance_animation_clock_precise(2.25 / 30.);
    let stale = editor.animation_request().unwrap();
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Editing)?;
    assert!(!editor.accepts_animation(stale.identity));
    assert!(!editor.animation_content_valid(stale.identity));
    assert_eq!(editor.animation().unwrap().range_events().count(), 0);
    Ok(())
}

#[test]
fn range_playback_seek_parity_includes_physics_expression_and_motion_fade() -> anyhow::Result<()> {
    let (document, _assets) = crate::support::combined_animation_document()?;
    let mut editor = crate::support::editor_from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 256.,
        height: 256.,
        scale_factor: 1.,
    });
    let document = editor.active_id().unwrap();
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Result)?;
    let selection = editor
        .document()
        .unwrap()
        .motion_resources()
        .iter()
        .find(|r| r.name == "Once" && r.fade_in == 1.)
        .unwrap()
        .selection
        .clone();
    editor.select_motion(document, selection)?;
    let expression = editor.document().unwrap().expression_resources()[0]
        .id
        .clone();
    editor.select_expression(document, expression)?;
    let panel = bind(&mut editor);
    editor.set_timeline_loop_range(panel, 0.25, 0.75)?;
    let mut evaluator = AnimationEvaluator::default();
    editor.toggle_animation()?;
    let first = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(first.identity, first.snapshot));
    editor.advance_animation_clock_precise(0.4375);
    let end = evaluate(&mut editor, &mut evaluator)?;
    assert!(editor.present_animation(end.identity, end.snapshot));
    editor.advance_animation_clock_precise(2.125);
    let wrapped = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(
        wrapped.identity.range_progress.unwrap(),
        AnimationRangeProgress {
            cycle: 4,
            local: 0.375
        }
    );
    assert!(editor.present_animation(wrapped.identity, wrapped.snapshot.clone()));
    editor.seek_timeline(document, 0.375)?;
    let direct = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    assert!(!direct.snapshot.physics.is_empty());
    assert_eq!(wrapped.snapshot.parameters, direct.snapshot.parameters);
    assert_eq!(wrapped.snapshot.physics, direct.snapshot.physics);
    assert_eq!(
        wrapped.snapshot.model_opacity,
        direct.snapshot.model_opacity
    );
    assert_eq!(wrapped.scene.viewport_frame, direct.scene.viewport_frame);
    Ok(())
}

#[test]
fn range_playback_clips_bounds_on_duration_changes_and_clears_empty_intersections()
-> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    editor.create_timeline_motion(editor.active_id().unwrap())?;
    let binding = bind(&mut editor);
    editor.update_timeline_view(binding.document, |view| view.snap = false)?;
    editor.set_timeline_loop_range(binding, 1.25, 1.9)?;
    editor.enable_timeline_loop_range(binding, false)?;
    editor.execute_timeline(binding.document, TimelineCommand::ScaleTime(1.5))?;
    assert_eq!(
        editor.timeline_view().unwrap().loop_range(),
        Some(TimelineLoopRange {
            start: 1.25,
            end: 1.5,
            enabled: false,
        })
    );
    editor.execute_timeline(binding.document, TimelineCommand::ScaleTime(1.25))?;
    assert!(editor.timeline_view().unwrap().loop_range().is_none());
    Ok(())
}
