use super::common::*;
use std::sync::Arc;
use zipzi_editor_core::*;
use zipzi_sdk::MotionPreview;

#[test]
fn navigation_and_selection_share_rows_but_visibility_changes_rebuild_them() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let doc = editor.active_id().unwrap();
    let original = editor.timeline_visible_rows().unwrap();
    let key = editor
        .timeline_projection()
        .unwrap()
        .tracks
        .values()
        .next()
        .unwrap()
        .keys[0]
        .reference
        .clone();
    editor.update_timeline_view(doc, |view| {
        view.scroll = 52.;
        view.time_range = (0.1, 0.7);
        view.mode = TimelineDisplayMode::Curves;
        view.keys.insert(key.clone());
    })?;
    assert!(Arc::ptr_eq(
        &original,
        &editor.timeline_visible_rows().unwrap()
    ));
    assert!(editor.timeline_view().unwrap().keys.contains(&key));
    editor.update_timeline_view(doc, |view| view.search = "does not exist".into())?;
    assert!(!Arc::ptr_eq(
        &original,
        &editor.timeline_visible_rows().unwrap()
    ));
    assert!(editor.timeline_view().unwrap().keys.is_empty());
    Ok(())
}

#[test]
fn channel_value_scrubs_share_model_rows_and_unchanged_tracks_without_mutating_old_requests()
-> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    editor.seek_timeline(document, 0.6)?;
    let original = editor.timeline_projection().unwrap();
    let visible = editor.timeline_visible_rows().unwrap();
    let token = editor.begin_timeline_edit(document, "Scrub")?;
    let mut requests = Vec::new();
    for value in [0.7, 1.1, 0.2] {
        editor.update_timeline_edit(document, token, write(2, None, value))?;
        let projection = editor.timeline_projection().unwrap();
        assert!(Arc::ptr_eq(&original.rows, &projection.rows));
        assert!(Arc::ptr_eq(&original.channels, &projection.channels));
        assert!(Arc::ptr_eq(
            &original.tracks[&id(50)],
            &projection.tracks[&id(50)]
        ));
        assert!(!Arc::ptr_eq(
            &original.tracks[&id(8)],
            &projection.tracks[&id(8)]
        ));
        assert!(Arc::ptr_eq(
            &visible,
            &editor.timeline_visible_rows().unwrap()
        ));
        assert_eq!(projection.key(&key(11)).unwrap().value, value);
        assert_eq!(original.key(&key(11)).unwrap().value, 0.5);
        requests.push(editor.animation_request().unwrap());
    }
    let mut evaluator = AnimationEvaluator::default();
    for (request, value) in requests.iter().zip([0.7, 1.1, 0.2]) {
        assert!(Arc::ptr_eq(
            &requests[0].source.snapshot,
            &request.source.snapshot
        ));
        let evaluated = evaluator.evaluate(request, |_, _| true)?;
        assert_eq!(evaluated.snapshot.parameters[&id(2)], value);
        assert_eq!(
            request
                .source
                .snapshot
                .document()
                .get_motion(&id(4))
                .unwrap()
                .tracks[0]
                .segments[0]
                .end()
                .value,
            0.5
        );
    }
    // Re-evaluating an earlier immutable recipe must restore its own candidate.
    assert_eq!(
        evaluator
            .evaluate(&requests[0], |_, _| true)?
            .snapshot
            .parameters[&id(2)],
        0.7
    );
    editor.cancel_timeline_edit();
    let restored = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(restored.snapshot.parameters[&id(2)], 0.5);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn first_channel_key_updates_layout_once_then_reuses_it_during_scrubbing() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    editor.seek_timeline(document, 0.7)?;
    let original = editor.timeline_projection().unwrap();
    let token = editor.begin_timeline_edit(document, "Create and scrub")?;
    editor.update_timeline_edit(document, token, write(3, None, 0.8))?;
    let created = editor.timeline_projection().unwrap();
    let visible = editor.timeline_visible_rows().unwrap();
    let selection = editor.timeline_view().unwrap().keys.clone();
    assert!(!Arc::ptr_eq(&original.rows, &created.rows));
    assert_eq!(selection.len(), 1);
    for value in [0.9, 1.1] {
        editor.update_timeline_edit(document, token, write(3, None, value))?;
        let projection = editor.timeline_projection().unwrap();
        assert!(Arc::ptr_eq(&created.rows, &projection.rows));
        assert!(Arc::ptr_eq(
            &visible,
            &editor.timeline_visible_rows().unwrap()
        ));
        assert_eq!(editor.timeline_view().unwrap().keys, selection);
        assert_eq!(
            projection.key(selection.first().unwrap()).unwrap().value,
            value
        );
    }
    editor.finish_timeline_edit(document, token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .channel(&TimelineChannelId::Parameter(id(3)))
            .unwrap()
            .tracks
            .len(),
        0
    );
    Ok(())
}

#[test]
fn result_candidate_uses_its_immutable_overlay_and_materializes_only_once() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Result)?;
    editor.seek_timeline(document, 0.6)?;
    let token = editor.begin_timeline_edit(document, "Result candidate")?;
    let mut requests = Vec::new();
    for value in [0.7, 1.1] {
        editor.update_timeline_edit(document, token, write(2, None, value))?;
        requests.push(editor.animation_request().unwrap());
    }
    let mut evaluator = AnimationEvaluator::default();
    for request in &requests {
        let actual = evaluator.evaluate(request, |_, _| true)?;
        let snapshot = request.source.materialized_snapshot()?;
        assert!(Arc::ptr_eq(
            &snapshot,
            &request.source.materialized_snapshot()?
        ));
        assert!(!Arc::ptr_eq(&snapshot, &request.source.snapshot));
        let mut expected = MotionPreview::new(snapshot.document());
        expected.schedule_motion(&id(4), 0.)?;
        assert_eq!(
            actual.snapshot.as_ref(),
            expected.seek(request.identity.time)?
        );
    }
    assert_eq!(
        requests[0].source.candidate.as_ref().unwrap().clip().tracks[0].segments[0]
            .end()
            .value,
        0.7
    );
    assert_eq!(
        requests[1].source.candidate.as_ref().unwrap().clip().tracks[0].segments[0]
            .end()
            .value,
        1.1
    );
    editor.cancel_timeline_edit();
    let restored = editor.animation_request().unwrap();
    assert!(restored.source.candidate.is_none());
    assert!(Arc::ptr_eq(
        &restored.source.snapshot,
        &restored.source.materialized_snapshot()?
    ));
    Ok(())
}

#[test]
fn revealing_keys_clears_event_selection_and_keeps_zoom() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    editor.update_timeline_view(document, |view| {
        view.events.insert(id(40));
        view.search = "missing channel".into();
        view.time_range = (1., 1.1);
    })?;
    editor.reveal_timeline_keys(document, &[key(10)])?;
    let view = editor.timeline_view().unwrap();
    let time = editor
        .timeline_projection()
        .unwrap()
        .key(&key(10))
        .unwrap()
        .time;
    assert!(view.events.is_empty());
    assert!(view.search.is_empty());
    assert_eq!(view.keys.iter().cloned().collect::<Vec<_>>(), vec![key(10)]);
    assert!(view.time_range.0 < time && view.time_range.1 > time);
    assert!((view.time_range.1 - view.time_range.0 - 0.1).abs() < 1e-9);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn camera_navigation_preserves_candidate_identity_and_history_boundary() -> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    let original = editor
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap()
        .as_ref()
        .clone();
    let token = editor.begin_timeline_edit(document, "Drag motion key")?;
    editor.update_timeline_edit(
        document,
        token,
        TimelineCommand::SetKey {
            key: key(10),
            time: 0.1137,
            value: 0.8,
        },
    )?;
    let identity = editor.timeline_edit().unwrap().identity;
    let candidate = editor.timeline_edit().unwrap().candidate.clip().clone();
    editor.navigate_timeline_view(document, (0.2, 0.7), 26.)?;
    assert_eq!(editor.timeline_edit().unwrap().identity, identity);
    assert_eq!(editor.timeline_edit().unwrap().candidate.clip(), &candidate);
    assert_eq!(
        editor
            .document()
            .unwrap()
            .motion_definition(&id(4))
            .as_deref(),
        Some(&original)
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(
        editor
            .navigate_timeline_view(document, (-0.1, 0.7), 26.)
            .is_err()
    );
    assert_eq!(editor.timeline_edit().unwrap().identity, identity);
    editor.finish_timeline_edit(document, identity)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor
            .document()
            .unwrap()
            .motion_definition(&id(4))
            .as_deref(),
        Some(&original)
    );
    Ok(())
}
