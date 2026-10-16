use super::setup;
use crate::actions::RunCommand;
use editor_core::{CommandId, CommandSource, ViewportInteractionKind};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, InputEvent as _, TestAppContext};

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
                view.editor
                    .begin_viewport_interaction(
                        view.editor.primary_viewport_binding().unwrap(),
                        ViewportInteractionKind::Modeling,
                    )
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
        assert!(view.editor.finish_viewport_interaction(replacement));
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
            dock.active_canvas().clone(),
            dock.properties.clone(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        area.update(cx, |area, cx| {
            area.remove_panel(properties.clone(), window, cx)
        });
        let layout = DockLayout::tabs()
            .panel_view(panel_handle(canvas.clone()), cx)
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
            area.select_panel(PanelId::from(canvas.entity_id()), window, cx)
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
