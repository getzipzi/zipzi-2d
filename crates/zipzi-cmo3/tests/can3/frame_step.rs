use super::*;
use zipzi_animation::{sample_motion_curve, CompiledCurve};

fn motion(curve: MotionCurve, frames: u32, fps: f32) -> MotionClip {
    let mut clip = make_clip();
    clip.events.clear();
    clip.looping = false;
    clip.restricted_beziers = false;
    clip.fps = fps;
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0., 0.),
        segments: Arc::new(vec![MotionSegment {
            end: key(12, 1., 1.),
            curve,
            frame_step: MotionFrameStep::from_frames(frames).unwrap(),
        }]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    clip
}

fn compare(source: &MotionClip, result: &MotionClip, times: impl IntoIterator<Item = f32>) {
    let times: Vec<_> = times.into_iter().collect();
    assert_eq!(source.tracks.len(), result.tracks.len());
    for a in &source.tracks {
        let b = result.tracks.iter().find(|b| b.target == a.target).unwrap();
        let first = CompiledCurve::from_track(a, source.fps);
        let second = CompiledCurve::from_track(b, result.fps);
        for &t in &times {
            let x = sample_motion_curve(&first, t, source.restricted_beziers);
            let y = sample_motion_curve(&second, t, result.restricted_beziers);
            assert!((x - y).abs() < 0.00005, "time {t}: {x} != {y}");
        }
    }
}

#[test]
fn native_frame_step_keeps_editable_curves_and_n_in_all_export_options() {
    for fps in [24., 30., 60.] {
        for n in [2, 3, 7, 100] {
            for curve in [
                MotionCurve::Linear {},
                MotionCurve::AutoSmooth {},
                MotionCurve::Stepped {},
                MotionCurve::Bezier {
                    control1: MotionPoint {
                        time: 0.2,
                        value: 0.9,
                    },
                    control2: MotionPoint {
                        time: 0.7,
                        value: 0.1,
                    },
                    preset: None,
                },
            ] {
                let clip = motion(curve, n, fps);
                let (base, _) = fixture();
                let doc = prepare(&base, |b| b.create_motion(clip.clone()));
                let content = doc.content_id();
                for bake_animation in [false, true] {
                    for auto_frame_rate in [false, true] {
                        let out = encode_can3(
                            &doc,
                            &Can3Options {
                                bake_animation,
                                auto_frame_rate,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                        assert!(!out.report.has_loss());
                        assert!(out
                            .report
                            .diagnostics
                            .iter()
                            .any(|d| d.code == "CAN3_NATIVE_FRAME_STEP"));
                        let imported = import_animation(&out.bytes, &base).unwrap();
                        let after = imported.motion(&clip.id).unwrap();
                        assert_eq!(after.tracks[0].segments.len(), 1);
                        assert_eq!(
                            after.tracks[0].segments[0].frame_step,
                            MotionFrameStep::EveryFrames { frames: n }
                        );
                        compare(&clip, after, (0..=100).map(|i| i as f32 / 100.));
                        assert_eq!(doc.content_id(), content);
                    }
                }
            }
        }
    }
}

#[test]
fn independent_grids_and_off_intervals_lower_locally_without_retiming_other_tracks() {
    let mut clip = motion(MotionCurve::AutoSmooth {}, 3, 30.);
    Arc::make_mut(&mut clip.tracks[0].segments).push(MotionSegment::new(
        key(13, 2., 0.),
        MotionCurve::Bezier {
            control1: MotionPoint {
                time: 1.3,
                value: 0.9,
            },
            control2: MotionPoint {
                time: 1.7,
                value: 0.1,
            },
            preset: None,
        },
    ));
    clip.duration = 2.;
    clip.tracks.push(MotionTrack {
        id: uuid(20),
        target: MotionTrackTarget::Model {
            runtime_id: "Opacity".into(),
        },
        initial: key(21, 0., 0.2),
        segments: Arc::new(vec![MotionSegment {
            end: key(22, 2., 0.9),
            curve: MotionCurve::Linear {},
            frame_step: MotionFrameStep::EveryFrames { frames: 2 },
        }]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    let (base, png) = fixture();
    let doc = prepare(&base, |b| b.create_motion(clip.clone()));
    let out = encode_can3(&doc, &Can3Options::default()).unwrap();
    assert!(!out.report.has_loss());
    assert!(out
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_FRAME_STEP_BAKED"));
    let result = import_animation(&out.bytes, &base).unwrap();
    let after = result.motion(&clip.id).unwrap();
    compare(&clip, after, (0..=2000).map(|i| i as f32 / 1000.));
    assert!(matches!(
        after.tracks[0].segments.last().unwrap().curve,
        MotionCurve::Bezier { .. }
    ));
    let first = CompiledCurve::from_track(&clip.tracks[1], clip.fps);
    let second = CompiledCurve::from_track(&after.tracks[1], after.fps);
    for i in 0..=60 {
        let t = (i as f64 / 30.) as f32;
        assert!(
            (sample_motion_curve(&first, t, false) - sample_motion_curve(&second, t, false)).abs()
                < 0.00005
        );
    }
    let pair = encode_cubism(
        &doc,
        &|_: &AssetRecord| Ok(png.clone()),
        &CubismOptions::default(),
    )
    .unwrap();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/can3-frame-step-local");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("model.cmo3"), pair.cmo3.bytes).unwrap();
    std::fs::write(dir.join("model.can3"), pair.can3.bytes).unwrap();
    let tracks: Vec<_> = clip.tracks.iter().zip(["live2dParam_ParamX", "opacity"]).map(|(track, attribute)| {
        let curve = CompiledCurve::from_track(track, clip.fps);
        serde_json::json!({
            "attribute":attribute,
            "values":(0..=60).map(|i|sample_motion_curve(&curve,(i as f64/30.) as f32,false)).collect::<Vec<_>>(),
            "checks":(0..=120).map(|i| {
                let frame=i as f64/2.;
                serde_json::json!({"frame":frame,"value":sample_motion_curve(&curve,(frame/30.) as f32,false)})
            }).collect::<Vec<_>>()
        })
    }).collect();
    std::fs::write(
        dir.join("expected.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!([{"name":clip.name,"fps":clip.fps,"tracks":tracks}]),
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn subframe_origins_nonintegral_fps_large_n_and_inverse_boundaries_keep_displayed_frames() {
    for fps in [29.97, 30., 60.] {
        for n in [1, 2, 3, 7, u32::MAX] {
            let mut clip = motion(MotionCurve::AutoSmooth {}, n, fps);
            clip.tracks[0].initial.time = 0.01257;
            Arc::make_mut(&mut clip.tracks[0].segments)[0].end.time = 0.517;
            Arc::make_mut(&mut clip.tracks[0].segments).push(MotionSegment {
                end: key(13, 1., 0.3),
                curve: MotionCurve::InverseStepped {},
                frame_step: MotionFrameStep::from_frames(n).unwrap(),
            });
            let (base, _) = fixture();
            let doc = prepare(&base, |b| b.create_motion(clip.clone()));
            for auto_frame_rate in [false, true] {
                let out = encode_can3(
                    &doc,
                    &Can3Options {
                        bake_animation: true,
                        auto_frame_rate,
                        ..Default::default()
                    },
                )
                .unwrap();
                let result = import_animation(&out.bytes, &base).unwrap();
                let after = result.motion(&clip.id).unwrap();
                compare(
                    &clip,
                    after,
                    (0..=(after.fps.ceil() as i32))
                        .map(|i| (i as f64 / f64::from(after.fps)) as f32),
                );
            }
        }
    }
}

#[test]
fn native_same_n_keys_keep_each_interval_phase_and_write_cubism_probe() {
    let mut clip = motion(MotionCurve::AutoSmooth {}, 3, 30.);
    Arc::make_mut(&mut clip.tracks[0].segments)[0].end.time = 4. / 30.;
    Arc::make_mut(&mut clip.tracks[0].segments).push(MotionSegment {
        end: key(13, 1., 0.2),
        curve: MotionCurve::AutoSmooth {},
        frame_step: MotionFrameStep::EveryFrames { frames: 3 },
    });
    let (base, png) = fixture();
    let doc = prepare(&base, |b| b.create_motion(clip.clone()));
    let out = encode_cubism(
        &doc,
        &|_: &AssetRecord| Ok(png.clone()),
        &CubismOptions::default(),
    )
    .unwrap();
    let result = import_animation(&out.can3.bytes, &base).unwrap();
    let after = result.motion(&clip.id).unwrap();
    assert_eq!(after.tracks[0].segments.len(), 2);
    compare(&clip, after, (0..=30).map(|i| (i as f64 / 30.) as f32));
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/can3-frame-step-native");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("model.cmo3"), out.cmo3.bytes).unwrap();
    std::fs::write(dir.join("model.can3"), out.can3.bytes).unwrap();
    let curve = CompiledCurve::from_track(&clip.tracks[0], clip.fps);
    let values: Vec<_> = (0..=30)
        .map(|i| sample_motion_curve(&curve, (i as f64 / 30.) as f32, false))
        .collect();
    let expected = serde_json::json!([{
        "name": clip.name,
        "fps": clip.fps,
        "tracks": [{
            "attribute": "live2dParam_ParamX",
            "values": values,
            "native_step_counts": [
                {"frame": 0, "value": 3},
                {"frame": 4, "value": 3},
                {"frame": 30, "value": 1}
            ],
            "step_checks": [
                {"frame": 3.5, "stepped": 3},
                {"frame": 4.5, "stepped": 4},
                {"frame": 6.5, "stepped": 4},
                {"frame": 7., "stepped": 7}
            ]
        }]
    }]);
    std::fs::write(
        dir.join("expected.json"),
        serde_json::to_vec_pretty(&expected).unwrap(),
    )
    .unwrap();
}

#[test]
fn one_frame_stays_continuous_and_restricted_beziers_use_effective_time_handles() {
    for restricted in [false, true] {
        let mut clip = motion(
            MotionCurve::Bezier {
                control1: MotionPoint {
                    time: 0.7,
                    value: 0.1,
                },
                control2: MotionPoint {
                    time: 0.9,
                    value: 0.8,
                },
                preset: None,
            },
            1,
            30.,
        );
        clip.restricted_beziers = restricted;
        let (base, _) = fixture();
        let doc = prepare(&base, |b| b.create_motion(clip.clone()));
        let out = encode_can3(&doc, &Can3Options::default()).unwrap();
        assert!(!out
            .report
            .diagnostics
            .iter()
            .any(|d| d.code == "CAN3_NATIVE_FRAME_STEP"));
        let result = import_animation(&out.bytes, &base).unwrap();
        let after = result.motion(&clip.id).unwrap();
        assert_eq!(
            after.tracks[0].segments[0].frame_step,
            MotionFrameStep::Off {}
        );
        compare(&clip, after, (0..=1000).map(|n| n as f32 / 1000.));
    }
}

#[test]
fn inferred_scene_fps_retains_source_frame_step_period_and_unstepped_curve_shape() {
    let mut clip = motion(MotionCurve::AutoSmooth {}, 3, 30.);
    clip.tracks[0].initial.time = 1. / 60.;
    let (base, _) = fixture();
    let doc = prepare(&base, |b| b.create_motion(clip.clone()));
    let out = encode_can3(
        &doc,
        &Can3Options {
            auto_frame_rate: true,
            ..Default::default()
        },
    )
    .unwrap();
    let result = import_animation(&out.bytes, &base).unwrap();
    let after = result.motion(&clip.id).unwrap();
    assert_eq!(after.fps, 60.);
    compare(&clip, after, (0..=60).map(|n| (n as f64 / 60.) as f32));
}

#[test]
fn frame_step_preserves_mixed_inverse_step_boundaries_in_default_strict_export() {
    let mut clip = motion(MotionCurve::AutoSmooth {}, 3, 30.);
    Arc::make_mut(&mut clip.tracks[0].segments)[0].end.time = 0.5;
    Arc::make_mut(&mut clip.tracks[0].segments).push(MotionSegment {
        end: key(13, 1., 0.2),
        curve: MotionCurve::InverseStepped {},
        frame_step: MotionFrameStep::EveryFrames { frames: 2 },
    });
    let (base, _) = fixture();
    let doc = prepare(&base, |b| b.create_motion(clip.clone()));
    let out = encode_can3(&doc, &Can3Options::default()).unwrap();
    assert!(!out.report.has_loss());
    let result = import_animation(&out.bytes, &base).unwrap();
    let after = result.motion(&clip.id).unwrap();
    compare(&clip, after, (0..=30).map(|n| (n as f64 / 30.) as f32));
}

#[test]
#[ignore = "Run the local Cubism probe with --resave-animation first"]
fn native_frame_step_reimport_after_cubism_resave_retains_n_and_phase() {
    let (base, _) = fixture();
    for variant in ["native", "local"] {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/can3-frame-step-{variant}"));
        let before =
            import_animation(&std::fs::read(dir.join("model.can3")).unwrap(), &base).unwrap();
        let saved = std::fs::read(dir.join("native-resaved.can3")).unwrap();
        let after = import_animation(&saved, &base).unwrap();
        let a = before.motion(&uuid(100)).unwrap();
        let b = after.motion(&uuid(100)).unwrap();
        if variant == "native" {
            assert_eq!(b.tracks[0].segments.len(), 2);
            assert!(b.tracks[0]
                .segments
                .iter()
                .all(|segment| segment.frame_step == MotionFrameStep::EveryFrames { frames: 3 }));
        }
        compare(a, b, (0..=2000).map(|n| n as f32 / 1000.));
    }
}

#[test]
fn expansion_is_bounded_before_allocating_and_both_paths_leave_source_unchanged() {
    let mut clip = motion(MotionCurve::Linear {}, 2, 30.);
    clip.duration = 100_000.;
    Arc::make_mut(&mut clip.tracks[0].segments)[0].end.time = clip.duration;
    let mut other = clip.tracks[0].clone();
    other.id = uuid(20);
    other.target = MotionTrackTarget::Model {
        runtime_id: "Opacity".into(),
    };
    other.initial.id = uuid(21);
    let segment = &mut Arc::make_mut(&mut other.segments)[0];
    segment.end.id = uuid(22);
    segment.frame_step = MotionFrameStep::EveryFrames { frames: 3 };
    clip.tracks.push(other);
    let (base, _) = fixture();
    let doc = prepare(&base, |b| b.create_motion(clip));
    let content = doc.content_id();
    assert!(encode_can3(&doc, &Can3Options::default())
        .unwrap_err()
        .to_string()
        .contains("one million"));
    assert_eq!(doc.content_id(), content);
}
