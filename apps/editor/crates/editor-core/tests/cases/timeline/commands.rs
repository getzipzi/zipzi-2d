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
fn timeline_commands_follow_editing_context_independently_of_layout() -> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    editor.set_workspace(WorkspaceKind::Modeling)?;
    editor.set_editing_context(EditingContext::Animation)?;
    let document = editor.active_id().unwrap();
    editor.update_timeline_view(document, |view| {
        view.keys.insert(key(11));
    })?;
    run(&mut editor, CommandId::TimelineCopy)?;
    run(&mut editor, CommandId::TimelineNextFrame)?;
    run(&mut editor, CommandId::TimelineDelete)?;
    assert!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(11))
            .is_none()
    );
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);
    let saved = editor.command(CommandId::TimelineNextFrame);
    editor.set_editing_context(EditingContext::Modeling)?;
    assert!(!editor.command_available(saved));
    assert!(!editor.command_available(editor.command(CommandId::TimelineNextFrame)));
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

#[test]
fn navigation_capture_and_stale_panel_callbacks_preserve_the_new_timeline_candidate()
-> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    let viewport = ViewportId::new();
    editor.register_primary_viewport(viewport, ViewportKind::Animation)?;
    let old = editor.begin_timeline_edit(document, "Old edit")?;
    editor.cancel_timeline_edit_for(document, old);
    let current = editor.begin_timeline_edit(document, "Current edit")?;
    let current = editor.update_timeline_edit(document, current, write(2, Some(id(8)), 0.4))?;
    let candidate = editor.timeline_edit().unwrap().candidate.clone();
    let token = editor.begin_viewport_navigation(editor.viewport_binding(viewport).unwrap())?;
    assert!(editor.cancel_viewport_interaction(token));
    assert!(!editor.cancel_timeline_edit_for(document, old));
    assert_eq!(editor.timeline_edit().unwrap().identity, current);
    assert_eq!(editor.timeline_edit().unwrap().candidate, candidate);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.end_editing(document, EditingLifecycle::Save)?;
    assert!(editor.timeline_edit().is_none());
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

#[test]
fn timeline_invocations_reject_rebound_contexts_and_preview_modes() -> anyhow::Result<()> {
    let (mut editor, _temp) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    let saved = editor
        .command(CommandId::TimelineNextFrame)
        .with_source(CommandSource::ContextMenu);
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Result)?;
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Editing)?;
    assert!(editor.dispatch(saved).is_err());
    let saved = editor.command(CommandId::TimelineNextFrame);
    editor.new_document()?;
    editor.activate(document)?;
    assert!(editor.dispatch(saved).is_err());
    run(&mut editor, CommandId::TimelineNextFrame)?;
    Ok(())
}

#[test]
fn a_background_save_completion_keeps_the_active_timeline_context_and_candidate()
-> anyhow::Result<()> {
    let (mut editor, temp) = fixture(false, false)?;
    let original = editor.active_id().unwrap();
    let saving = editor.new_document()?;
    let save = editor.prepare_save(saving, &temp.path().join("background"), false)?;
    editor.activate(original)?;
    let viewport = ViewportId::new();
    editor.register_primary_viewport(viewport, ViewportKind::Animation)?;
    let invocation = editor.command(CommandId::TimelineNextFrame);
    let token = editor.begin_timeline_edit(original, "Keep candidate")?;
    let token = editor.update_timeline_edit(original, token, write(2, Some(id(8)), 0.4))?;
    let binding = editor.viewport_binding(viewport).unwrap();
    editor.finish_save(&mut save.run())?;
    assert!(editor.command_available(invocation));
    assert!(editor.viewport_binding_valid(binding));
    assert_eq!(editor.timeline_edit().unwrap().identity, token);
    Ok(())
}
