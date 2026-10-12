use super::setup;
use crate::docking::LayoutConfig;
use editor_core::{CommandId, ToolId, WorkspaceKind};
use gpui_kit::component::dock::{DockPlacement, PanelId};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ScrollDelta, ScrollWheelEvent, TestAppContext, point, px,
};

#[gpui_kit::test]
fn switching_keeps_document_canvas_identity_and_modeling_tool(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (document, canvas) = harness.view.read_with(cx, |view, _| {
        (
            view.editor.active_id(),
            view.dock.as_ref().unwrap().canvas.entity_id(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_glue", cx);
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        assert!(window.try_find("tool_select").is_none());
        assert!(window.try_find("tool_glue").is_none());
        assert!(window.try_find("tool_generate_mesh").is_none());
        assert!(window.try_find("tool_hand").is_some());
        assert!(window.try_find("animation-playback-bar").is_some());
        assert!(window.try_find("AnimationMotions").is_some());
        assert!(window.try_find("AnimationInfo").is_some());
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.workspace(), WorkspaceKind::Animation);
            assert_eq!(view.editor.active_id(), document);
            assert_eq!(view.editor.document_count(), 1);
            assert_eq!(view.dock.as_ref().unwrap().canvas.entity_id(), canvas);
            assert!(view.frame.is_none());
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        window.click("animation-play-pause", cx);
        window.press("space", cx);
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        assert!(window.try_find("animation-playback-bar").is_none());
        assert!(window.try_find("tool_select").is_some());
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.workspace(), WorkspaceKind::Modeling);
            assert_eq!(view.editor.active_id(), document);
            assert_eq!(view.editor.active_tool(), ToolId::Glue);
            assert_eq!(view.dock.as_ref().unwrap().canvas.entity_id(), canvas);
            assert!(!view.editor.document().unwrap().modified());
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn middle_button_trackpad_and_hand_navigation_work_in_both_workspaces(cx: &mut TestAppContext) {
    let harness = setup(cx);
    for workspace in [WorkspaceKind::Modeling, WorkspaceKind::Animation] {
        cx.update_window(harness.window, |_, window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.switch_workspace(workspace, window, cx));
            window.render_frame(cx);
            let from = window.find("canvas-body").bounds().center();
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Middle,
                    position: from,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            window.dispatch_event(
                MouseMoveEvent {
                    position: from + point(px(40.), px(20.)),
                    pressed_button: Some(MouseButton::Middle),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Middle,
                    position: from + point(px(40.), px(20.)),
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            window.dispatch_event(
                ScrollWheelEvent {
                    position: from,
                    delta: ScrollDelta::Pixels(point(px(10.), px(-5.))),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.click("tool_hand", cx);
            window.render_frame(cx);
            let from = window.find("canvas-body").bounds().center();
            window.drag(from, from + point(px(12.), px(8.)), cx);
            harness.view.read_with(cx, |view, _| {
                assert_eq!(
                    view.editor.view().pan,
                    editor_core::CanvasPoint { x: 62., y: 23. }
                );
                assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
                assert!(!view.editor.document().unwrap().modified());
            });
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn old_pan_and_wheel_callbacks_cannot_continue_after_switching(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let from = window.find("canvas-body").bounds().center();
        window.dispatch_event(
            ScrollWheelEvent {
                position: from,
                delta: ScrollDelta::Lines(point(0., 4.)),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::ModelingWorkspace, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        assert_eq!(harness.view.read(cx).editor.view().zoom, 1.);
        let from = window.find("canvas-body").bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Middle,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx)
        });
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: from + point(px(100.), px(20.)),
                pressed_button: Some(MouseButton::Middle),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            harness.view.read(cx).editor.view().pan,
            editor_core::CanvasPoint::default()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn layouts_are_independent_and_pending_reset_captures_its_workspace(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let store = harness.view.read_with(cx, |_, cx| {
        cx.global::<crate::settings::Settings>().store.clone()
    });
    let mesh = harness.view.read_with(cx, |view, _| {
        PanelId::from(view.dock.as_ref().unwrap().mesh_tree.entity_id())
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.simulate_next_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::MeshTree, window, cx)
        });
        window.render_frame(cx);
        assert!(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .area
                .read(cx)
                .panel(mesh)
                .is_none()
        );
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::ResetLayout, window, cx)
        });
        // Switch before the reset's measurement callback. It must not write
        // the Modeling layout into the Animation config (or vice versa).
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::ModelingWorkspace, window, cx)
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(
                view.dock
                    .as_ref()
                    .unwrap()
                    .area
                    .read(cx)
                    .panel(mesh)
                    .is_none()
            );
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        harness.view.read_with(cx, |view, cx| {
            let area = view.dock.as_ref().unwrap().area.read(cx);
            assert!(area.layout(DockPlacement::Left).is_none());
            crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Animation).unwrap();
        });
    })
    .unwrap();
    let modeling = store
        .read::<LayoutConfig>("layout-modeling.json")
        .unwrap()
        .unwrap();
    let animation = store
        .read::<LayoutConfig>("layout-animation.json")
        .unwrap()
        .unwrap();
    assert_eq!(modeling.workspace, WorkspaceKind::Modeling);
    assert_eq!(animation.workspace, WorkspaceKind::Animation);
    crate::docking::validate_layout(&modeling.layout, WorkspaceKind::Modeling).unwrap();
    crate::docking::validate_layout(&animation.layout, WorkspaceKind::Animation).unwrap();
    assert_ne!(modeling.layout, animation.layout);
    assert!(!store.root().join("layout.json").exists());
}

#[gpui_kit::test]
fn invalid_animation_layout_falls_back_without_resetting_modeling(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let store = harness.view.read_with(cx, |_, cx| {
        cx.global::<crate::settings::Settings>().store.clone()
    });
    std::fs::write(store.root().join("layout-animation.json"), "{").unwrap();
    // The retired layout is ignored, even when malformed.
    std::fs::write(store.root().join("layout.json"), "{").unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.simulate_next_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::MeshTree, window, cx)
        });
        window.render_frame(cx);
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        assert!(harness.view.read(cx).edit_error.is_some());
        assert!(window.try_find("animation-playback-bar").is_some());
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            assert!(
                dock.area
                    .read(cx)
                    .panel(PanelId::from(dock.mesh_tree.entity_id()))
                    .is_none()
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn workspace_and_layout_restore_in_a_new_window_and_transport_fits_small_windows(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{Bounds, WindowBounds, WindowOptions, size};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.simulate_next_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::MeshTree, window, cx)
        });
        window.render_frame(cx);
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationParameters, window, cx);
            view.run_command(CommandId::Expressions, window, cx);
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
    })
    .unwrap();
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(640.), px(480.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| crate::EditorView::new(window, cx)),
        )
        .unwrap()
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).editor.workspace(), WorkspaceKind::Animation);
        view.read_with(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let area = dock.area.read(cx);
            assert!(
                area.panel(PanelId::from(dock.animation_parameters.entity_id()))
                    .is_some()
            );
            assert!(
                area.panel(PanelId::from(dock.animation_expressions.entity_id()))
                    .is_none()
            );
            crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Animation).unwrap();
            assert!(
                crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Modeling).is_err()
            );
        });
        for id in [
            "workspace-modeling",
            "workspace-animation",
            "animation-play-pause",
        ] {
            let button = window.find(id);
            assert!(button.visible());
            assert!(button.bounds().right() <= px(640.));
            assert!(button.bounds().bottom() <= px(480.));
        }
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        view.read_with(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            assert!(
                dock.area
                    .read(cx)
                    .panel(PanelId::from(dock.mesh_tree.entity_id()))
                    .is_none()
            );
            assert!(
                dock.area
                    .read(cx)
                    .panel(PanelId::from(dock.parameters.entity_id()))
                    .is_some()
            );
        });
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        view.update(cx, |view, cx| {
            view.run_command(CommandId::ResetLayout, window, cx)
        });
        window.render_frame(cx);
        window.simulate_next_frame(cx);
        view.read_with(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let area = dock.area.read(cx);
            assert!(
                area.panel(PanelId::from(dock.animation_parameters.entity_id()))
                    .is_none()
            );
            assert!(
                area.panel(PanelId::from(dock.animation_expressions.entity_id()))
                    .is_some()
            );
            assert!(
                area.panel(PanelId::from(dock.animation_info.entity_id()))
                    .is_some()
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn modal_blocks_space_workspace_switch_and_clears_pan_capture(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt;
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        let from = window.find("canvas-body").bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Middle,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.open_dialog(cx, |dialog, _, _| dialog.title("Modal"));
        window.render_frame(cx);
        window.press("space", cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::ModelingWorkspace, window, cx)
        });
        assert_eq!(
            harness.view.read(cx).editor.workspace(),
            WorkspaceKind::Animation
        );
        window.close_dialog(cx);
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: from + point(px(100.), px(20.)),
                pressed_button: Some(MouseButton::Middle),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            harness.view.read(cx).editor.view().pan,
            editor_core::CanvasPoint::default()
        );
    })
    .unwrap();
}
