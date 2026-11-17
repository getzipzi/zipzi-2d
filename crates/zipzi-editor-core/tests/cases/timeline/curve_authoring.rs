use super::common::*;
use zipzi_core::document::{MotionCurve, MotionEasingPreset, MotionFrameStep};
use zipzi_editor_core::*;

#[test]
fn settings_explain_selection_and_exclude_terminal_keys_without_partial_locked_edits()
-> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(true, false)?;
    let p = editor.timeline_projection().unwrap();
    assert_eq!(
        p.curve_settings(&[], false).disabled,
        Some(TimelineCurveDisabled::NoSelection)
    );
    assert_eq!(
        p.curve_settings(&[key(12)], false).disabled,
        Some(TimelineCurveDisabled::NoOutgoing)
    );
    assert_eq!(
        p.curve_settings(&[key(10)], true).disabled,
        Some(TimelineCurveDisabled::EventsSelected)
    );
    let s = p.curve_settings(&[key(10), key(11), key(12), key(30)], false);
    assert!(s.enabled());
    assert_eq!(s.keys.len(), 3);
    assert_eq!(s.trailing_keys, 1);
    assert_eq!(s.interpolation, TimelineMixed::Mixed);
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    crate::support::publish_host(&host, client, session, "Lock endpoint", |commands| {
        let mut flags = commands.read_document()?.editor_state().clone();
        flags.objects.entry(id(11)).or_default().locked = true;
        commands.replace_editor_state(flags)
    })?;
    editor.sync_runtime()?;
    let p = editor.timeline_projection().unwrap();
    assert_eq!(
        p.curve_settings(&[key(10), key(30)], false).disabled,
        Some(TimelineCurveDisabled::Locked)
    );
    let before = editor.document().unwrap().history_lengths();
    let d = editor.active_id().unwrap();
    assert!(
        editor
            .execute_timeline(
                d,
                TimelineCommand::FrameStep {
                    keys: vec![key(10), key(30)],
                    frame_step: MotionFrameStep::EveryFrames { frames: 3 }
                }
            )
            .is_err()
    );
    assert_eq!(editor.document().unwrap().history_lengths(), before);
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(30))
            .unwrap()
            .frame_step,
        MotionFrameStep::Off {}
    );
    let (opaque, _assets) = fixture(false, true)?;
    assert_eq!(
        opaque
            .timeline_projection()
            .unwrap()
            .curve_settings(&[key(10)], false)
            .disabled,
        Some(TimelineCurveDisabled::ReadOnly)
    );
    Ok(())
}

#[test]
fn curve_commands_share_preview_cancel_history_and_external_invalidation() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(true, false)?;
    let d = editor.active_id().unwrap();
    let keys = vec![key(10), key(30), key(12)];
    editor.execute_timeline(
        d,
        TimelineCommand::EasingPreset {
            keys: keys.clone(),
            preset: MotionEasingPreset::CubicOut,
        },
    )?;
    let p = editor.timeline_projection().unwrap();
    let s = p.curve_settings(&keys, false);
    assert_eq!(s.keys.len(), 2);
    assert_eq!(
        s.preset,
        TimelineMixed::Uniform(Some(MotionEasingPreset::CubicOut))
    );
    let token = editor.begin_timeline_edit(d, "Set N")?;
    for frames in [2, 3, 7, 100] {
        editor.update_timeline_edit(
            d,
            token,
            TimelineCommand::FrameStep {
                keys: keys.clone(),
                frame_step: MotionFrameStep::EveryFrames { frames },
            },
        )?;
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.cancel_timeline_edit();
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(10))
            .unwrap()
            .frame_step,
        MotionFrameStep::Off {}
    );
    let token = editor.begin_timeline_edit(d, "Set N")?;
    for frames in [2, 3, 7] {
        editor.update_timeline_edit(
            d,
            token,
            TimelineCommand::FrameStep {
                keys: keys.clone(),
                frame_step: MotionFrameStep::EveryFrames { frames },
            },
        )?;
    }
    editor.finish_timeline_edit(d, token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(10))
            .unwrap()
            .frame_step,
        MotionFrameStep::Off {}
    );
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(10))
            .unwrap()
            .frame_step,
        MotionFrameStep::EveryFrames { frames: 7 }
    );
    let token = editor.begin_timeline_edit(d, "Pending N")?;
    editor.update_timeline_edit(
        d,
        token,
        TimelineCommand::FrameStep {
            keys,
            frame_step: MotionFrameStep::EveryFrames { frames: 3 },
        },
    )?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    crate::support::publish_host(&host, client, session, "External", |commands| {
        let mut clip = commands.read_document()?.motion(&id(4)).unwrap().clone();
        clip.name = "External".into();
        commands.replace_motion(clip)
    })?;
    editor.sync_runtime()?;
    assert!(editor.timeline_edit().is_none());
    assert!(editor.finish_timeline_edit(d, token).is_err());
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(10))
            .unwrap()
            .frame_step,
        MotionFrameStep::EveryFrames { frames: 7 }
    );
    Ok(())
}

#[test]
fn automatic_handles_materialize_and_tidy_clears_the_source_without_changing_n()
-> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let d = editor.active_id().unwrap();
    editor.execute_timeline(
        d,
        TimelineCommand::Interpolation {
            keys: vec![key(10), key(11)],
            kind: zipzi_sdk::MotionInterpolation::AutoSmooth,
        },
    )?;
    let handles = editor
        .timeline_projection()
        .unwrap()
        .display_handles(&key(11))
        .unwrap();
    editor.execute_timeline(
        d,
        TimelineCommand::AlignedHandle {
            end: key(11),
            handle: zipzi_sdk::MotionBezierHandle::Control2,
            time: handles.control2.time.into(),
            value: handles.control2.value - 0.1,
        },
    )?;
    let p = editor.timeline_projection().unwrap();
    assert_eq!(
        p.key(&key(10)).unwrap().outgoing,
        Some(zipzi_sdk::MotionInterpolation::Bezier)
    );
    assert_eq!(
        p.key(&key(11)).unwrap().outgoing,
        Some(zipzi_sdk::MotionInterpolation::Bezier)
    );
    editor.execute_timeline(
        d,
        TimelineCommand::EasingPreset {
            keys: vec![key(10)],
            preset: MotionEasingPreset::CubicIn,
        },
    )?;
    editor.execute_timeline(
        d,
        TimelineCommand::FrameStep {
            keys: vec![key(10)],
            frame_step: MotionFrameStep::EveryFrames { frames: 3 },
        },
    )?;
    editor.execute_timeline(
        d,
        TimelineCommand::TidyHandles {
            keys: vec![key(10)],
        },
    )?;
    let clip = editor
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap();
    assert!(matches!(
        clip.tracks[0].segments[0].curve,
        MotionCurve::Bezier { preset: None, .. }
    ));
    assert_eq!(
        clip.tracks[0].segments[0].frame_step,
        MotionFrameStep::EveryFrames { frames: 3 }
    );
    Ok(())
}

#[test]
fn visible_stair_geometry_uses_the_shared_grid_without_generating_keys() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let d = editor.active_id().unwrap();
    editor.execute_timeline(
        d,
        TimelineCommand::FrameStep {
            keys: vec![key(10)],
            frame_step: MotionFrameStep::EveryFrames { frames: 3 },
        },
    )?;
    let p = editor.timeline_projection().unwrap();
    let track = &p.tracks[&id(8)];
    let points = p.frame_step_points(track, 0, 0.2, 0.59, 100).unwrap();
    for pair in points.windows(2) {
        assert!(pair[0].time == pair[1].time || pair[0].value == pair[1].value);
    }
    for point in points
        .iter()
        .skip(1)
        .filter(|point| point.time > 0.2 && point.time < 0.59)
    {
        let index = ((point.time - track.keys[0].time as f32) * 10.).round() as u64;
        assert_eq!(
            point.time,
            zipzi_animation::motion_frame_step_boundary(track.keys[0].time as f32, 30., 3, index)?
        );
    }
    assert!(p.frame_step_points(track, 0, 0.2, 0.59, 2).unwrap().len() <= 7);
    assert_eq!(track.keys.len(), 3);
    assert_eq!(
        editor
            .document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap()
            .tracks[0]
            .segments
            .len(),
        2
    );
    Ok(())
}
