use editor_core::*;
use kasane_core::{
    Canvas, Parameter, Part, Vec2,
    document::{
        CdiParameterEntry, CdiParameterGroup, MotionClip, MotionEvent, MotionKey, MotionPoint,
        MotionSegment, MotionTrack, MotionTrackTarget,
    },
};
use kasane_sdk::{
    AuthoringSession, CompiledCurve, MotionInterpolation, MotionPreview, sample_motion_curve,
};
use std::{collections::HashSet, sync::Arc};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn fixture(duplicate: bool, opaque: bool) -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let temp = tempfile::tempdir()?;
    let mut s = AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.))?;
    s.edit("Fixture", None, |edit| {
        for (n, runtime, name, default_value) in [(2, "ParamX", "X", 0.), (3, "ParamY", "Y", 0.3)] {
            edit.create_parameter(Parameter {
                id: id(n),
                runtime_id: runtime.into(),
                name: name.into(),
                minimum: -2.,
                maximum: 2.,
                default_value,
                ..Default::default()
            })?;
        }
        edit.create_part(Part {
            id: id(7),
            runtime_id: "Part0".into(),
            name: "Part".into(),
            ..Default::default()
        })?;
        let mut info = edit.candidate_document().display_info().clone();
        info.parameter_groups = Some(vec![
            CdiParameterGroup {
                id: id(5),
                runtime_id: "Root".into(),
                name: "Root".into(),
                parent_id: None,
                extensions: Default::default(),
            },
            CdiParameterGroup {
                id: id(6),
                runtime_id: "Child".into(),
                name: "Child".into(),
                parent_id: Some(id(5)),
                extensions: Default::default(),
            },
        ]);
        info.parameters = Some(vec![
            CdiParameterEntry::Resolved {
                parameter_id: id(3),
                group_id: Some(id(5)),
                extensions: Default::default(),
            },
            CdiParameterEntry::Resolved {
                parameter_id: id(2),
                group_id: Some(id(6)),
                extensions: Default::default(),
            },
        ]);
        edit.replace_display_info(info)?;
        let track = MotionTrack {
            id: id(8),
            target: MotionTrackTarget::Parameter {
                parameter_id: id(2),
            },
            initial: MotionKey {
                id: id(10),
                time: 0.1137,
                value: -0.8,
            },
            segments: Arc::new(vec![
                MotionSegment::Bezier {
                    control1: MotionPoint {
                        time: 0.2,
                        value: 1.2,
                    },
                    control2: MotionPoint {
                        time: 0.45,
                        value: -0.2,
                    },
                    end: MotionKey {
                        id: id(11),
                        time: 0.6,
                        value: 0.5,
                    },
                },
                MotionSegment::Linear {
                    end: MotionKey {
                        id: id(12),
                        time: 1.5,
                        value: 0.8,
                    },
                },
            ]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        };
        let mut tracks = vec![track.clone()];
        if duplicate {
            let mut second = track;
            second.id = id(9);
            second.initial.id = id(30);
            for (i, segment) in Arc::make_mut(&mut second.segments).iter_mut().enumerate() {
                segment.end_mut().id = id(31 + i as u32);
            }
            tracks.push(second);
        }
        tracks.push(MotionTrack {
            id: id(50),
            target: MotionTrackTarget::Unresolved {
                category: "Parameter".into(),
                runtime_id: "Missing".into(),
            },
            initial: MotionKey {
                id: id(51),
                time: 0.5,
                value: 0.4,
            },
            segments: Arc::new(vec![]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        });
        edit.create_motion(MotionClip {
            id: id(4),
            name: "Fixture".into(),
            duration: 2.,
            fps: 30.,
            looping: true,
            restricted_beziers: false,
            fade_in: Some(1.),
            fade_out: Some(1.),
            tracks,
            events: vec![
                MotionEvent {
                    id: id(40),
                    time: 0.5,
                    value: "B".into(),
                    extensions: Default::default(),
                },
                MotionEvent {
                    id: id(41),
                    time: 0.5,
                    value: "A".into(),
                    extensions: Default::default(),
                },
            ],
            extensions: if opaque {
                [("Future".into(), serde_json::json!(true))].into()
            } else {
                Default::default()
            },
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
    })?;
    let receipt = s.save_project(&temp.path().join("fixture.kasane"), None)?;
    let mut editor = Editor::from_document(Document::open(&receipt.manifest)?);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 256.,
        height: 256.,
        scale_factor: 1.,
    });
    let document = editor.active_id().unwrap();
    editor.select_motion(
        document,
        MotionSelection {
            clip_id: id(4),
            entry: None,
        },
    )?;
    editor.update_timeline_view(document, |view| view.snap = false)?;
    Ok((editor, temp))
}
fn key(n: u32) -> TimelineKeyRef {
    TimelineKeyRef {
        clip_id: id(4),
        track_id: if n >= 30 { id(9) } else { id(8) },
        key_id: id(n),
    }
}
fn write(channel: u32, track_id: Option<String>, value: f32) -> TimelineCommand {
    TimelineCommand::WriteValue {
        channel: TimelineChannelId::Parameter(id(channel)),
        track_id,
        selected_key: None,
        value,
    }
}
fn evaluate(
    editor: &mut Editor,
    evaluator: &mut AnimationEvaluator,
) -> anyhow::Result<AnimationEvaluated> {
    evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)
}

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
    assert!(!e.present_animation(first_frame.identity, first_frame.snapshot));
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
    let mut sdk = MotionPreview::new(request.source.snapshot.document());
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
    let task = e.prepare_save(d, &temp.path().join("saved.kasane"), false)?;
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

#[test]
fn paste_collision_on_second_track_rolls_back_the_entire_batch() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    e.execute_timeline(d, write(3, None, 0.2))?;
    let first = e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .tracks
        .last()
        .unwrap();
    let other = TimelineKeyRef {
        clip_id: id(4),
        track_id: first.id.clone(),
        key_id: first.initial.id.clone(),
    };
    e.seek_timeline(d, 1.2)?;
    e.execute_timeline(d, write(3, None, 0.5))?;
    e.reveal_timeline_keys(d, &[key(11), other])?;
    e.copy_timeline_selection(d)?;
    let before = e
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .clone();
    let history = e.document().unwrap().history_lengths();
    assert!(e.paste_timeline(d, None).is_err());
    assert_eq!(
        e.document().unwrap().motion_definition(&id(4)),
        Some(&before)
    );
    assert_eq!(e.document().unwrap().history_lengths(), history);
    Ok(())
}

#[test]
fn commit_undo_and_modes_reject_old_cpu_and_gpu_candidates() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    let mut evaluator = AnimationEvaluator::default();
    e.begin_timeline_edit(d, "Value")?;
    e.update_timeline_edit(d, e.timeline_edit().unwrap().identity, write(2, None, 0.9))?;
    let pending = evaluate(&mut e, &mut evaluator)?;
    e.finish_timeline_edit(d, e.timeline_edit().unwrap().identity)?;
    assert!(!e.present_animation(pending.identity, pending.snapshot));
    let committed = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(committed.snapshot.parameters[&id(2)], 0.9);
    e.execute(EditorCommand::Undo)?;
    assert!(!e.present_animation(committed.identity, committed.snapshot));
    let restored = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(restored.snapshot.parameters[&id(2)], 0.5);
    e.set_animation_preview_mode(d, AnimationPreviewMode::Result)?;
    assert!(!e.present_animation(restored.identity, restored.snapshot));
    Ok(())
}

#[test]
fn editing_loop_is_session_only_and_natural_end_can_restart_looping_assets() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.toggle_animation()?;
    e.advance_animation_clock(2.);
    assert!(!e.animation().unwrap().playing);
    let final_frame = evaluate(&mut e, &mut AnimationEvaluator::default())?;
    assert!(e.present_animation(final_frame.identity, final_frame.snapshot));
    assert_eq!(e.animation().unwrap().edit_time, 2.);
    e.toggle_animation()?;
    assert!(e.animation().unwrap().playing);
    assert_eq!(e.animation().unwrap().time, 0.);
    e.pause_animation();
    e.set_animation_preview_loop(d, true)?;
    e.toggle_animation()?;
    e.advance_animation_clock(2.3);
    assert!(e.animation().unwrap().playing);
    assert!((e.animation().unwrap().time - 0.3).abs() < 1e-5);
    assert!(!e.document().unwrap().modified());
    assert_eq!(e.document().unwrap().history_lengths(), (0, 0));
    assert!(
        e.document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap()
            .looping
    );
    Ok(())
}

#[test]
fn shared_parameter_layout_defensively_handles_disconnected_groups_and_reveal() {
    let groups = vec![
        ParameterGroupValue {
            id: id(5),
            name: "A".into(),
            parent_id: Some(id(6)),
        },
        ParameterGroupValue {
            id: id(6),
            name: "B".into(),
            parent_id: Some(id(5)),
        },
    ];
    let parameter = ParameterValue {
        id: id(2),
        name: "X".into(),
        group_id: Some(id(6)),
        minimum: -1.,
        maximum: 1.,
        default_value: 0.,
        value: 0.,
        decimal_places: 6,
    };
    let layout = ParameterLayout::new(&groups, &[parameter], "Ungrouped");
    assert_eq!(
        layout
            .rows()
            .iter()
            .filter(|row| matches!(row, ParameterRow::Parameter { .. }))
            .count(),
        1
    );
    let mut collapsed = HashSet::from([Some(id(5)), Some(id(6))]);
    layout.reveal(0, &mut collapsed);
    assert!(collapsed.is_empty());
}

#[test]
fn blocked_modeling_commands_preserve_candidate_and_reset_and_close_cancel_it() -> anyhow::Result<()>
{
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    e.begin_timeline_edit(d, "Value")?;
    e.update_timeline_edit(d, e.timeline_edit().unwrap().identity, write(2, None, 0.9))?;
    assert!(
        e.execute(EditorCommand::SetPreviewParameter {
            parameter_id: id(2),
            value: 0.2
        })
        .is_err()
    );
    assert!(e.timeline_edit().is_some());
    e.reset_animation()?;
    assert!(e.timeline_edit().is_none());
    assert!(e.timeline_projection().is_none());
    e.select_motion(
        d,
        MotionSelection {
            clip_id: id(4),
            entry: None,
        },
    )?;
    e.seek_timeline(d, 0.6)?;
    e.begin_timeline_edit(d, "Value")?;
    e.update_timeline_edit(d, e.timeline_edit().unwrap().identity, write(2, None, 0.9))?;
    assert_eq!(e.prepare_close(d)?, CloseDisposition::Ready);
    assert!(e.timeline_edit().is_none());
    assert!(!e.document().unwrap().modified());
    Ok(())
}

#[test]
fn model_and_part_channels_create_valid_tracks_and_pose_diagnostics_remain_visible()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.7)?;
    let projection = e.timeline_projection().unwrap();
    let part = projection
        .channel(&TimelineChannelId::PartOpacity(id(7)))
        .unwrap();
    assert!(part.editable);
    assert!(
        part.diagnostics
            .contains(&TimelineDiagnostic::PartOpacityWithoutPose)
    );
    for (channel, value) in [
        (TimelineChannelId::PartOpacity(id(7)), 0.6),
        (TimelineChannelId::Model("Opacity".into()), 0.4),
    ] {
        e.execute_timeline(
            d,
            TimelineCommand::WriteValue {
                channel,
                track_id: None,
                selected_key: None,
                value,
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
        assert_eq!(track.initial.time, 0.7);
        assert_eq!(track.keys().count(), 1);
    }
    let frame = evaluate(&mut e, &mut AnimationEvaluator::default())?;
    assert_eq!(frame.snapshot.model_opacity, 0.4);
    assert_eq!(frame.snapshot.part_opacity_channels[&id(7)], 0.6);
    assert!(
        frame
            .snapshot
            .coverage
            .iter()
            .any(|message| message.contains("has no Pose"))
    );
    Ok(())
}

#[test]
fn imported_tail_stays_visible_fps_revalidates_and_explicit_movement_can_fix_tail()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.execute_timeline(
        d,
        TimelineCommand::Timing {
            duration: 1.49,
            fps: 30.,
            looping: false,
            fade_in: Some(1.),
            fade_out: Some(1.),
        },
    )?;
    let projection = e.timeline_projection().unwrap();
    assert_eq!(projection.display_end, 1.5);
    assert!(projection.display_end > projection.duration);
    assert_eq!(projection.tracks[&id(8)].visible_keys(1.49, 1.51).len(), 1);
    let history = e.document().unwrap().history_lengths();
    assert!(
        e.execute_timeline(
            d,
            TimelineCommand::Timing {
                duration: 1.49,
                fps: 120.,
                looping: false,
                fade_in: Some(1.),
                fade_out: Some(1.)
            }
        )
        .is_err()
    );
    assert_eq!(e.document().unwrap().history_lengths(), history);
    e.seek_timeline(d, 1.5)?;
    e.execute_timeline(
        d,
        TimelineCommand::SetKey {
            key: key(12),
            time: 1.49,
            value: 0.8,
        },
    )?;
    assert_eq!(
        e.document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap()
            .tracks[0]
            .segments[1]
            .end()
            .id,
        id(12)
    );
    assert_eq!(
        e.timeline_projection().unwrap().key(&key(12)).unwrap().time,
        f64::from(1.49f32)
    );
    Ok(())
}

#[test]
fn handle_command_uses_segment_end_identity_and_allows_value_overshoot() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    let projection = e.timeline_projection().unwrap();
    assert!(projection.display_handles(&key(10)).is_none());
    let handles = projection.display_handles(&key(11)).unwrap();
    assert!(handles.horizontal_editable && handles.value_editable);
    e.execute_timeline(
        d,
        TimelineCommand::Handle {
            end: key(11),
            handle: kasane_sdk::MotionBezierHandle::Control1,
            time: 0.2,
            value: 3.,
        },
    )?;
    let clip = e.document().unwrap().motion_definition(&id(4)).unwrap();
    let MotionSegment::Bezier { control1, end, .. } = &clip.tracks[0].segments[0] else {
        panic!()
    };
    assert_eq!(control1.value, 3.);
    assert_eq!(end.id, id(11));
    assert_eq!(end.value, 0.5);
    assert_eq!(e.document().unwrap().history_lengths(), (1, 0));
    e.execute(EditorCommand::Undo)?;
    assert_eq!(
        e.timeline_projection()
            .unwrap()
            .display_handles(&key(11))
            .unwrap()
            .control1
            .value,
        1.2
    );
    Ok(())
}

#[test]
fn late_gesture_callbacks_cannot_update_or_commit_a_new_gesture_on_the_same_track()
-> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    let old = e.begin_timeline_edit(d, "Old")?;
    e.update_timeline_edit(d, old, write(2, None, 0.7))?;
    e.cancel_timeline_edit();
    let current = e.begin_timeline_edit(d, "Current")?;
    let before = e.timeline_edit().unwrap().candidate.clone();
    assert!(e.update_timeline_edit(d, old, write(2, None, 0.9)).is_err());
    assert!(e.finish_timeline_edit(d, old).is_err());
    assert_eq!(e.timeline_edit().unwrap().candidate, before);
    e.update_timeline_edit(d, current, write(2, None, 0.8))?;
    e.finish_timeline_edit(d, current)?;
    assert_eq!(
        e.document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap()
            .tracks[0]
            .segments[0]
            .end()
            .value,
        0.8
    );
    assert_eq!(e.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}
