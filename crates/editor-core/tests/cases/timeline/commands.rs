use super::common::*;
use editor_core::*;

fn run(editor: &mut Editor, id: CommandId) -> anyhow::Result<()> {
    assert!(matches!(
        editor.dispatch(editor.command(id))?,
        CommandEffect::Done
    ));
    Ok(())
}

#[test]
fn user_navigation_commands_seek_and_reveal_without_creating_history() -> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    run(&mut editor, CommandId::TimelineStart)?;
    assert_eq!(editor.animation().unwrap().edit_time, 0.);
    run(&mut editor, CommandId::TimelineNextFrame)?;
    assert!((editor.animation().unwrap().edit_time - 1. / 30.).abs() < 1e-6);
    run(&mut editor, CommandId::TimelinePreviousFrame)?;
    assert_eq!(editor.animation().unwrap().edit_time, 0.);
    run(&mut editor, CommandId::TimelineNextKey)?;
    assert!((editor.animation().unwrap().edit_time - 0.1137).abs() < 1e-6);
    assert!(editor.timeline_view().unwrap().reveal_row.is_some());
    run(&mut editor, CommandId::TimelineEnd)?;
    assert_eq!(
        editor.animation().unwrap().edit_time,
        editor.timeline_projection().unwrap().duration as f32
    );
    run(&mut editor, CommandId::TimelinePreviousKey)?;
    assert_eq!(editor.animation().unwrap().edit_time, 1.5);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn timeline_user_commands_share_selection_and_history_guards() -> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    for id in [
        CommandId::TimelineDelete,
        CommandId::TimelineCopy,
        CommandId::TimelinePaste,
    ] {
        let state = editor.command_state(editor.command(id));
        assert!(!state.enabled);
        assert_eq!(
            state.disabled_reason,
            Some(CommandDisabledReason::UnavailableInContext)
        );
        assert!(editor.dispatch(editor.command(id)).is_err());
    }
    editor.update_timeline_view(document, |view| {
        view.keys.insert(key(11));
    })?;
    run(&mut editor, CommandId::TimelineCopy)?;
    assert!(
        editor
            .command_state(editor.command(CommandId::TimelinePaste))
            .enabled
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    run(&mut editor, CommandId::TimelineDelete)?;
    assert!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(11))
            .is_none()
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    run(&mut editor, CommandId::Undo)?;
    assert!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(11))
            .is_some()
    );
    editor.seek_timeline(document, 1.)?;
    run(&mut editor, CommandId::TimelinePaste)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(
        editor.timeline_projection().unwrap().tracks[&id(8)]
            .keys
            .iter()
            .any(|key| key.time == 1.)
    );
    Ok(())
}

#[test]
fn timeline_dispatch_revalidates_playback_readonly_workspace_and_document() -> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    editor.update_timeline_view(document, |view| {
        view.keys.insert(key(11));
    })?;
    let delete = editor.command(CommandId::TimelineDelete);
    run(&mut editor, CommandId::PlayPause)?;
    assert!(
        editor
            .command_state(editor.command(CommandId::PlayPause))
            .checked
    );
    assert!(!editor.command_available(delete));
    assert!(editor.dispatch(delete).is_err());
    editor.pause_animation();
    assert!(editor.command_available(delete));
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert!(!editor.command_available(delete));
    assert!(editor.dispatch(delete).is_err());
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.new_document()?;
    let state = editor.command_state(delete);
    assert_eq!(
        state.disabled_reason,
        Some(CommandDisabledReason::InvalidDocument)
    );
    assert!(editor.dispatch(delete).is_err());
    let (mut readonly, _temp) = fixture(false, true)?;
    let document = readonly.active_id().unwrap();
    readonly.update_timeline_view(document, |view| {
        view.keys.insert(key(11));
    })?;
    assert!(
        readonly
            .dispatch(readonly.command(CommandId::TimelineDelete))
            .is_err()
    );
    assert_eq!(readonly.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
