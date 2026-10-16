use super::support;
use editor_core::*;

fn register(editor: &mut Editor, kind: ViewportKind) -> ViewportId {
    let viewport = ViewportId::new();
    editor.register_primary_viewport(viewport, kind).unwrap();
    viewport
}

#[test]
fn commands_require_registered_targets_and_fixed_viewport_capabilities() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    assert!(!editor.command_available(editor.command(CommandId::SelectTool)));
    let modeling = register(&mut editor, ViewportKind::Modeling);
    let animation = register(&mut editor, ViewportKind::Animation);
    let binding = editor.viewport_binding(modeling).unwrap();
    assert!(
        editor
            .register_primary_viewport(modeling, ViewportKind::Animation)
            .is_err()
    );
    assert!(editor.viewport_binding(animation).is_some());
    editor.dispatch(editor.command_for_viewport(
        CommandId::GlueTool,
        binding,
        CommandSource::Toolbar,
    ))?;
    assert_eq!(editor.active_tool(), ToolId::Glue);
    let mut forged = editor.command_for_viewport(CommandId::ZoomIn, binding, CommandSource::Direct);
    forged.id = CommandId::Open;
    assert!(editor.dispatch(forged).is_err());
    editor.set_workspace(WorkspaceKind::Animation)?;
    let binding = editor.viewport_binding(animation).unwrap();
    for id in [
        CommandId::SelectTool,
        CommandId::GlueTool,
        CommandId::GenerateMesh,
        CommandId::ToggleDeformers,
    ] {
        assert!(
            editor
                .dispatch(editor.command_for_viewport(id, binding, CommandSource::Direct))
                .is_err()
        );
    }
    assert!(
        editor
            .canvas_input_for(
                binding,
                CanvasInput::Down {
                    point: CanvasPoint::default(),
                    shift: false
                }
            )
            .is_err()
    );
    editor.dispatch(editor.command_for_viewport(
        CommandId::HandTool,
        binding,
        CommandSource::Toolbar,
    ))?;
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert_eq!(editor.active_tool(), ToolId::Glue);
    Ok(())
}

#[test]
fn saved_invocations_cannot_survive_document_workspace_visibility_or_recreation()
-> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let viewport = register(&mut editor, ViewportKind::Modeling);
    register(&mut editor, ViewportKind::Animation);
    let original = editor.active_id().unwrap();
    let saved = editor
        .command(CommandId::ZoomIn)
        .with_source(CommandSource::NativeMenu);
    let binding = editor.viewport_binding(viewport).unwrap();
    editor.new_document()?;
    editor.activate(original)?;
    assert_eq!(editor.active_id(), saved.document());
    assert!(editor.viewport_binding(viewport).unwrap().generation > binding.generation);
    assert!(editor.dispatch(saved).is_err());
    let saved = editor.command(CommandId::ZoomIn);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert!(editor.dispatch(saved).is_err());
    let saved = editor.command(CommandId::ZoomIn);
    editor.set_viewport_visible(viewport, false);
    assert!(editor.viewport_binding(viewport).is_none());
    editor.set_viewport_visible(viewport, true);
    assert!(editor.dispatch(saved).is_err());
    let saved = editor.command(CommandId::ZoomIn);
    editor.unregister_viewport(viewport);
    editor.register_primary_viewport(viewport, ViewportKind::Modeling)?;
    assert!(editor.dispatch(saved).is_err());
    editor.dispatch(editor.command(CommandId::ZoomIn))?;
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn saving_releases_bindings_and_completion_rejects_previous_invocations() -> anyhow::Result<()> {
    let (doc, dir) = support::sample_document()?;
    let mut editor = Editor::from_document(doc);
    let viewport = register(&mut editor, ViewportKind::Modeling);
    let document = editor.active_id().unwrap();
    editor.zoom_by(0.5);
    let before = editor.view();
    let saved = editor.command(CommandId::ZoomIn);
    let save = editor.prepare_save(document, &dir.path().join("save"), false)?;
    assert!(editor.viewport_binding(viewport).is_none());
    assert!(editor.dispatch(saved).is_err());
    editor.finish_save(&mut save.run())?;
    assert!(editor.viewport_binding(viewport).is_some());
    assert!(editor.dispatch(saved).is_err());
    assert_eq!(editor.view().zoom, before.zoom);
    editor.dispatch(editor.command(CommandId::ZoomIn))?;
    Ok(())
}

#[test]
fn delayed_cancellation_cannot_cancel_a_new_modeling_candidate() -> anyhow::Result<()> {
    let (doc, _dir) = support::sample_document()?;
    let mut editor = Editor::from_document(doc);
    let viewport = register(&mut editor, ViewportKind::Modeling);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    support::present_canvas(&mut editor)?;
    let original = editor.document().unwrap().selected_vertex_positions();
    let start = editor.overlay().vertices[1].position;
    let first = editor.begin_viewport_interaction(
        editor.viewport_binding(viewport).unwrap(),
        ViewportInteractionKind::Modeling,
    )?;
    editor.canvas_input_for(
        first.binding,
        CanvasInput::Down {
            point: start,
            shift: false,
        },
    )?;
    editor.canvas_input_for(
        first.binding,
        CanvasInput::Move {
            point: CanvasPoint {
                x: start.x + 20.,
                y: start.y + 10.,
            },
            left_down: true,
        },
    )?;
    assert!(editor.cancel_viewport_interaction(first));
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    support::present_canvas(&mut editor)?;
    let second = editor.begin_viewport_interaction(
        editor.viewport_binding(viewport).unwrap(),
        ViewportInteractionKind::Modeling,
    )?;
    editor.canvas_input_for(
        second.binding,
        CanvasInput::Down {
            point: start,
            shift: false,
        },
    )?;
    editor.canvas_input_for(
        second.binding,
        CanvasInput::Move {
            point: CanvasPoint {
                x: start.x + 30.,
                y: start.y + 15.,
            },
            left_down: true,
        },
    )?;
    let candidate = editor.document().unwrap().selected_vertex_positions();
    assert_ne!(candidate, original);
    assert!(
        editor
            .canvas_interaction_input(first, CanvasInput::Up { point: start })
            .is_err()
    );
    assert!(!editor.cancel_viewport_interaction(first));
    assert!(editor.viewport_interaction_valid(second));
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        candidate
    );
    assert!(editor.cancel_viewport_interaction(second));
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn hidden_or_deactivated_viewports_release_only_the_current_capture() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let viewport = register(&mut editor, ViewportKind::Modeling);
    let binding = editor.viewport_binding(viewport).unwrap();
    let token = editor.begin_viewport_interaction(binding, ViewportInteractionKind::Navigation)?;
    editor.set_viewport_visible(viewport, false);
    assert!(!editor.viewport_interaction_active());
    editor.set_viewport_visible(viewport, true);
    let new_binding = editor.viewport_binding(viewport).unwrap();
    let new_token =
        editor.begin_viewport_interaction(new_binding, ViewportInteractionKind::Navigation)?;
    assert!(!editor.finish_viewport_interaction(token));
    assert!(editor.viewport_interaction_valid(new_token));
    editor.end_editing(new_binding.document, EditingLifecycle::WindowDeactivated)?;
    assert!(!editor.viewport_interaction_active());
    assert!(!editor.viewport_binding_valid(new_binding));
    assert!(
        editor
            .dispatch(editor.command_for_viewport(
                CommandId::ZoomIn,
                binding,
                CommandSource::ContextMenu
            ))
            .is_err()
    );
    Ok(())
}

#[test]
fn resizing_and_exit_do_not_leave_a_revivable_capture() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let viewport = register(&mut editor, ViewportKind::Modeling);
    let token = editor.begin_viewport_interaction(
        editor.viewport_binding(viewport).unwrap(),
        ViewportInteractionKind::Modeling,
    )?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 800.,
        height: 600.,
        scale_factor: 2.,
    });
    assert!(!editor.viewport_interaction_active());
    assert!(!editor.viewport_interaction_valid(token));
    let token = editor.begin_viewport_interaction(
        editor.viewport_binding(viewport).unwrap(),
        ViewportInteractionKind::Navigation,
    )?;
    editor.request_exit()?;
    assert!(!editor.viewport_interaction_active());
    editor.cancel_exit();
    assert!(!editor.viewport_interaction_valid(token));
    assert!(!editor.viewport_binding_valid(token.binding));
    Ok(())
}
