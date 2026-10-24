use super::support;
use zipzi_editor_core::*;

fn assert_view(actual: ViewState, expected: ViewState) {
    assert_eq!(actual.zoom, expected.zoom);
    assert!((actual.pan.x - expected.pan.x).abs() < 0.0001);
    assert!((actual.pan.y - expected.pan.y).abs() < 0.0001);
}

#[test]
fn project_modes_survive_background_save_and_closing_the_active_tab() -> anyhow::Result<()> {
    let mut editor = crate::support::new_editor()?;
    support::attach_viewports(&mut editor);
    let first = editor.active_id().unwrap();
    let second = editor.new_document()?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    let output = tempfile::tempdir()?;
    let save = editor.prepare_save(second, &output.path().join("second"), false)?;
    editor.activate(first)?;
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);
    editor.finish_save(&mut save.run())?;
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);
    editor.activate(second)?;
    assert_eq!(editor.workspace(), WorkspaceKind::Animation);
    assert_eq!(editor.active_tool(), ToolId::Select);
    // Closing after an asynchronous save must restore the surviving tab's mode.
    let save = editor.prepare_save(second, &output.path().join("second"), true)?;
    editor.finish_save(&mut save.run())?;
    assert_eq!(editor.active_id(), Some(first));
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);
    let third = editor.new_document()?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.close_document(third, true)?;
    assert_eq!(editor.active_id(), Some(first));
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);
    Ok(())
}

#[test]
fn default_views_fit_measured_layouts_until_manual_navigation() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = support::editor_from_document(document);
    support::attach_viewports(&mut editor);
    let first = editor.active_id().unwrap();
    // An unmeasured canvas must not consume the default Fit behavior.
    assert!(editor.preview()?.canvas.is_none());
    let fitted = |editor: &Editor| {
        if !editor
            .document()
            .unwrap()
            .object_index()
            .objects
            .iter()
            .any(|o| o.reference.kind == ObjectKind::Mesh)
        {
            return ViewState::default();
        }
        let metrics = editor.canvas_metrics();
        let zoom = ((f64::from(metrics.width) / 208.).min(f64::from(metrics.height) / 208.) * 0.9)
            .clamp(0.04, 32.) as f32;
        ViewState {
            zoom,
            pan: ViewportPoint {
                x: -128. * zoom,
                y: 128. * zoom,
            },
        }
    };
    for workspace in WorkspaceKind::ALL {
        editor.set_workspace(workspace)?;
        for (width, height, scale_factor) in [(800., 600., 1.), (420., 280., 2.)] {
            editor.set_canvas_metrics(ViewportMetrics {
                width,
                height,
                scale_factor,
            });
            match workspace {
                WorkspaceKind::Modeling => {
                    editor.preview_if_needed()?.unwrap();
                }
                WorkspaceKind::Animation => {
                    let _ = editor.animation_request();
                }
            }
            assert_view(editor.view(), fitted(&editor));
        }
        match workspace {
            WorkspaceKind::Modeling => assert!(editor.preview_if_needed()?.is_none()),
            WorkspaceKind::Animation => assert!(editor.animation_request().is_none()),
        }
        // Explicit 100%, including a centered view, must survive later layouts.
        editor.reset_view();
        editor.set_canvas_metrics(ViewportMetrics {
            width: 600.,
            height: 450.,
            scale_factor: 2.,
        });
        match workspace {
            WorkspaceKind::Modeling => {
                editor.preview_if_needed()?.unwrap();
            }
            WorkspaceKind::Animation => {
                let _ = editor.animation_request();
            }
        }
        assert_view(editor.view(), ViewState::default());
    }
    editor.set_workspace(WorkspaceKind::Modeling)?;
    let second = editor.new_document()?;
    editor.preview_if_needed()?.unwrap();
    assert_view(editor.view(), fitted(&editor));
    editor.activate(first)?;
    editor.preview_if_needed()?.unwrap();
    assert_view(editor.view(), ViewState::default());
    editor.activate(second)?;
    editor.set_zoom_at(0.75, None);
    editor.pan_by(ViewportPoint { x: 12., y: -8. });
    let manual = editor.view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 320.,
        height: 240.,
        scale_factor: 1.,
    });
    editor.preview_if_needed()?.unwrap();
    assert_view(editor.view(), manual);
    assert!(!editor.document().unwrap().modified());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn fit_view_uses_content_and_display_scale_without_changing_other_views() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = support::editor_from_document(document);
    support::attach_viewports(&mut editor);
    assert!(!editor.command_available(editor.command(CommandId::FitView)));
    let first = editor.active_id().unwrap();
    editor.pan_by(ViewportPoint { x: 30., y: 40. });
    let modeling = editor.view();
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 420.,
        height: 280.,
        scale_factor: 2.,
    });
    editor.pan_by(ViewportPoint { x: 10., y: -40. });
    editor.dispatch(editor.command(CommandId::FitView))?;
    let zoom = ((420_f64 / 208.).min(280. / 208.) * 0.9) as f32;
    assert_view(
        editor.view(),
        ViewState {
            zoom,
            pan: ViewportPoint {
                x: -128. * zoom,
                y: 128. * zoom,
            },
        },
    );
    let fitted = editor.view();
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert_view(editor.view(), modeling);
    let second = editor.new_document()?;
    assert_view(editor.view(), ViewState::default());
    editor.activate(first)?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_view(editor.view(), fitted);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    editor.activate(second)?;
    assert_view(editor.view(), ViewState::default());
    Ok(())
}

#[test]
fn workspaces_preserve_modeling_pose_selection_tools_history_and_separate_views()
-> anyhow::Result<()> {
    let (document, _assets) = support::sample_document_with_binding(true)?;
    let mut editor = crate::support::editor_from_document(document);
    support::attach_viewports(&mut editor);
    let first = editor.active_id().unwrap();
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter.clone(),
        value: 0.75,
    })?;
    editor.execute(EditorCommand::SetTool(ToolId::Glue))?;
    editor.zoom_by(0.75);
    editor.pan_by(ViewportPoint { x: 13., y: -21. });
    let modeling = editor.view();
    let history = editor.document().unwrap().history_lengths();
    let selection = editor.document().unwrap().context().selections.clone();
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);

    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_view(editor.view(), ViewState::default());
    assert_eq!(editor.active_tool(), ToolId::Glue);
    editor.zoom_by(-0.5);
    editor.pan_by(ViewportPoint { x: -8., y: 40. });
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
    for (command, kind) in [
        (CommandId::Properties, PanelKind::Properties),
        (CommandId::Physics, PanelKind::AnimationPhysics),
        (CommandId::Pose, PanelKind::AnimationPose),
        (CommandId::AnimationTimeline, PanelKind::AnimationTimeline),
    ] {
        assert!(editor.command_available(editor.command(command)));
        assert_eq!(
            PanelKind::for_command(command, editor.workspace()),
            Some(kind)
        );
    }
    assert!(editor.command_available(editor.command(CommandId::HandTool)));
    assert!(editor.command_available(editor.command(CommandId::Save)));
    assert!(editor.command_available(editor.command(CommandId::SaveAs)));
    editor.dispatch(editor.command(CommandId::HandTool))?;

    let second = editor.new_document()?;
    assert_eq!(editor.workspace(), WorkspaceKind::Animation);
    assert_view(editor.view(), ViewState::default());
    editor.pan_by(ViewportPoint { x: 101., y: 9. });
    let second_animation = editor.view();
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert!(!editor.command_available(editor.command(CommandId::AnimationTimeline)));
    assert_eq!(
        PanelKind::for_command(CommandId::Properties, editor.workspace()),
        Some(PanelKind::Properties)
    );
    assert_eq!(
        PanelKind::for_command(CommandId::Physics, editor.workspace()),
        None
    );
    assert_eq!(
        PanelKind::for_command(CommandId::Pose, editor.workspace()),
        Some(PanelKind::Pose)
    );
    assert_view(editor.view(), ViewState::default());
    editor.activate(first)?;
    assert_eq!(editor.workspace(), WorkspaceKind::Animation);
    assert_view(editor.view(), animation);
    editor.set_workspace(WorkspaceKind::Modeling)?;
    assert_view(editor.view(), modeling);
    assert_eq!(editor.active_tool(), ToolId::Glue);
    assert_eq!(editor.document().unwrap().parameters()[0].value, 0.75);
    assert_eq!(editor.document().unwrap().context().selections, selection);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert!(!editor.document().unwrap().modified());
    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_view(editor.view(), animation);
    editor.activate(second)?;
    assert_eq!(editor.workspace(), WorkspaceKind::Modeling);
    editor.set_workspace(WorkspaceKind::Animation)?;
    assert_view(editor.view(), second_animation);
    editor.reset_view();
    assert_view(editor.view(), ViewState::default());
    Ok(())
}

#[test]
fn switching_rolls_back_a_live_gesture_and_rejects_old_frames_and_input() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = crate::support::editor_from_document(document);
    support::attach_viewports(&mut editor);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let frame = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    let original = editor.document().unwrap().selected_vertex_positions();
    let start = editor.overlay().vertices[0].position;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: ViewportPoint {
                x: start.x + 20.,
                y: start.y,
            },
            left_down: true,
        },
    )?;
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
    assert!(!crate::support::pointer_input(
        &mut editor,
        CanvasInput::Up { point: start }
    )?);
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
    let mut editor = crate::support::editor_from_document(document);
    support::attach_viewports(&mut editor);
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
    let mut editor = crate::support::editor_from_document(document);
    support::attach_viewports(&mut editor);
    editor.zoom_by(1.);
    let modeling = editor.view();
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.pan_by(ViewportPoint { x: 14., y: 30. });
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
    let mut editor = crate::support::new_editor()?;
    support::attach_viewports(&mut editor);
    let play = CommandId::PlayPause.spec();
    assert_eq!(play.keys, &["space"]);
    assert_eq!(play.scope, CommandScope::Document);
    assert!(!play.repeat);
    for workspace in WorkspaceKind::ALL {
        editor.set_workspace(workspace)?;
        // A blank document has no playable motion in either workspace.
        assert!(!editor.command_available(editor.command(CommandId::PlayPause)));
        assert!(
            editor
                .dispatch(editor.command(CommandId::PlayPause))
                .is_err()
        );
    }
    Ok(())
}
