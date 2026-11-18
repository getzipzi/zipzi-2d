use super::common::*;
use zipzi_editor_core::*;

#[test]
fn retiming_candidates_use_the_baseline_and_publish_one_mixed_edit() -> anyhow::Result<()> {
    let (mut editor, temp) = fixture(true, false)?;
    let document = editor.active_id().unwrap();
    let panel = TimelineId::new();
    editor.set_timeline_visible(panel, true);
    let binding = editor.timeline_binding(panel).unwrap();
    editor.update_timeline_view(document, |view| view.snap = true)?;
    editor.set_timeline_loop_range(binding, 0.1, 1.4)?;
    let range = editor.timeline_view().unwrap().loop_range();
    let original = editor
        .document()
        .unwrap()
        .motion_definition(&id(4))
        .unwrap();
    let keys = vec![key(10), key(11), key(30), key(31)];
    let command = |moving_time| TimelineCommand::RetimeSelection {
        keys: keys.clone(),
        events: vec![id(40)],
        fixed_end: MotionRetimeFixedEnd::Earliest,
        moving_time,
    };
    editor.seek_timeline(document, 0.4)?;
    let token = editor.begin_timeline_edit_for(binding, "Retime selection")?;
    editor.update_timeline_edit(document, token, command(0.8))?;
    let prior = editor.timeline_edit().unwrap().candidate.clone();
    assert!(
        editor
            .update_timeline_edit(document, token, command(1.8))
            .is_err()
    );
    assert_eq!(editor.timeline_edit().unwrap().candidate, prior);

    editor.update_timeline_edit(document, token, command(f64::from(0.6_f32)))?;
    assert_eq!(
        &editor.timeline_edit().unwrap().candidate,
        original.as_ref()
    );
    editor.finish_timeline_edit(document, token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));

    let token = editor.begin_timeline_edit_for(binding, "Retime selection")?;
    editor.update_timeline_edit(document, token, command(0.799))?;
    editor.update_timeline_edit(document, token, command(0.401))?;
    let after = editor.timeline_edit().unwrap().candidate.clone();
    let start = f64::from(original.tracks[0].initial.time);
    let ratio = (0.4 - start) / (f64::from(0.6f32) - start);
    assert_eq!(after.events[0].id, id(40));
    assert_eq!(after.events[0].time, (start + (0.5 - start) * ratio) as f32);
    assert_eq!(after.events[1], original.events[1]);
    assert_eq!(after.tracks[0].segments[0].end().time, 0.4);
    assert_eq!(after.tracks[1].segments[0].end().time, 0.4);
    assert_eq!(after.tracks[2], original.tracks[2]);
    let frame = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    assert_ne!(frame.identity.candidate.unwrap().revision, 0);
    assert_eq!(frame.snapshot.parameters[&id(2)], 0.5);
    editor.finish_timeline_edit(document, token)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor
            .document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap(),
        original
    );
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        editor
            .document()
            .unwrap()
            .motion_definition(&id(4))
            .unwrap()
            .as_ref(),
        &after
    );
    assert_eq!(editor.timeline_view().unwrap().loop_range(), range);
    let path = temp.path().join("retimed.zipzi");
    let save = editor.prepare_save(document, &path, false)?;
    editor.finish_save(&mut save.run())?;
    assert_eq!(
        Document::open(&path)?
            .motion_definition(&id(4))
            .unwrap()
            .as_ref(),
        &after
    );
    Ok(())
}
