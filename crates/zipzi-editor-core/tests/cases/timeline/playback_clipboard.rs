use super::common::*;
use std::sync::Arc;
use zipzi_core::document::MotionSegment;
use zipzi_editor_core::*;
use zipzi_sdk::{CompiledCurve, MotionInterpolation, MotionPreview, sample_motion_curve};

#[test]
fn multi_key_moves_snap_only_anchor_keep_rhythm_and_reject_collisions_atomically()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(true, false)?;
    let d = e.active_id().unwrap();
    e.update_timeline_view(d, |view| view.snap = true)?;
    e.begin_timeline_edit(d, "Move")?;
    let keys = vec![key(10), key(11), key(30), key(31)];
    e.update_timeline_edit(
        d,
        e.timeline_edit().unwrap().identity,
        TimelineCommand::MoveKeys {
            keys: keys.clone(),
            anchor: key(10),
            time_delta: 0.11,
            value_delta: 0.,
        },
    )?;
    let legal = e.timeline_edit().unwrap().candidate.clone();
    let track = &legal.clip().tracks[0];
    assert!((track.initial.time - 7. / 30.).abs() < 1e-6);
    assert!(((track.segments[0].end().time - track.initial.time) - (0.6 - 0.1137)).abs() < 1e-6);
    assert!(
        e.update_timeline_edit(
            d,
            e.timeline_edit().unwrap().identity,
            TimelineCommand::MoveKeys {
                keys: keys.clone(),
                anchor: key(10),
                time_delta: 1.,
                value_delta: 0.
            }
        )
        .is_err()
    );
    assert_eq!(e.timeline_edit().unwrap().candidate, legal);
    assert!(
        e.update_timeline_edit(
            d,
            e.timeline_edit().unwrap().identity,
            TimelineCommand::MoveKeys {
                keys,
                anchor: key(10),
                time_delta: 0.,
                value_delta: 0.1
            }
        )
        .is_err()
    );
    e.cancel_timeline_edit();
    assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn view_projection_is_cached_during_playback_and_modes_do_not_dirty_document() -> anyhow::Result<()>
{
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    let projection = e.timeline_projection().unwrap();
    e.toggle_animation()?;
    for time in [0.1, 0.2, 0.3] {
        e.advance_animation_clock(time);
        e.animation_request();
        assert!(Arc::ptr_eq(&projection, &e.timeline_projection().unwrap()));
    }
    e.pause_animation();
    e.set_animation_preview_mode(d, AnimationPreviewMode::Result)?;
    e.set_animation_preview_mode(d, AnimationPreviewMode::Editing)?;
    e.update_timeline_view(d, |view| {
        view.normalized = true;
        view.mode = TimelineDisplayMode::Curves;
        view.time_range = (0.3, 1.4);
    })?;
    assert!(!e.document().unwrap().modified());
    assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn editing_preview_uses_raw_clip_result_preview_matches_sdk_and_keeps_expression_schedule()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    let mut evaluator = AnimationEvaluator::default();
    let edit = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(edit.snapshot.parameters[&id(2)], 0.5);
    assert!(edit.snapshot.fired_events.is_empty());
    e.set_animation_preview_mode(d, AnimationPreviewMode::Result)?;
    let request = e.animation_request().unwrap();
    let result = evaluator.evaluate(&request, |_, _| true)?;
    let mut sdk = MotionPreview::new(request.source.materialized_snapshot().unwrap().document());
    sdk.schedule_motion(&id(4), 0.)?;
    let expected = sdk.seek(request.identity.time)?;
    assert_eq!(result.snapshot.as_ref(), expected);
    assert_ne!(result.snapshot.parameters[&id(2)], 0.5);
    let raw = e.timeline_projection().unwrap().raw_value(
        &TimelineChannelId::Parameter(id(2)),
        None,
        0.6,
    )?;
    assert_eq!(raw, 0.5);
    Ok(())
}

#[test]
fn result_seek_uses_sdk_current_cycle_and_pause_uses_presented_time() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.set_animation_preview_mode(d, AnimationPreviewMode::Result)?;
    e.toggle_animation()?;
    e.advance_animation_clock(4.5);
    let mut evaluator = AnimationEvaluator::default();
    let frame = evaluate(&mut e, &mut evaluator)?;
    let position = frame.snapshot.motion_positions[0].clone();
    assert!(position.loop_index >= 2);
    assert!(position.period > 2.);
    assert!(e.present_animation(frame.identity, frame.snapshot));
    e.advance_animation_clock(4.8);
    let stale = e.animation_request().unwrap();
    e.pause_animation();
    assert_eq!(e.animation().unwrap().time, 4.5);
    assert_eq!(e.animation().unwrap().edit_time, position.local_time);
    assert!(!e.accepts_animation(stale.identity));
    e.seek_timeline(d, 2.)?;
    let located = evaluate(&mut e, &mut evaluator)?;
    let position = located.snapshot.motion_positions[0].clone();
    assert_eq!(position.loop_index, 2);
    assert!((position.local_time - 2.).abs() < 1e-5);
    assert!(e.present_animation(located.identity, located.snapshot));
    e.set_animation_preview_mode(d, AnimationPreviewMode::Editing)?;
    e.set_animation_preview_mode(d, AnimationPreviewMode::Result)?;
    let restored = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(restored.snapshot.motion_positions[0].loop_index, 2);
    let tail = (2. + position.period) / 2.;
    e.seek_timeline(d, tail)?;
    let tail_frame = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(tail_frame.snapshot.motion_positions[0].loop_index, 2);
    assert!((tail_frame.snapshot.motion_positions[0].local_time - tail).abs() < 1e-5);
    assert!(tail_frame.snapshot.motion_positions[0].local_time > 2.);
    Ok(())
}

#[test]
fn context_changes_and_play_cancel_candidate_and_save_command_commits() -> anyhow::Result<()> {
    for context in 0..4 {
        let (mut e, _temp) = fixture(false, false)?;
        let d = e.active_id().unwrap();
        e.seek_timeline(d, 0.6)?;
        e.begin_timeline_edit(d, "Change")?;
        e.update_timeline_edit(d, e.timeline_edit().unwrap().identity, write(2, None, 0.9))?;
        let stale = e.animation_request().unwrap();
        match context {
            0 => {
                e.set_workspace(WorkspaceKind::Modeling)?;
            }
            1 => {
                e.new_document()?;
                e.activate(d)?;
            }
            2 => {
                e.select_motion(
                    d,
                    MotionSelection {
                        clip_id: id(4),
                        entry: None,
                    },
                )?;
            }
            _ => {
                e.toggle_animation()?;
            }
        }
        assert!(e.timeline_edit().is_none());
        assert!(!e.accepts_animation(stale.identity));
        assert_eq!(
            e.document()
                .unwrap()
                .motion_definition(&id(4))
                .unwrap()
                .tracks[0]
                .segments[0]
                .end()
                .value,
            0.5
        );
        assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    }
    let (mut e, temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    e.begin_timeline_edit(d, "Save")?;
    e.update_timeline_edit(d, e.timeline_edit().unwrap().identity, write(2, None, 0.9))?;
    e.execute(EditorCommand::FinishEdit)?;
    let receipt = e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .clone();
    assert_eq!(e.document().unwrap().history_lengths(), (1, 0));
    let task = e.prepare_save(d, &temp.path().join("saved.zipzi"), false)?;
    e.finish_save(&mut task.run())?;
    let reopened = Document::open(e.document().unwrap().path().unwrap())?;
    assert_eq!(reopened.motion_definition(&id(4)), Some(&receipt));
    Ok(())
}

#[test]
fn event_order_is_stable_and_invalid_edit_keeps_last_legal_candidate() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    assert_eq!(
        e.timeline_projection()
            .unwrap()
            .events
            .iter()
            .map(|event| event.value.as_str())
            .collect::<Vec<_>>(),
        vec!["B", "A"]
    );
    e.execute_timeline(
        d,
        TimelineCommand::Event {
            id: None,
            time: 0.5,
            value: "C".into(),
        },
    )?;
    let events = &e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .events;
    assert_eq!(
        events
            .iter()
            .map(|event| event.value.as_str())
            .collect::<Vec<_>>(),
        vec!["B", "A", "C"]
    );
    e.begin_timeline_edit(d, "Event")?;
    e.update_timeline_edit(
        d,
        e.timeline_edit().unwrap().identity,
        TimelineCommand::Event {
            id: Some(id(40)),
            time: 0.7,
            value: "B2".into(),
        },
    )?;
    let legal = e.timeline_edit().unwrap().candidate.clone();
    assert!(
        e.update_timeline_edit(
            d,
            e.timeline_edit().unwrap().identity,
            TimelineCommand::Event {
                id: Some(id(40)),
                time: 3.,
                value: "Bad".into()
            }
        )
        .is_err()
    );
    assert_eq!(e.timeline_edit().unwrap().candidate, legal);
    e.cancel_timeline_edit();
    Ok(())
}

#[test]
fn clipboard_regenerates_ids_preserves_internal_segments_and_paste_is_one_atomic_edit()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.reveal_timeline_keys(d, &[key(10), key(11)])?;
    assert_eq!(e.copy_timeline_selection(d)?, 2);
    e.seek_timeline(d, 0.7)?;
    e.paste_timeline(
        d,
        Some(TimelinePasteTarget {
            channel: TimelineChannelId::Parameter(id(3)),
            track_id: None,
        }),
    )?;
    let clip = e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .clone();
    let pasted = clip.tracks.last().unwrap();
    assert_eq!(pasted.initial.time, 0.7);
    assert_ne!(pasted.initial.id, id(10));
    assert!(matches!(pasted.segments[0], MotionSegment::Bezier { .. }));
    assert_eq!(e.timeline_view().unwrap().keys.len(), 2);
    assert_eq!(e.document().unwrap().history_lengths(), (1, 0));
    let old = CompiledCurve::from(&clip.tracks[0]);
    let new = CompiledCurve::from(pasted);
    for i in 0..=50 {
        let local = (0.6 - 0.1137) * i as f32 / 50.;
        assert!(
            (sample_motion_curve(&old, 0.1137 + local, false)
                - sample_motion_curve(&new, 0.7 + local, false))
            .abs()
                < 0.0003
        );
    }
    e.execute(EditorCommand::Undo)?;
    assert_eq!(
        e.document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap()
            .tracks
            .len(),
        2
    );
    e.execute(EditorCommand::Redo)?;
    assert_eq!(e.document().unwrap().motion_definition(&id(4)), Some(&clip));
    e.seek_timeline(d, 0.1137)?;
    let history = e.document().unwrap().history_lengths();
    assert!(e.paste_timeline(d, None).is_err());
    assert_eq!(e.document().unwrap().motion_definition(&id(4)), Some(&clip));
    assert_eq!(e.document().unwrap().history_lengths(), history);
    let other = e.create_timeline_motion(d)?;
    let history = e.document().unwrap().history_lengths();
    assert!(e.paste_timeline(d, None).is_err());
    assert_eq!(e.document().unwrap().history_lengths(), history);
    assert!(
        e.document()
            .unwrap()
            .motion_definition(&other)
            .unwrap()
            .tracks
            .is_empty()
    );
    Ok(())
}

#[test]
fn interpolation_batch_and_hidden_delete_never_delete_channels_or_parameters() -> anyhow::Result<()>
{
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.execute_timeline(
        d,
        TimelineCommand::Interpolation {
            keys: vec![key(10), key(11), key(12)],
            kind: MotionInterpolation::InverseStepped,
        },
    )?;
    assert_eq!(
        e.timeline_projection()
            .unwrap()
            .key(&key(10))
            .unwrap()
            .outgoing,
        Some(MotionInterpolation::InverseStepped)
    );
    e.reveal_timeline_keys(d, &[key(10), key(11)])?;
    e.update_timeline_view(d, |view| {
        view.collapsed.insert(TimelineRowId::Group(Some(id(5))));
    })?;
    let history = e.document().unwrap().history_lengths();
    e.delete_timeline_selection(d)?;
    assert_eq!(e.document().unwrap().history_lengths(), history);
    assert_eq!(e.document().unwrap().parameters().len(), 2);
    assert_eq!(
        e.document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap()
            .tracks[0]
            .keys()
            .count(),
        3
    );
    Ok(())
}
