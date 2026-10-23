use super::common::*;
use editor_core::*;

#[test]
fn shared_hierarchy_filter_search_summaries_and_hidden_selection() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(true, false)?;
    let d = e.active_id().unwrap();
    let projection = e.timeline_projection().unwrap();
    assert_eq!(
        projection.channels[0].id,
        TimelineChannelId::Parameter(id(3))
    ); // CDI order.
    assert_eq!(
        projection
            .raw_value(&TimelineChannelId::Parameter(id(2)), None, 0.5)
            .is_err(),
        true
    );
    assert!(
        e.timeline_rows()
            .unwrap()
            .iter()
            .any(|row| row.id == TimelineRowId::Track(id(8)))
    );
    assert!(
        !e.timeline_rows()
            .unwrap()
            .iter()
            .any(|row| row.id == TimelineRowId::Channel(TimelineChannelId::Parameter(id(3))))
    );
    e.reveal_timeline_keys(d, &[key(10)])?;
    e.update_timeline_view(d, |view| {
        view.collapsed.insert(TimelineRowId::Group(Some(id(5))));
    })?;
    assert!(e.timeline_view().unwrap().keys.is_empty());
    let root = e
        .timeline_rows()
        .unwrap()
        .iter()
        .find(|row| row.id == TimelineRowId::Group(Some(id(5))))
        .unwrap();
    assert_eq!(root.summary.len(), 3);
    assert_eq!(root.summary[0].keys.len(), 2); // Exact duplicate times, two tracks.
    let collapsed = e.timeline_view().unwrap().collapsed.clone();
    e.update_timeline_view(d, |view| view.search = "paramy".into())?;
    assert!(
        e.timeline_rows()
            .unwrap()
            .iter()
            .any(|row| row.id == TimelineRowId::Channel(TimelineChannelId::Parameter(id(3))))
    );
    assert_eq!(e.timeline_view().unwrap().collapsed, collapsed);
    e.update_timeline_view(d, |view| view.search.clear())?;
    assert!(
        !e.timeline_rows()
            .unwrap()
            .iter()
            .any(|row| row.id == TimelineRowId::Track(id(8)))
    );
    assert!(!e.document().unwrap().modified());
    assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn new_clips_have_defaults_unique_names_and_document_clip_view_isolation() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.update_timeline_view(d, |view| {
        view.mode = TimelineDisplayMode::Curves;
        view.time_range = (0.2, 0.9);
        view.scroll = 42.;
    })?;
    let clip = e.create_timeline_motion(d)?;
    let definition = e.document().unwrap().motion_definition(&clip).unwrap();
    assert_eq!(
        (
            definition.duration,
            definition.fps,
            definition.looping,
            definition.fade_in,
            definition.fade_out
        ),
        (2., 30., false, Some(0.), Some(0.))
    );
    assert!(definition.tracks.is_empty() && definition.events.is_empty());
    assert!(
        e.timeline_rows()
            .unwrap()
            .iter()
            .any(|row| row.id == TimelineRowId::Channel(TimelineChannelId::Parameter(id(3))))
    );
    let second = e.create_timeline_motion(d)?;
    assert_ne!(
        e.document().unwrap().motion_definition(&clip).unwrap().name,
        e.document()
            .unwrap()
            .motion_definition(&second)
            .unwrap()
            .name
    );
    e.select_motion(
        d,
        MotionSelection {
            clip_id: id(4),
            entry: None,
        },
    )?;
    assert_eq!(e.timeline_view().unwrap().mode, TimelineDisplayMode::Curves);
    assert_eq!(e.timeline_view().unwrap().time_range, (0.2, 0.9));
    let other = e.new_document()?;
    assert!(e.timeline_view().is_none());
    e.activate(d)?;
    assert_eq!(e.timeline_view().unwrap().scroll, 42.);
    e.activate(other)?;
    assert!(e.timeline_projection().is_none());
    Ok(())
}

#[test]
fn first_write_creates_only_current_key_and_candidates_are_detached_cancelable()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    let original = e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .clone();
    e.seek_timeline(d, 0.7137)?;
    let mut evaluator = AnimationEvaluator::default();
    let baseline = evaluate(&mut e, &mut evaluator)?;
    assert!(e.present_animation(baseline.identity, baseline.snapshot.clone()));
    e.begin_timeline_edit(d, "Value drag")?;
    e.update_timeline_edit(d, e.timeline_edit().unwrap().identity, write(3, None, 0.8))?;
    let first = e.animation_request().unwrap();
    let first_frame = evaluator.evaluate(&first, |_, _| true)?;
    assert_eq!(first_frame.snapshot.parameters[&id(3)], 0.8);
    e.update_timeline_edit(d, e.timeline_edit().unwrap().identity, write(3, None, 0.9))?;
    assert!(e.present_animation(first_frame.identity, first_frame.snapshot));
    let candidate = e.timeline_edit().unwrap().candidate.clip();
    let track = candidate.tracks.last().unwrap();
    assert_eq!(track.keys().count(), 1);
    assert_eq!(track.initial.time, 0.7137);
    assert_eq!(e.timeline_view().unwrap().keys.len(), 1);
    assert!(
        e.timeline_view()
            .unwrap()
            .channels
            .contains(&TimelineRowId::Channel(TimelineChannelId::Parameter(id(3))))
    );
    assert_eq!(
        e.document().unwrap().motion_definition(&id(4)),
        Some(&original)
    );
    assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    let pending = e.animation_request().unwrap();
    e.cancel_timeline_edit();
    assert!(!e.accepts_animation(pending.identity));
    let restored = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(restored.snapshot.parameters[&id(3)], 0.3);
    assert!(!e.document().unwrap().modified());
    assert!(e.timeline_view().unwrap().keys.is_empty());
    Ok(())
}

#[test]
fn commit_is_one_history_step_undo_first_cancels_and_uuid_selection_survives() -> anyhow::Result<()>
{
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.reveal_timeline_keys(d, &[key(11)])?;
    e.begin_timeline_edit(d, "Move")?;
    for delta in [0.05, 0.1, 0.2] {
        e.update_timeline_edit(
            d,
            e.timeline_edit().unwrap().identity,
            TimelineCommand::MoveKeys {
                keys: vec![key(11)],
                anchor: key(11),
                time_delta: delta,
                value_delta: 0.,
            },
        )?;
    }
    assert!((e.timeline_projection().unwrap().key(&key(11)).unwrap().time - 0.8).abs() < 1e-6);
    assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    assert!(e.can_undo());
    e.execute(EditorCommand::Undo)?;
    assert!(e.timeline_edit().is_none());
    assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    e.execute_timeline(
        d,
        TimelineCommand::SetKey {
            key: key(11),
            time: 0.8,
            value: 0.6,
        },
    )?;
    assert_eq!(e.document().unwrap().history_lengths(), (1, 0));
    e.execute(EditorCommand::Undo)?;
    assert_eq!(
        e.timeline_projection()
            .unwrap()
            .key(&key(11))
            .unwrap()
            .value,
        0.5
    );
    assert!(e.timeline_view().unwrap().keys.contains(&key(11)));
    e.begin_timeline_edit(d, "No change")?;
    e.finish_timeline_edit(d, e.timeline_edit().unwrap().identity)?;
    assert_eq!(e.document().unwrap().history_lengths(), (0, 1));
    e.execute(EditorCommand::Redo)?;
    assert_eq!(
        e.timeline_projection()
            .unwrap()
            .key(&key(11))
            .unwrap()
            .value,
        0.6
    );
    Ok(())
}

#[test]
fn duplicate_targets_require_concrete_tracks_and_opaque_unresolved_are_read_only()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(true, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    assert!(e.execute_timeline(d, write(2, None, 0.9)).is_err());
    e.execute_timeline(d, write(2, Some(id(8)), 0.9))?;
    let clip = e.document().unwrap().motion_definition(&id(4)).unwrap();
    assert_eq!(clip.tracks[0].segments[0].end().value, 0.9);
    assert_eq!(clip.tracks[1].segments[0].end().value, 0.5);
    let unknown = TimelineKeyRef {
        clip_id: id(4),
        track_id: id(50),
        key_id: id(51),
    };
    assert!(
        e.execute_timeline(
            d,
            TimelineCommand::MoveKeys {
                keys: vec![unknown.clone()],
                anchor: unknown.clone(),
                time_delta: 0.,
                value_delta: 0.1
            }
        )
        .is_err()
    );
    assert!(
        e.execute_timeline(
            d,
            TimelineCommand::SetKey {
                key: unknown,
                time: 0.4,
                value: 0.3
            }
        )
        .is_err()
    );
    let (mut opaque, _temp) = fixture(false, true)?;
    let d = opaque.active_id().unwrap();
    assert!(!opaque.timeline_projection().unwrap().editable);
    assert!(opaque.begin_timeline_edit(d, "Forbidden").is_err());
    Ok(())
}

#[test]
fn explicitly_selected_subframe_key_keeps_exact_time_with_snap_enabled() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.update_timeline_view(d, |view| view.snap = true)?;
    e.seek_timeline(d, 0.1137)?;
    e.execute_timeline(
        d,
        TimelineCommand::WriteValue {
            channel: TimelineChannelId::Parameter(id(2)),
            track_id: None,
            selected_key: Some(key(10)),
            value: 0.1,
        },
    )?;
    let clip = e.document().unwrap().motion_definition(&id(4)).unwrap();
    assert_eq!(clip.tracks[0].initial.time, 0.1137);
    assert_eq!(clip.tracks[0].initial.id, id(10));
    assert_eq!(clip.tracks[0].keys().count(), 3);
    e.execute_timeline(
        d,
        TimelineCommand::InsertKey {
            channel: TimelineChannelId::Parameter(id(3)),
            track_id: None,
            selected_key: None,
        },
    )?;
    let track = e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .tracks
        .last()
        .unwrap();
    assert_eq!(track.initial.time, 0.1);
    assert_eq!(track.initial.value, 0.3);
    Ok(())
}

#[test]
fn inspector_value_and_event_text_edits_preserve_subframe_times() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.execute_timeline(
        d,
        TimelineCommand::Event {
            id: Some(id(40)),
            time: f64::from(0.1137_f32),
            value: "Original".into(),
        },
    )?;
    e.update_timeline_view(d, |view| view.snap = true)?;
    let original = e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .clone();
    let initial = &original.tracks[0].initial;
    e.execute_timeline(
        d,
        TimelineCommand::SetKey {
            key: key(10),
            time: f64::from(initial.time),
            value: 0.25,
        },
    )?;
    assert_eq!(
        e.timeline_projection().unwrap().key(&key(10)).unwrap().time,
        f64::from(initial.time)
    );
    e.execute_timeline(
        d,
        TimelineCommand::Event {
            id: Some(id(40)),
            time: f64::from(original.events[0].time),
            value: "Renamed".into(),
        },
    )?;
    let clip = e.document().unwrap().motion_definition(&id(4)).unwrap();
    assert_eq!(clip.events[0].time, original.events[0].time);
    assert_eq!(clip.events[0].value, "Renamed");
    assert_eq!(clip.tracks[0].initial.value, 0.25);
    // Explicit time edits still use frame snapping.
    e.execute_timeline(
        d,
        TimelineCommand::SetKey {
            key: key(10),
            time: 0.24,
            value: 0.25,
        },
    )?;
    e.execute_timeline(
        d,
        TimelineCommand::Event {
            id: Some(id(40)),
            time: 0.24,
            value: "Renamed".into(),
        },
    )?;
    let clip = e.document().unwrap().motion_definition(&id(4)).unwrap();
    assert_eq!(clip.tracks[0].initial.time, 7. / 30.);
    assert_eq!(clip.events[0].time, 7. / 30.);
    Ok(())
}

#[test]
fn shrinking_motion_reconciles_the_edit_head_with_the_preview_clock() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 1.9)?;
    e.execute_timeline(
        d,
        TimelineCommand::Timing {
            duration: 1.6,
            fps: 30.,
            looping: false,
            fade_in: Some(0.),
            fade_out: Some(0.),
        },
    )?;
    assert_eq!(e.animation().unwrap().time, 1.6);
    assert_eq!(e.animation().unwrap().edit_time, 1.6);
    e.execute_timeline(d, write(3, None, 0.4))?;
    let clip = e.document().unwrap().motion_definition(&id(4)).unwrap();
    assert_eq!(clip.tracks.last().unwrap().initial.time, 1.6);
    Ok(())
}
