use super::common::*;
use kasane_project::import_expression3;

#[test]
fn expression_schedule_edits_reuse_only_unchanged_canonical_history() {
    let expression = "00000000-0000-4000-8000-000000000e06";
    let mut preview = combined_cache_preview();
    preview.advance_to_with_progress(1., |_, _| true).unwrap();
    preview.advance_to_with_progress(60.5, |_, _| true).unwrap();
    let schedules = [
        vec![(expression, 0.2), (expression, 1.5), (expression, 60.5)],
        // Replace a future branch and preserve equal-time activation order.
        vec![
            (expression, 0.2),
            (expression, 1.5),
            (expression, 30.),
            (expression, 30.),
        ],
        vec![(expression, 0.2), (expression, 1.5)],
        vec![],
        vec![(expression, 0.)],
    ];
    for schedule in schedules {
        preview
            .replace_expression_schedule(schedule.iter().copied())
            .unwrap();
        let mut cold = combined_cache_preview();
        cold.set_seek_cache_budget(0);
        cold.replace_expression_schedule(schedule.iter().copied())
            .unwrap();
        let expected = cold.seek(60.5).unwrap().clone();
        let mut steps = 0;
        assert_eq!(
            preview
                .advance_to_with_progress(60.5, |_, total| {
                    steps = total;
                    true
                })
                .unwrap(),
            &expected
        );
        if schedule.last().is_some_and(|(_, time)| *time == 60.5) {
            assert_eq!(steps, 30, "a live switch must not replay 60 seconds");
        }
        // Restoring old checkpoints must load the new schedule, never resurrect
        // a removed future activation or lose an appended one.
        for time in [1., 30., 30.01, 60., 60.5, 61.] {
            assert_eq!(
                preview.seek(time).unwrap(),
                cold.seek(time).unwrap(),
                "time={time}"
            );
        }
    }
}

#[test]
fn future_expression_schedule_edits_preserve_playback_and_invalid_edits_are_atomic() {
    let expression = "00000000-0000-4000-8000-000000000e06";
    let mut preview = combined_cache_preview();
    preview.seek(4.).unwrap();
    let schedule = [(expression, 0.2), (expression, 1.5), (expression, 5.)];
    let snapshot = preview.snapshot().clone();
    preview.replace_expression_schedule(schedule).unwrap();
    assert_eq!(preview.snapshot(), &snapshot);
    let operation = preview.operation().clone();
    let stats = preview.seek_cache_stats();
    preview.replace_expression_schedule(schedule).unwrap();
    assert_eq!(preview.operation(), &operation);
    assert_eq!(preview.seek_cache_stats(), stats);
    for invalid in [
        vec![("missing", 0.)],
        vec![(expression, -1.)],
        vec![(expression, f32::NAN)],
        vec![(expression, 6.), ("missing", 7.)],
    ] {
        assert!(preview.replace_expression_schedule(invalid).is_err());
        assert_eq!(preview.snapshot(), &snapshot);
        assert_eq!(preview.operation(), &operation);
        assert_eq!(preview.seek_cache_stats(), stats);
    }
    let mut cold = combined_cache_preview();
    cold.replace_expression_schedule(schedule).unwrap();
    assert_eq!(
        preview.advance_to_with_progress(6., |_, _| true).unwrap(),
        cold.seek(6.).unwrap()
    );
}

#[test]
fn expression_schedule_edits_respect_cache_budget_and_resume_after_cancellation() {
    let expression = "00000000-0000-4000-8000-000000000e06";
    for budget in [0, 1, 128 * 1024, 16 * 1024 * 1024] {
        let mut preview = combined_cache_preview();
        preview.set_seek_cache_budget(budget);
        preview.advance_to_with_progress(1., |_, _| true).unwrap();
        preview.advance_to_with_progress(10., |_, _| true).unwrap();
        let schedule = [(expression, 0.2), (expression, 1.5), (expression, 10.)];
        preview.replace_expression_schedule(schedule).unwrap();
        assert!(
            preview.snapshot().time < 10.,
            "checkpoint at activation time is already affected"
        );
        assert!(preview
            .advance_to_with_progress(10., |done, _| done < 10)
            .is_err());
        let mut cold = combined_cache_preview();
        cold.replace_expression_schedule(schedule).unwrap();
        assert_eq!(
            preview.advance_to_with_progress(10., |_, _| true).unwrap(),
            cold.seek(10.).unwrap()
        );
        assert!(preview.seek_cache_stats().estimated_bytes <= budget);
        // Cancelled work from one recipe must not leak into its replacement.
        preview
            .replace_expression_schedule([(expression, 0.2)])
            .unwrap();
        cold.replace_expression_schedule([(expression, 0.2)])
            .unwrap();
        assert_eq!(
            preview.advance_to_with_progress(10., |_, _| true).unwrap(),
            cold.seek(10.).unwrap()
        );
    }
}

#[test]
fn immediate_expression_transitions_replace_the_queue_and_invalidate_cached_fades() {
    use kasane_animation::ExpressionTransition::{Fade, Immediate};
    let expression = "00000000-0000-4000-8000-000000000e06";
    let mut preview = combined_cache_preview();
    preview.seek(8.).unwrap();
    for transition in [Immediate, Fade, Immediate] {
        let schedule = [(expression, 0.2, Fade), (expression, 1.033, transition)];
        preview
            .replace_expression_schedule_with_transitions(schedule)
            .unwrap();
        let mut cold = combined_cache_preview();
        cold.set_seek_cache_budget(0);
        cold.replace_expression_schedule_with_transitions(schedule)
            .unwrap();
        for time in [1.033, 8., 0., 1., 1.033, 1.05, 4.] {
            let snapshot = preview.advance_to_with_progress(time, |_, _| true).unwrap();
            assert_eq!(
                snapshot,
                cold.seek(time).unwrap(),
                "time={time}, transition={transition:?}"
            );
            if time == 1.033 {
                assert_eq!(
                    snapshot.active_expressions.len(),
                    if transition == Immediate { 1 } else { 2 }
                );
            }
        }
    }
    // Immediate applies even at time zero, where a positive authored fade has zero weight.
    let doc = import_expression3(&document(), expression, "Overwrite",
        r#"{"Type":"Live2D Expression","FadeInTime":1,"Parameters":[{"Id":"ParamX","Value":0.75,"Blend":"Overwrite"}]}"#).unwrap().candidate;
    let mut preview = MotionPreview::new(&doc);
    preview
        .replace_expression_schedule_with_transitions([(expression, 0., Immediate)])
        .unwrap();
    assert_eq!(preview.advance(0.).unwrap().parameters[PARAM], 0.75);
    preview
        .replace_expression_schedule([(expression, 0.)])
        .unwrap();
    assert_eq!(preview.advance(0.).unwrap().parameters[PARAM], 0.);
}
