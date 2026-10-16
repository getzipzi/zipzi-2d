use super::common::*;

#[test]
fn canonical_continuous_playback_matches_cold_and_cached_seek_with_physics_pose_expressions() {
    let mut continuous = combined_cache_preview();
    let mut cached = combined_cache_preview();
    let mut cold = combined_cache_preview();
    cold.set_seek_cache_budget(0);
    cached.seek(8.0).unwrap();
    // Seek(0) initializes Pose/Physics; the first positive target still follows
    // the canonical cold replay, which starts with 1/60 rather than advance(0).
    continuous
        .advance_to_with_progress(0., |_, _| true)
        .unwrap();
    for step in [1, 2, 13, 59, 60, 61, 90, 143, 144, 300, 417, 480] {
        let target = step as f32 / 60.;
        let snapshot = continuous
            .advance_to_with_progress(target, |_, _| true)
            .unwrap();
        assert_eq!(snapshot, cold.seek(target).unwrap(), "cold step {step}");
        assert_eq!(snapshot, cached.seek(target).unwrap(), "cached step {step}");
    }
    for target in [8.003, 8.02, 0.0, 1.01, 2., 2.01, 4.] {
        assert_eq!(
            continuous
                .advance_to_with_progress(target, |_, _| true)
                .unwrap(),
            cold.seek(target).unwrap(),
            "fraction/backward {target}"
        );
    }
    continuous.advance(0.031).unwrap();
    assert_eq!(
        continuous
            .advance_to_with_progress(5., |_, _| true)
            .unwrap(),
        cold.seek(5.).unwrap()
    );
}

#[test]
fn canonical_playback_cancellation_resumes_without_skipping_simulation_steps() {
    let mut preview = combined_cache_preview();
    let mut cold = combined_cache_preview();
    preview.advance_to_with_progress(1., |_, _| true).unwrap();
    assert!(matches!(
        preview.advance_to_with_progress(4., |done, _| done < 37),
        Err(kasane_animation::AnimationError::SeekCancelled)
    ));
    assert_eq!(preview.snapshot().time, 97. / 60.);
    assert_eq!(
        preview.advance_to_with_progress(4., |_, _| true).unwrap(),
        cold.seek(4.).unwrap()
    );
    let before = preview.snapshot().clone();
    for time in [f32::NAN, -1., 1_000_001. / 60.] {
        assert!(preview.advance_to_with_progress(time, |_, _| true).is_err());
        assert_eq!(preview.snapshot(), &before);
    }
    preview.schedule_parameter_input(PARAM, 4.5, 0.3).unwrap();
    cold.schedule_parameter_input(PARAM, 4.5, 0.3).unwrap();
    assert_eq!(
        preview.advance_to_with_progress(5., |_, _| true).unwrap(),
        cold.seek(5.).unwrap()
    );
}

#[test]
fn display_samples_reuse_full_steps_and_match_cold_seek() {
    for hz in [75, 120, 144] {
        let mut live = combined_cache_preview();
        let mut cold = combined_cache_preview();
        live.set_seek_cache_budget(0);
        cold.set_seek_cache_budget(0);
        for frame in 1..=hz * 3 {
            let time = frame as f32 / hz as f32;
            let mut steps = 0;
            let actual = live
                .advance_to_with_progress(time, |_, total| {
                    steps = total;
                    true
                })
                .unwrap();
            assert_eq!(actual, cold.seek(time).unwrap(), "{hz} Hz frame {frame}");
            assert!(steps <= 2, "{hz} Hz frame {frame} replayed {steps} steps");
        }
        // A fractional seek can also resume without replaying old history.
        live.seek(1.007).unwrap();
        assert_eq!(
            live.advance_to_with_progress(1.014, |_, total| {
                assert_eq!(total, 1);
                true
            })
            .unwrap(),
            cold.seek(1.014).unwrap()
        );
    }
}

#[test]
fn fractional_sampling_cancellation_and_schedule_changes_preserve_canonical_history() {
    let mut live = combined_cache_preview();
    let mut cold = combined_cache_preview();
    live.advance_to_with_progress(1.007, |_, _| true).unwrap();
    let before = live.snapshot().clone();
    assert!(live.advance_to_with_progress(1.01, |_, _| false).is_err());
    assert_eq!(live.snapshot(), &before);
    assert!(live
        .advance_to_with_progress(1.025, |done, total| done < total)
        .is_err());
    assert_eq!(live.snapshot(), cold.seek(61. / 60.).unwrap());
    assert_eq!(
        live.advance_to_with_progress(1.033, |_, _| true).unwrap(),
        cold.seek(1.033).unwrap()
    );
    let expression = "00000000-0000-4000-8000-000000000e06";
    live.schedule_expression(expression, 1.033).unwrap();
    cold.schedule_expression(expression, 1.033).unwrap();
    for time in [1.04, 1.05, 0.007, 0.014, 0., 0.008, 0.02] {
        assert_eq!(
            live.advance_to_with_progress(time, |_, _| true).unwrap(),
            cold.seek(time).unwrap()
        );
    }
}
