use super::support;
use zipzi_editor_core::*;

fn metrics(width: f32, height: f32, scale_factor: f32) -> ViewportMetrics {
    ViewportMetrics {
        width,
        height,
        scale_factor,
    }
}
fn command(editor: &Editor, viewport: ViewportId, id: CommandId) -> Command {
    editor.command_for_viewport(
        id,
        editor.viewport_binding(viewport).unwrap(),
        CommandSource::Toolbar,
    )
}

#[test]
fn same_kind_instances_keep_navigation_metrics_limits_and_revisions_independent()
-> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    let first = ViewportId::new();
    let second = ViewportId::new();
    editor.register_primary_viewport(first, ViewportKind::Modeling)?;
    editor.register_viewport(second, ViewportKind::Modeling)?;
    let document = editor.active_id().unwrap();
    editor.set_viewport_metrics(first, metrics(800., 600., 1.));
    editor.set_viewport_metrics(second, metrics(240., 400., 2.));
    editor.preview()?;
    let first_state = editor.viewport_state(document, first).unwrap();
    editor.dispatch(command(&editor, second, CommandId::FitView))?;
    let second_state = editor.viewport_state(document, second).unwrap();
    assert_ne!(first_state.view.zoom, second_state.view.zoom);
    assert_eq!(editor.viewport_state(document, first), Some(first_state));
    let binding = editor.viewport_binding(second).unwrap();
    editor.set_viewport_zoom(binding, MAX_ZOOM, None)?;
    assert!(!editor.command_available(command(&editor, second, CommandId::ZoomIn)));
    assert!(editor.command_available(command(&editor, first, CommandId::ZoomIn)));
    editor.pan_viewport(binding, ViewportPoint { x: 16., y: -9. })?;
    editor.dispatch(command(&editor, second, CommandId::ZoomOut))?;
    assert_eq!(editor.viewport_state(document, first), Some(first_state));
    let manual = editor.viewport_state(document, second).unwrap();
    assert!(!manual.default_fit);
    editor.set_viewport_metrics(first, metrics(900., 700., 2.));
    assert_eq!(editor.viewport_state(document, second), Some(manual));
    editor.set_viewport_visible(second, false);
    assert!(
        editor
            .pan_viewport(binding, ViewportPoint { x: 100., y: 100. })
            .is_err()
    );
    assert_eq!(editor.viewport_state(document, second), Some(manual));
    editor.set_viewport_visible(second, true);
    editor.dispatch(command(&editor, second, CommandId::ResetView))?;
    assert_eq!(
        editor.viewport_state(document, second).unwrap().view,
        ViewState::default()
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn document_switches_restore_each_instance_and_freeze_the_whole_map_during_save()
-> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let a = ViewportId::new();
    let b = ViewportId::new();
    let animation = ViewportId::new();
    editor.register_primary_viewport(a, ViewportKind::Modeling)?;
    editor.register_viewport(b, ViewportKind::Modeling)?;
    editor.register_primary_viewport(animation, ViewportKind::Animation)?;
    let original = editor.active_id().unwrap();
    editor.set_viewport_metrics(a, metrics(640., 480., 1.));
    editor.set_viewport_metrics(b, metrics(300., 200., 2.));
    editor.dispatch(command(&editor, a, CommandId::ZoomIn))?;
    editor.pan_viewport(
        editor.viewport_binding(b).unwrap(),
        ViewportPoint { x: 7., y: 12. },
    )?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(metrics(512., 384., 1.));
    editor.zoom_by(-0.5);
    editor.set_workspace(WorkspaceKind::Modeling)?;
    let frozen = [a, b, animation].map(|id| editor.viewport_state(original, id).unwrap());
    let output = tempfile::tempdir()?;
    let save = editor.prepare_save(original, &output.path().join("saved"), false)?;
    for (id, state) in [a, b, animation].into_iter().zip(frozen) {
        assert_eq!(editor.viewport_state(original, id), Some(state));
    }
    let added = ViewportId::new();
    editor.register_viewport(added, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(b, metrics(700., 500., 1.));
    let other = editor.new_document()?;
    editor.pan_viewport(
        editor.viewport_binding(b).unwrap(),
        ViewportPoint { x: 99., y: -24. },
    )?;
    let other_state = editor.viewport_state(other, b).unwrap();
    editor.finish_save(&mut save.run())?;
    assert_eq!(editor.viewport_state(other, b), Some(other_state));
    editor.activate(original)?;
    for (id, state) in [a, b, animation].into_iter().zip(frozen) {
        let restored = editor.viewport_state(original, id).unwrap();
        assert_eq!(restored.view, state.view);
        assert_eq!(restored.default_fit, state.default_fit);
        assert!(restored.revision > state.revision);
    }
    assert!(editor.viewport_state(original, added).unwrap().default_fit);
    assert_eq!(editor.viewport_metrics(b), Some(metrics(700., 500., 1.)));
    editor.activate(other)?;
    assert_eq!(editor.viewport_state(other, b), Some(other_state));
    Ok(())
}

#[test]
fn failed_save_and_instance_recreation_preserve_navigation_but_reject_old_targets()
-> anyhow::Result<()> {
    let mut editor = support::new_editor()?;
    let binding = editor.primary_viewport_binding().unwrap();
    let document = binding.document;
    editor.set_zoom_at(2.5, None);
    let before = editor.viewport_state(document, binding.viewport).unwrap();
    let output = tempfile::tempdir()?;
    let blocker = output.path().join("file");
    std::fs::create_dir(&blocker)?;
    let save = editor.prepare_save(document, &blocker.join("project"), false)?;
    std::fs::remove_dir(&blocker)?;
    std::fs::write(&blocker, b"not a directory")?;
    assert_eq!(
        editor.viewport_state(document, binding.viewport),
        Some(before)
    );
    assert!(editor.finish_save(&mut save.run()).is_err());
    assert!(!editor.is_busy());
    let restored = editor.viewport_state(document, binding.viewport).unwrap();
    assert_eq!(restored.view, before.view);
    assert_eq!(restored.default_fit, before.default_fit);
    editor.unregister_viewport(binding.viewport);
    editor.register_primary_viewport(binding.viewport, ViewportKind::Modeling)?;
    assert_eq!(
        editor.viewport_state(document, binding.viewport),
        Some(restored)
    );
    assert!(editor.zoom_viewport(binding, 0.25, None).is_err());
    Ok(())
}

#[test]
fn contexts_can_change_without_changing_layout_or_overwriting_modeling_tools() -> anyhow::Result<()>
{
    let mut editor = support::new_editor()?;
    editor.execute(EditorCommand::SetTool(ToolId::Glue))?;
    let binding = editor.primary_viewport_binding().unwrap();
    editor.set_editing_context(EditingContext::Animation)?;
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);
    assert_eq!(editor.editing_context(), Some(EditingContext::Animation));
    assert_eq!(editor.active_tool(), ToolId::Glue);
    assert!(!editor.viewport_binding_valid(binding));
    assert!(
        editor
            .execute(EditorCommand::SetTool(ToolId::Select))
            .is_err()
    );
    editor.set_editing_context(EditingContext::Modeling)?;
    assert_eq!(editor.active_tool(), ToolId::Glue);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn resizing_another_instance_cannot_cancel_the_primary_modeling_candidate() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = support::editor_from_document(document);
    editor.set_canvas_metrics(metrics(512., 512., 2.));
    support::present_canvas(&mut editor)?;
    let first = editor.primary_viewport_binding().unwrap();
    let second = ViewportId::new();
    editor.register_viewport(second, ViewportKind::Modeling)?;
    let point = editor.overlay().vertices[0].position;
    let token = editor
        .begin_canvas_gesture(first, point, false, false)?
        .capture
        .unwrap();
    editor.update_canvas_gesture(
        token,
        CanvasInput::Move {
            point: ViewportPoint {
                x: point.x + 20.,
                y: point.y,
            },
            left_down: true,
        },
    )?;
    assert!(editor.gesture_active());
    editor.set_viewport_metrics(second, metrics(300., 400., 1.));
    assert!(editor.gesture_active());
    assert!(editor.viewport_interaction_valid(token));
    let second_binding = editor.viewport_binding(second).unwrap();
    assert!(!crate::support::pointer_input_for(
        &mut editor,
        second_binding,
        CanvasInput::Move {
            point,
            left_down: false
        }
    )?);
    assert!(editor.viewport_interaction_valid(token));
    editor.set_viewport_metrics(first.viewport, metrics(600., 400., 1.));
    assert!(!editor.gesture_active());
    assert!(!editor.viewport_interaction_valid(token));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn replacing_the_primary_instance_rejects_old_frames_with_matching_metrics_and_revisions()
-> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = support::editor_from_document(document);
    let measured = metrics(512., 512., 2.);
    editor.set_canvas_metrics(measured);
    support::present_canvas(&mut editor)?;
    let previous = editor.primary_viewport_binding().unwrap();
    let pending = editor.preview()?.canvas.unwrap();
    let replacement = ViewportId::new();
    editor.register_primary_viewport(replacement, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(replacement, measured);
    let next = editor.preview()?.canvas.unwrap();
    assert_eq!(next.version.viewport, pending.version.viewport);
    assert!(!editor.present_frame(pending));
    assert!(editor.present_frame(next));
    assert!(!editor.viewport_binding_valid(previous));
    Ok(())
}
