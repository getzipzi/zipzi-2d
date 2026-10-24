use super::setup;
use crate::actions::RunCommand;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, InputEvent as _, TestAppContext};
use zipzi_editor_core::{CommandId, CommandSource};

#[gpui_kit::test]
fn saved_menu_and_toolbar_invocations_reject_a_rebound_document(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (menu, toolbar, original) = harness.view.read_with(cx, |view, _| {
        (
            view.editor
                .command(CommandId::ZoomIn)
                .with_source(CommandSource::NativeMenu),
            view.editor
                .command(CommandId::GlueTool)
                .with_source(CommandSource::Toolbar),
            view.editor.active_id().unwrap(),
        )
    });
    let (before, tool) = cx
        .update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor.new_document().unwrap();
                view.editor.activate(original).unwrap();
                view.refresh_document(window, cx);
            });
            window.render_frame(cx);
            let before = harness.view.read(cx).editor.view();
            let tool = harness.view.read(cx).editor.active_tool();
            window.dispatch_action(Box::new(RunCommand::targeted(menu)), cx);
            window.dispatch_action(Box::new(RunCommand::targeted(toolbar)), cx);
            (before, tool)
        })
        .unwrap();
    // GPUI dispatches actions after the window update is released.
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.view().zoom, before.zoom);
        assert_eq!(view.editor.active_tool(), tool);
    });
    cx.update_window(harness.window, |_, window, cx| {
        let fresh = harness
            .view
            .read(cx)
            .editor
            .command(CommandId::ZoomIn)
            .with_source(CommandSource::NativeMenu);
        window.dispatch_action(Box::new(RunCommand::targeted(fresh)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.view().zoom > before.zoom);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn an_old_canvas_blur_cannot_release_a_replacement_capture(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let replacement = cx
        .update_window(harness.window, |_, window, cx| {
            let position = window.find("canvas-body").bounds().center();
            window.dispatch_event(
                gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Middle,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            let replacement = harness.view.update(cx, |view, _| {
                assert!(view.editor.viewport_interaction_active());
                let frame = view.editor.preview().unwrap().canvas.unwrap();
                assert!(view.editor.present_frame(frame));
                view.editor
                    .begin_canvas_gesture(
                        view.editor.primary_viewport_binding().unwrap(),
                        view.editor.overlay().vertices[0].position,
                        false,
                        false,
                    )
                    .unwrap()
                    .capture
                    .unwrap()
            });
            let focus = harness.view.read(cx).focus.clone();
            window.focus(&focus, cx);
            replacement
        })
        .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.viewport_interaction_valid(replacement))
    });
    harness.view.update(cx, |view, _| {
        assert!(view.editor.cancel_viewport_interaction(replacement));
    });
}

#[gpui_kit::test]
fn switching_dock_tabs_unbinds_hidden_viewports_and_rejects_old_menus(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::{DockLayout, PanelId, panel_handle};
    let harness = setup(cx);
    harness.view.update(cx, |view, _| view.editor.reset_view());
    let saved = harness
        .view
        .read_with(cx, |view, _| view.editor.command(CommandId::ZoomIn));
    let (area, canvas, properties) = harness.view.read_with(cx, |view, _| {
        let dock = view.dock.as_ref().unwrap();
        (
            dock.area.clone(),
            dock.viewport_panel(),
            dock.properties.clone(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| {
            area.remove_panel(properties.clone(), window, cx)
        });
        let layout = DockLayout::tabs()
            .panel_view(canvas.clone(), cx)
            .panel_view(panel_handle(properties.clone()), cx);
        area.update(cx, |area, cx| {
            area.set_center(layout, window, cx);
            area.select_panel(PanelId::from(properties.entity_id()), window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.primary_viewport_binding().is_none());
        assert!(!view.editor.command_available(saved));
    });
    let zoom = harness
        .view
        .read_with(cx, |view, _| view.editor.view().zoom);
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| {
            area.select_panel(canvas.panel_id(cx), window, cx)
        });
        window.render_frame(cx);
        window.dispatch_action(Box::new(RunCommand::targeted(saved)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.primary_viewport_binding().is_some());
        assert!(!view.editor.command_available(saved));
        assert_eq!(view.editor.view().zoom, zoom);
    });
}

#[gpui_kit::test]
fn targeted_navigation_actions_update_only_their_instance(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let ids = cx
        .update_window(harness.window, |_, window, cx| {
            let ids = harness.view.update(cx, |view, cx| {
                view.install_viewport_scene(
                    &[zipzi_editor_core::ViewportKind::Modeling; 2],
                    window,
                    cx,
                )
            });
            window.render_frame(cx);
            ids
        })
        .unwrap();
    let (command, before, document) = harness.view.read_with(cx, |view, _| {
        let document = view.editor.active_id().unwrap();
        let command = view.editor.command_for_viewport(
            CommandId::ZoomIn,
            view.editor.viewport_binding(ids[1]).unwrap(),
            CommandSource::Toolbar,
        );
        (
            command,
            view.editor.viewport_state(document, ids[0]).unwrap(),
            document,
        )
    });
    let second = harness.view.read_with(cx, |view, _| {
        view.editor
            .viewport_state(document, ids[1])
            .unwrap()
            .view
            .zoom
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_action(Box::new(RunCommand::targeted(command)), cx)
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.viewport_state(document, ids[0]), Some(before));
        assert_eq!(
            view.editor
                .viewport_state(document, ids[1])
                .unwrap()
                .view
                .zoom,
            second + 0.25
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn viewport_render_failures_stay_with_their_owner_and_stale_errors_are_ignored(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let ids = view.install_viewport_scene(
                &[zipzi_editor_core::ViewportKind::Modeling; 2],
                window,
                cx,
            );
            for state in view.viewport_frames.values_mut() {
                state.error = None;
            }
            let binding = view.editor.viewport_binding(ids[0]).unwrap();
            let old = view.editor.preview_for(binding).unwrap();
            let error = |revision, text: &str| crate::preview::Frame {
                binding: Some(binding),
                trace_id: 0,
                revision,
                canvas: None,
                animation: None,
                stats: (0, 0, 0),
                image: Err(text.to_owned()),
            };
            view.receive_viewport_frame(error(old.revision, "first viewport failed"), cx);
            assert_eq!(
                view.viewport_error(ids[0]).map(String::as_str),
                Some("first viewport failed")
            );
            assert!(view.viewport_error(ids[1]).is_none());
            view.editor
                .pan_viewport(binding, zipzi_editor_core::ViewportPoint { x: 3., y: 0. })
                .unwrap();
            let newer = view.editor.preview_for(binding).unwrap();
            view.receive_viewport_frame(error(old.revision, "stale error"), cx);
            assert_eq!(
                view.viewport_error(ids[0]).map(String::as_str),
                Some("first viewport failed")
            );
            view.receive_viewport_frame(error(newer.revision, "new error"), cx);
            assert_eq!(
                view.viewport_error(ids[0]).map(String::as_str),
                Some("new error")
            );
            assert!(view.viewport_error(ids[1]).is_none());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn local_shortcuts_follow_the_focused_instance_in_a_dual_viewport_scene(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let ids = cx
        .update_window(harness.window, |_, window, cx| {
            let ids = harness.view.update(cx, |view, cx| {
                view.install_viewport_scene(
                    &[zipzi_editor_core::ViewportKind::Modeling; 2],
                    window,
                    cx,
                )
            });
            window.render_frame(cx);
            ids
        })
        .unwrap();
    let before = harness.view.read_with(cx, |view, _| {
        ids.iter()
            .map(|&id| {
                view.editor
                    .viewport_state(view.editor.active_id().unwrap(), id)
                    .unwrap()
                    .view
            })
            .collect::<Vec<_>>()
    });
    cx.update_window(harness.window, |_, window, cx| {
        let surface = harness.view.read(cx).viewport_surface(ids[1], cx).unwrap();
        window.focus(&surface.read(cx).focus.clone(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(harness.view.read(cx).focused_viewport, Some(ids[1]));
        window.dispatch_action(Box::new(RunCommand::new(CommandId::ZoomIn)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.active_id().unwrap();
        assert_eq!(
            view.editor.viewport_state(document, ids[0]).unwrap().view,
            before[0]
        );
        assert_eq!(
            view.editor
                .viewport_state(document, ids[1])
                .unwrap()
                .view
                .zoom,
            before[1].zoom + 0.25
        );
    });
}
