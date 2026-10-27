use super::support;
use zipzi_editor_core::*;

fn register(editor: &mut Editor, kind: ViewportKind) -> ViewportId {
    let viewport = ViewportId::new();
    editor.register_primary_viewport(viewport, kind).unwrap();
    viewport
}

#[test]
fn pointer_release_finishes_the_candidate_and_capture_in_one_core_call() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    let viewport = register(&mut editor, ViewportKind::Modeling);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    support::present_canvas(&mut editor)?;
    let binding = editor.viewport_binding(viewport).unwrap();
    let start = editor.overlay().vertices[0].position;
    let original = editor.document().unwrap().selected_vertex_positions();
    let down = editor.begin_canvas_gesture(binding, start, false, false, false)?;
    let token = down.capture.unwrap();
    assert!(!down.model_changed);

    assert!(!editor.document().unwrap().has_pending_edit());
    let end = ViewportPoint {
        x: start.x + 20.,
        y: start.y + 10.,
    };
    let update = editor.update_canvas_gesture(
        token,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    assert!(update.model_changed);
    assert_eq!(update.capture, Some(token));
    let candidate = editor.document().unwrap().selected_vertex_positions();
    assert!(
        editor
            .update_canvas_gesture(
                token,
                CanvasInput::Down {
                    primary: false,
                    point: start,
                    shift: false,
                    alt: false,
                }
            )
            .is_err()
    );
    assert!(editor.viewport_interaction_valid(token));
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        candidate
    );
    let release = editor.update_canvas_gesture(token, CanvasInput::Up { point: end })?;
    assert!(release.capture.is_none());
    assert!(!editor.viewport_interaction_active());
    assert!(!editor.gesture_active());
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_ne!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(
        editor
            .update_canvas_gesture(token, CanvasInput::Up { point: start })
            .is_err()
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    Ok(())
}

#[test]
fn cancelled_lost_or_abandoned_captures_release_the_candidate_without_history() -> anyhow::Result<()>
{
    for ending in 0..4 {
        let (document, _assets) = support::sample_document()?;
        let mut editor = Editor::from_document(document);
        let viewport = register(&mut editor, ViewportKind::Modeling);
        editor.set_canvas_metrics(ViewportMetrics {
            width: 512.,
            height: 512.,
            scale_factor: 2.,
        });
        support::present_canvas(&mut editor)?;
        let binding = editor.viewport_binding(viewport).unwrap();
        let start = editor.overlay().vertices[0].position;
        let original = editor.document().unwrap().selected_vertex_positions();
        let token = editor
            .begin_canvas_gesture(binding, start, false, false, false)?
            .capture
            .unwrap();
        let end = ViewportPoint {
            x: start.x + 20.,
            y: start.y,
        };
        editor.update_canvas_gesture(
            token,
            CanvasInput::Move {
                point: end,
                left_down: true,
            },
        )?;
        match ending {
            0 => {
                let update = editor.update_canvas_gesture(token, CanvasInput::Cancel)?;
                assert!(update.model_changed);
                assert!(update.capture.is_none());
            }
            1 => {
                let update = editor.update_canvas_gesture(
                    token,
                    CanvasInput::Move {
                        point: end,
                        left_down: false,
                    },
                )?;
                assert!(update.model_changed);
                assert!(update.capture.is_none());
            }
            2 => {
                assert!(editor.cancel_viewport_interaction(token));
            }
            _ => {
                assert!(
                    editor
                        .update_canvas_gesture(
                            token,
                            CanvasInput::Move {
                                point: ViewportPoint { x: f32::NAN, y: 0. },
                                left_down: true,
                            }
                        )
                        .is_err()
                );
            }
        }
        assert!(!editor.viewport_interaction_active(), "ending {ending}");
        assert!(!editor.gesture_active(), "ending {ending}");
        assert!(
            !editor.document().unwrap().has_pending_edit(),
            "ending {ending}"
        );
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions(),
            original,
            "ending {ending}"
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}

#[test]
fn command_cancellation_releases_capture_and_restores_hover_without_ui() -> anyhow::Result<()> {
    for drag in [false, true] {
        for ending in 0..3 {
            let (document, _assets) = support::sample_document()?;
            let mut editor = Editor::from_document(document);
            let viewport = register(&mut editor, ViewportKind::Modeling);
            editor.set_canvas_metrics(ViewportMetrics {
                width: 512.,
                height: 512.,
                scale_factor: 2.,
            });
            support::present_canvas(&mut editor)?;
            let binding = editor.viewport_binding(viewport).unwrap();
            let start = editor.overlay().vertices[0].position;
            let original = editor.document().unwrap().selected_vertex_positions();
            let token = editor
                .begin_canvas_gesture(binding, start, false, false, false)?
                .capture
                .unwrap();
            if drag {
                editor.update_canvas_gesture(
                    token,
                    CanvasInput::Move {
                        point: ViewportPoint {
                            x: start.x + 20.,
                            y: start.y,
                        },
                        left_down: true,
                    },
                )?;
            }
            // Validation failures that precede cancellation must retain ownership.
            let candidate = editor.document().unwrap().selected_vertex_positions();
            let object = editor.document().unwrap().context().selections[0].clone();
            assert!(
                editor
                    .execute(EditorCommand::SetObjectProperty {
                        object,
                        field: "missing-field".into(),
                        value: serde_json::json!(0),
                    })
                    .is_err()
            );
            assert!(editor.viewport_interaction_valid(token));
            assert!(editor.gesture_active());
            assert_eq!(
                editor.document().unwrap().selected_vertex_positions(),
                candidate
            );
            match ending {
                0 => editor.execute(EditorCommand::SetTool(ToolId::Lasso))?,
                // There is no committed history, but Undo still cancels the gesture.
                1 => assert!(editor.execute(EditorCommand::Undo).is_err()),
                // This command fails after the session has cancelled its gesture.
                _ => assert!(
                    editor
                        .execute(EditorCommand::Select("missing-mesh".into()))
                        .is_err()
                ),
            }
            assert!(!editor.gesture_active());
            assert!(!editor.viewport_interaction_active());
            assert!(editor.current_viewport_interaction().is_none());
            assert!(!editor.viewport_interaction_valid(token));
            assert!(editor.viewport_interaction_hints(token).is_empty());
            assert!(!editor.document().unwrap().has_pending_edit());
            assert_eq!(
                editor.document().unwrap().selected_vertex_positions(),
                original
            );
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            assert!(
                editor
                    .update_canvas_gesture(token, CanvasInput::Up { point: start })
                    .is_err()
            );

            support::present_canvas(&mut editor)?;
            let start = editor.overlay().vertices[0].position;
            editor.hover_canvas(binding, Some(start))?;
            assert!(editor.overlay().vertices[0].hovered);
            let replacement = editor
                .begin_canvas_gesture(binding, start, false, false, false)?
                .capture
                .unwrap();
            assert!(!editor.cancel_viewport_interaction(token));
            assert!(editor.viewport_interaction_valid(replacement));
            editor.update_canvas_gesture(replacement, CanvasInput::Up { point: start })?;
        }
    }
    Ok(())
}

#[test]
fn modeling_commands_preserve_navigation_capture() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let viewport = register(&mut editor, ViewportKind::Modeling);
    let token = editor.begin_viewport_navigation(editor.viewport_binding(viewport).unwrap())?;
    editor.execute(EditorCommand::SetTool(ToolId::Lasso))?;
    assert!(editor.viewport_interaction_valid(token));
    assert_eq!(editor.current_viewport_interaction(), Some(token));
    Ok(())
}

#[test]
fn selection_release_changes_selection_without_evaluating_or_opening_an_edit() -> anyhow::Result<()>
{
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    let viewport = register(&mut editor, ViewportKind::Modeling);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    support::present_canvas(&mut editor)?;
    let binding = editor.viewport_binding(viewport).unwrap();
    let evaluations = editor.evaluation_count();
    let vertex = editor.overlay().vertices[0].position;
    let token = editor
        .begin_canvas_gesture(binding, vertex, false, false, false)?
        .capture
        .unwrap();
    editor.update_canvas_gesture(token, CanvasInput::Up { point: vertex })?;
    assert!(
        editor.overlay().vertices[0].hovered,
        "finishing a click preserves its hover"
    );
    assert!(!editor.viewport_interaction_active());
    let blank = ViewportPoint {
        x: -1000.,
        y: -1000.,
    };
    let token = editor
        .begin_canvas_gesture(binding, blank, false, false, false)?
        .capture
        .unwrap();
    let update = editor.update_canvas_gesture(token, CanvasInput::Up { point: blank })?;

    assert!(!update.model_changed);
    assert!(update.capture.is_none());
    assert!(editor.document().unwrap().context().selections.is_empty());
    assert!(!editor.document().unwrap().has_pending_edit());
    assert!(editor.preview_if_needed()?.is_none());
    assert_eq!(editor.evaluation_count(), evaluations);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
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
        CommandId::LassoTool,
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
        crate::support::pointer_input_for(
            &mut editor,
            binding,
            CanvasInput::Down {
                primary: false,
                point: ViewportPoint::default(),
                shift: false,
                alt: false,
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
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    support::present_canvas(&mut editor)?;
    let original = editor.document().unwrap().selected_vertex_positions();
    let start = editor.overlay().vertices[1].position;
    let first = editor
        .begin_canvas_gesture(
            editor.viewport_binding(viewport).unwrap(),
            start,
            false,
            false,
            false,
        )?
        .capture
        .unwrap();
    crate::support::pointer_input_for(
        &mut editor,
        first.binding,
        CanvasInput::Move {
            point: ViewportPoint {
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
    let second = editor
        .begin_canvas_gesture(
            editor.viewport_binding(viewport).unwrap(),
            start,
            false,
            false,
            false,
        )?
        .capture
        .unwrap();
    crate::support::pointer_input_for(
        &mut editor,
        second.binding,
        CanvasInput::Move {
            point: ViewportPoint {
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
            .update_canvas_gesture(first, CanvasInput::Up { point: start })
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
    let token = editor.begin_viewport_navigation(binding)?;
    editor.set_viewport_visible(viewport, false);
    assert!(!editor.viewport_interaction_active());
    editor.set_viewport_visible(viewport, true);
    let new_binding = editor.viewport_binding(viewport).unwrap();
    let new_token = editor.begin_viewport_navigation(new_binding)?;
    assert!(!editor.cancel_viewport_interaction(token));
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
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    support::present_canvas(&mut editor)?;
    let token = editor
        .begin_canvas_gesture(
            editor.viewport_binding(viewport).unwrap(),
            ViewportPoint { x: 5., y: 5. },
            false,
            false,
            false,
        )?
        .capture
        .unwrap();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 800.,
        height: 600.,
        scale_factor: 2.,
    });
    assert!(!editor.viewport_interaction_active());
    assert!(!editor.viewport_interaction_valid(token));
    let token = editor.begin_viewport_navigation(editor.viewport_binding(viewport).unwrap())?;
    editor.request_exit()?;
    assert!(!editor.viewport_interaction_active());
    editor.cancel_exit();
    assert!(!editor.viewport_interaction_valid(token));
    assert!(!editor.viewport_binding_valid(token.binding));
    Ok(())
}
