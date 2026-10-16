use super::common::*;
use editor_core::*;
use kasane_core::document::MotionSegment;
use std::{collections::HashSet, sync::Arc};

#[test]
fn scrub_cancellation_checks_owner_and_invalidates_in_flight_samples() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let first = TimelineId::new();
    let second = TimelineId::new();
    editor.set_timeline_visible(first, true);
    editor.set_timeline_visible(second, true);
    let a = editor.timeline_binding(first).unwrap();
    let b = editor.timeline_binding(second).unwrap();
    editor.scrub_timeline_for(a, 0.3)?;
    editor.scrub_timeline_for(b, 0.5)?;
    let in_flight = editor.animation_request().unwrap().identity;
    assert!(!editor.cancel_timeline_scrub_for(a));
    assert!(editor.timeline_scrub_owned(b));
    assert!(editor.animation_request().is_none());
    assert!(editor.cancel_timeline_scrub_for(b));
    let final_sample = editor.animation_request().unwrap().identity;
    assert_eq!(final_sample.time, in_flight.time);
    assert!(final_sample.acceptance_floor > in_flight.request);
    assert!(!editor.timeline_scrub_owned(b));
    assert!(!editor.cancel_timeline_scrub_for(b));
    editor.scrub_timeline_for(a, 0.4)?;
    editor.toggle_animation()?;
    assert!(!editor.cancel_timeline_scrub_for(a));
    assert!(editor.animation().unwrap().playing);
    Ok(())
}

#[test]
fn panel_bindings_isolate_candidate_and_scrub_lifecycles() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let first = TimelineId::new();
    let second = TimelineId::new();
    editor.set_timeline_visible(first, true);
    editor.set_timeline_visible(second, true);
    let a = editor.timeline_binding(first).unwrap();
    let b = editor.timeline_binding(second).unwrap();
    let saved =
        editor.command_for_timeline(CommandId::TimelineNextFrame, a, CommandSource::Toolbar);
    let edit = editor.begin_timeline_edit_for(b, "Second panel")?;
    editor.update_timeline_edit(b.document, edit, write(2, None, 0.7))?;
    editor.set_timeline_visible(first, false);
    assert!(editor.timeline_edit_owned(b, edit));
    assert!(!editor.command_available(saved));
    editor.set_timeline_visible(first, true);
    assert_ne!(editor.timeline_binding(first), Some(a));
    assert!(!editor.command_available(saved));
    editor.set_timeline_visible(second, false);
    assert!(editor.timeline_edit().is_none());
    let a = editor.timeline_binding(first).unwrap();
    editor.scrub_timeline_for(a, 0.3)?;
    assert!(editor.animation_request().is_some());
    editor.set_timeline_visible(second, true);
    editor.set_timeline_visible(second, false);
    assert!(editor.animation_request().is_none());
    editor.set_timeline_visible(first, false);
    assert!(editor.animation_request().is_some());
    editor.set_timeline_visible(first, true);
    let fresh = editor.timeline_binding(first).unwrap();
    editor.unregister_timeline(first);
    editor.set_timeline_visible(first, true);
    assert_ne!(editor.timeline_binding(first), Some(fresh));
    assert!(editor.scrub_timeline_for(fresh, 0.5).is_err());
    let fresh = editor.timeline_binding(first).unwrap();
    editor.scrub_timeline_for(fresh, 0.5)?;
    editor.toggle_animation()?;
    editor.set_timeline_visible(first, false);
    assert!(
        editor.animation().unwrap().playing,
        "ending an old scrub must not pause new playback"
    );
    Ok(())
}

#[test]
fn inspection_sources_and_dialog_tokens_survive_only_their_own_context() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    editor.set_editing_context(EditingContext::Modeling)?;
    let parameter = ObjectRef::new(ObjectKind::Parameter, id(3));
    editor.execute(EditorCommand::Navigate(parameter.clone()))?;
    let modeling = editor.inspection(EditingContext::Modeling).unwrap();
    assert_eq!(modeling.object(), Some(parameter.clone()));
    let token = editor.editing_context_token().unwrap();
    editor.end_editing(document, EditingLifecycle::WindowDeactivated)?;
    assert!(editor.editing_context_token_valid(token));
    editor.set_editing_context(EditingContext::Animation)?;
    assert!(!editor.editing_context_token_valid(token));
    assert_eq!(
        editor.active_inspection().unwrap().object(),
        Some(ObjectRef::new(ObjectKind::Motion, id(4)))
    );
    assert_eq!(
        editor
            .inspection(EditingContext::Modeling)
            .unwrap()
            .object(),
        Some(parameter)
    );
    let token = editor.editing_context_token().unwrap();
    let other = editor.new_document()?;
    editor.activate(document)?;
    assert!(!editor.editing_context_token_valid(token));
    assert_ne!(document, other);
    Ok(())
}

#[test]
fn result_fade_masks_zero_time_candidate_but_editing_samples_it_directly() -> anyhow::Result<()> {
    for mode in [AnimationPreviewMode::Result, AnimationPreviewMode::Editing] {
        let (mut editor, _assets) = fixture(false, false)?;
        let document = editor.active_id().unwrap();
        editor.set_animation_preview_mode(document, mode)?;
        editor.execute_timeline(
            document,
            TimelineCommand::SetKey {
                key: key(10),
                time: 0.,
                value: -0.8,
            },
        )?;
        editor.seek_timeline(document, 0.)?;
        let mut evaluator = AnimationEvaluator::default();
        let before = evaluate(&mut editor, &mut evaluator)?;
        let command = TimelineCommand::WriteValue {
            channel: TimelineChannelId::Parameter(id(2)),
            track_id: Some(id(8)),
            selected_key: Some(key(10)),
            value: 0.8,
        };
        let token = editor.begin_timeline_edit(document, "Initial key")?;
        editor.update_timeline_edit(document, token, command.clone())?;
        let candidate = evaluate(&mut editor, &mut evaluator)?;
        assert_eq!(
            editor.timeline_edit().unwrap().candidate.clip().tracks[0]
                .initial
                .value,
            0.8
        );
        if mode == AnimationPreviewMode::Result {
            assert_eq!(before.snapshot.parameters[&id(2)], 0.);
            assert_eq!(candidate.snapshot.parameters, before.snapshot.parameters);
        } else {
            assert!((before.snapshot.parameters[&id(2)] + 0.8).abs() < 0.00001);
            assert!((candidate.snapshot.parameters[&id(2)] - 0.8).abs() < 0.00001);
        }
        // At a positive blend weight, the same candidate reaches Full Effect.
        // Seeking cancels a live gesture, so start a fresh edit at that time.
        editor.seek_timeline(document, 0.3)?;
        let original = evaluate(&mut editor, &mut evaluator)?;
        let token = editor.begin_timeline_edit(document, "Initial key after fade begins")?;
        editor.update_timeline_edit(document, token, command)?;
        let blended = evaluate(&mut editor, &mut evaluator)?;
        editor.cancel_timeline_edit();
        let restored = evaluate(&mut editor, &mut evaluator)?;
        assert_ne!(
            blended.snapshot.parameters[&id(2)],
            restored.snapshot.parameters[&id(2)]
        );
        assert_eq!(restored.snapshot.parameters, original.snapshot.parameters);
    }
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
fn viewport_pose_cache_tracks_candidate_revisions_cancel_commit_and_undo() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.6)?;
    let mut evaluator = AnimationEvaluator::default();
    let original = evaluate(&mut e, &mut evaluator)?;
    for value in [0.8, 0.9] {
        if e.timeline_edit().is_none() {
            e.begin_timeline_edit(d, "Value")?;
        }
        e.update_timeline_edit(
            d,
            e.timeline_edit().unwrap().identity,
            write(2, None, value),
        )?;
        let candidate = evaluate(&mut e, &mut evaluator)?;
        assert_eq!(candidate.snapshot.parameters[&id(2)], value);
        assert!(!Arc::ptr_eq(&original.snapshot, &candidate.snapshot));
        let binding = e.primary_viewport_binding().unwrap();
        let (before_pan, _) = e
            .animation_viewport_request(
                binding,
                candidate.identity,
                candidate.scene.viewport_frame.canvas,
            )
            .unwrap();
        e.pan_by(ViewportPoint { x: 2., y: 3. });
        assert!(e.animation_request().is_none());
        let (panned, _) = e
            .animation_viewport_request(
                binding,
                candidate.identity,
                candidate.scene.viewport_frame.canvas,
            )
            .unwrap();
        assert_eq!(panned.content, before_pan.content);
        assert!(e.present_animation_viewport(before_pan, candidate.snapshot.clone()));
        assert!(e.present_animation_viewport(panned, candidate.snapshot));
    }
    e.cancel_timeline_edit();
    let cancelled = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(cancelled.snapshot.parameters[&id(2)], 0.5);
    e.execute_timeline(d, write(2, None, 0.8))?;
    let committed = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(committed.snapshot.parameters[&id(2)], 0.8);
    e.execute(EditorCommand::Undo)?;
    let restored = evaluate(&mut e, &mut evaluator)?;
    assert_eq!(restored.snapshot.parameters[&id(2)], 0.5);
    Ok(())
}

#[test]
fn continuous_candidate_updates_present_inflight_frames_without_rewinding_or_crossing_edits()
-> anyhow::Result<()> {
    for mode in [AnimationPreviewMode::Editing, AnimationPreviewMode::Result] {
        let (mut editor, _assets) = fixture(false, false)?;
        let document = editor.active_id().unwrap();
        editor.set_animation_preview_mode(document, mode)?;
        editor.seek_timeline(document, 0.6)?;
        let mut evaluator = AnimationEvaluator::default();
        let before_edit = evaluate(&mut editor, &mut evaluator)?;
        let token = editor.begin_timeline_edit(document, "Continuous scrub")?;
        editor.update_timeline_edit(document, token, write(2, None, 0.7))?;
        let mut pending = evaluate(&mut editor, &mut evaluator)?;
        let first = pending.clone();
        assert!(!editor.accepts_animation(before_edit.identity));
        for sample in 1..=24 {
            editor.update_timeline_edit(
                document,
                token,
                write(2, None, 0.7 + sample as f32 * 0.01),
            )?;
            let newer = evaluate(&mut editor, &mut evaluator)?;
            assert!(newer.identity.request > pending.identity.request);
            assert!(editor.animation_request().is_none());
            // Input always gets ahead of the async result, just as in the
            // native drag that used to reject every frame for seconds.
            assert!(editor.accepts_animation(pending.identity));
            assert!(editor.present_animation(pending.identity, pending.snapshot.clone()));
            assert_eq!(
                editor.animation().unwrap().presented.as_ref(),
                Some(&pending.snapshot)
            );
            assert!(!editor.present_animation(pending.identity, pending.snapshot));
            pending = newer;
        }
        assert!(editor.present_animation(pending.identity, pending.snapshot.clone()));
        assert!(!editor.present_animation(first.identity, first.snapshot));
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));

        // A failed intermediate revision must not stop the valid current one.
        editor.update_timeline_edit(document, token, write(2, None, 1.1))?;
        let obsolete = evaluate(&mut editor, &mut evaluator)?;
        editor.update_timeline_edit(document, token, write(2, None, 1.2))?;
        let latest = evaluate(&mut editor, &mut evaluator)?;
        assert!(editor.accepts_animation(obsolete.identity));
        editor.fail_animation(
            obsolete.identity,
            AnimationFailure::Other("obsolete failure".into()),
        );
        assert!(editor.animation().unwrap().error.is_none());
        let mut future = latest.identity;
        future.candidate.as_mut().unwrap().revision += 1;
        assert!(!editor.accepts_animation(future));

        editor.cancel_timeline_edit();
        assert!(!editor.accepts_animation(obsolete.identity));
        assert!(!editor.present_animation(latest.identity, latest.snapshot.clone()));
        let restored = evaluate(&mut editor, &mut evaluator)?;
        assert!(editor.present_animation(restored.identity, restored.snapshot));
        let next = editor.begin_timeline_edit(document, "Next scrub")?;
        editor.update_timeline_edit(document, next, write(2, None, 1.3))?;
        let current = evaluate(&mut editor, &mut evaluator)?;
        assert!(!editor.present_animation(latest.identity, latest.snapshot));
        assert!(editor.present_animation(current.identity, current.snapshot));
        editor.finish_timeline_edit(document, next)?;
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(
            editor
                .timeline_projection()
                .unwrap()
                .key(&key(11))
                .unwrap()
                .value,
            0.5
        );
    }
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
    e.set_animation_preview_loop(d, false)?;
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

#[test]
fn acknowledging_a_new_key_reveal_keeps_the_live_candidate_and_snapshot() -> anyhow::Result<()> {
    let (mut e, _temp) = fixture(false, false)?;
    let d = e.active_id().unwrap();
    e.seek_timeline(d, 0.4)?;
    let token = e.begin_timeline_edit(d, "Write")?;
    e.update_timeline_edit(d, token, write(3, None, 0.6))?;
    let candidate = e.timeline_edit().unwrap().candidate.clone();
    let identity = e.timeline_edit().unwrap().identity;
    let row = e.timeline_view().unwrap().reveal_row.clone().unwrap();
    let rows = e.timeline_visible_rows().unwrap();
    e.acknowledge_timeline_reveal(d, &row, 26.)?;
    assert!(e.timeline_view().unwrap().reveal_row.is_none());
    assert_eq!(e.timeline_view().unwrap().scroll, 26.);
    assert_eq!(e.timeline_edit().unwrap().identity, identity);
    assert_eq!(e.timeline_edit().unwrap().candidate, candidate);
    assert!(Arc::ptr_eq(&rows, &e.timeline_visible_rows().unwrap()));
    e.finish_timeline_edit(d, token)?;
    assert_eq!(e.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn a_slower_animation_viewport_accepts_candidate_frames_after_its_peer_has_advanced()
-> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    let fast = editor.primary_viewport_binding().unwrap();
    let second = ViewportId::new();
    editor.register_viewport(second, ViewportKind::Animation)?;
    editor.set_viewport_metrics(
        second,
        ViewportMetrics {
            width: 300.,
            height: 240.,
            scale_factor: 1.,
        },
    );
    let slow = editor.viewport_binding(second).unwrap();
    editor.seek_timeline(document, 0.6)?;
    editor.begin_timeline_edit(document, "Value")?;
    let mut evaluator = AnimationEvaluator::default();
    let mut frames = Vec::new();
    for value in [0.4, 0.6, 0.8] {
        editor.update_timeline_edit(
            document,
            editor.timeline_edit().unwrap().identity,
            write(2, None, value),
        )?;
        let content = evaluate(&mut editor, &mut evaluator)?;
        let canvas = content.scene.viewport_frame.canvas;
        let (a, _) = editor
            .animation_viewport_request(fast, content.identity, canvas)
            .unwrap();
        let (b, _) = editor
            .animation_viewport_request(slow, content.identity, canvas)
            .unwrap();
        frames.push((a, b, content.snapshot));
    }
    assert!(editor.present_animation_viewport(frames[2].0, frames[2].2.clone()));
    assert!(editor.present_animation_viewport(frames[0].1, frames[0].2.clone()));
    assert!(editor.present_animation_viewport(frames[1].1, frames[1].2.clone()));
    assert_eq!(
        editor
            .animation()
            .unwrap()
            .presented
            .as_ref()
            .unwrap()
            .parameters[&id(2)],
        0.8
    );
    editor.set_viewport_visible(fast.viewport, false);
    assert!(editor.present_animation_viewport(frames[2].1, frames[2].2.clone()));
    editor.cancel_timeline_edit();
    assert!(!editor.present_animation_viewport(frames[2].1, frames[2].2.clone()));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
