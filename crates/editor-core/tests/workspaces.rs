use editor_core::*;
mod support;

fn assert_view(actual: ViewState, expected: ViewState) {
    assert_eq!(actual.zoom, expected.zoom);
    assert_eq!(actual.pan, expected.pan);
}

#[test]
fn workspaces_preserve_modeling_pose_selection_tools_history_and_separate_views()
-> anyhow::Result<()> {
    let (document, _assets) = support::sample_document_with_binding(true)?;
    let mut editor = Editor::from_document(document);
    let first = editor.active_id().unwrap();
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.clone(),
        value: 0.75,
    })?;
    editor.execute(EditorCommand::SetTool(ToolId::Glue))?;
    editor.zoom_by(0.75);
    editor.pan_by(CanvasPoint { x: 13., y: -21. });
    let modeling = editor.view();
    let history = editor.document().unwrap().history_lengths();
    let selection = editor.document().unwrap().context().selections.clone();
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);

    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_view(editor.view(), ViewState::default());
    assert_eq!(editor.active_tool(), ToolId::Hand);
    editor.zoom_by(-0.5);
    editor.pan_by(CanvasPoint { x: -8., y: 40. });
    let animation = editor.view();
    assert!(
        editor
            .execute(EditorCommand::SetPreviewParameter {
                parameter_id: parameter,
                value: 0.
            })
            .is_err()
    );
    assert!(!editor.command_available(editor.command(CommandId::GenerateMesh)));
    assert!(!editor.command_available(editor.command(CommandId::SelectTool)));
    assert!(!editor.command_available(editor.command(CommandId::Parameters)));
    assert!(editor.command_available(editor.command(CommandId::HandTool)));
    assert!(editor.command_available(editor.command(CommandId::Save)));
    assert!(editor.command_available(editor.command(CommandId::SaveAs)));
    editor.dispatch(editor.command(CommandId::HandTool))?;

    let second = editor.new_document()?;
    assert_eq!(editor.workspace(), WorkspaceKind::Animation);
    assert_view(editor.view(), ViewState::default());
    editor.pan_by(CanvasPoint { x: 101., y: 9. });
    let second_animation = editor.view();
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert_view(editor.view(), ViewState::default());
    editor.activate(first)?;
    assert_view(editor.view(), modeling);
    assert_eq!(editor.active_tool(), ToolId::Glue);
    assert_eq!(editor.document().unwrap().parameters()[0].value, 0.75);
    assert_eq!(editor.document().unwrap().context().selections, selection);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert!(!editor.document().unwrap().modified());
    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_view(editor.view(), animation);
    editor.activate(second)?;
    assert_view(editor.view(), second_animation);
    editor.reset_view();
    assert_view(editor.view(), ViewState::default());
    Ok(())
}

#[test]
fn switching_rolls_back_a_live_gesture_and_rejects_old_frames_and_input() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    let original = editor.document().unwrap().selected_vertex_positions();
    let start = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: CanvasPoint {
            x: start.x + 20.,
            y: start.y,
        },
        left_down: true,
    })?;
    assert!(editor.gesture_active());
    assert!(editor.document().unwrap().has_pending_edit());
    let pending = editor.preview()?.canvas.unwrap();
    editor.set_workspace(WorkspaceKind::Animation)?;
    assert!(!editor.gesture_active());
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    assert!(!editor.present_frame(pending.clone()));
    assert!(!editor.accepts_preview(pending.version.request));
    assert!(editor.preview_if_needed()?.is_none());
    assert!(editor.overlay().vertices.is_empty());
    assert!(!editor.canvas_input(CanvasInput::Up { point: start })?);
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert!(!editor.present_frame(pending));
    assert!(!editor.present_frame(frame));
    let fresh = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(fresh));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn switching_commits_properties_and_shared_undo_remains_available() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    let original = editor.document().unwrap().selected_layer().unwrap().name;
    editor.execute(EditorCommand::PreviewName("Changed".into()))?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(!editor.document().unwrap().has_pending_edit());
    assert!(editor.command_available(editor.command(CommandId::Undo)));
    editor.dispatch(editor.command(CommandId::Undo))?;
    assert_eq!(
        editor.document().unwrap().selected_layer().unwrap().name,
        original
    );
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(
        editor.document().unwrap().selected_layer().unwrap().name,
        "Changed"
    );
    Ok(())
}

#[test]
fn save_completion_keeps_both_views_and_uses_the_current_workspace() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let output = tempfile::tempdir()?;
    let mut editor = Editor::from_document(document);
    editor.zoom_by(1.);
    let modeling = editor.view();
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.pan_by(CanvasPoint { x: 14., y: 30. });
    let animation = editor.view();
    let save = editor.prepare_save(
        editor.active_id().unwrap(),
        &output.path().join("model"),
        false,
    )?;
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert_view(editor.view(), modeling);
    let mut completion = save.run();
    editor.finish_save(&mut completion)?;
    assert_view(editor.view(), modeling);
    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_view(editor.view(), animation);
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn play_pause_has_one_non_repeating_space_binding_in_both_modes() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let play = CommandId::PlayPause.spec();
    assert_eq!(play.keys, &["space"]);
    assert_eq!(play.scope, CommandScope::Document);
    assert!(!play.repeat);
    for workspace in WorkspaceKind::ALL {
        editor.set_workspace(workspace)?;
        // Phase one installs the entry point; phase two supplies availability.
        assert!(!editor.command_available(editor.command(CommandId::PlayPause)));
        assert!(
            editor
                .dispatch(editor.command(CommandId::PlayPause))
                .is_err()
        );
    }
    Ok(())
}
