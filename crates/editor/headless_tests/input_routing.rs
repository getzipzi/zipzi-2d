use super::{document_fixture, setup, setup_with_document};
use editor_core::{CommandId, CommandSource, ViewportKind};
use gpui_kit::component::WindowExt;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Focusable, InputEvent as _, KeyDownEvent, Keystroke, MouseExitEvent,
    MouseMoveEvent, TestAppContext,
};

#[gpui_kit::test]
fn captured_viewport_respects_the_configured_cancel_binding(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent};
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        for binding in &mut cx.global_mut::<crate::settings::Settings>().bindings {
            if binding.command == CommandId::CancelGesture {
                binding.key = "alt-x".into();
            }
        }
        window.dispatch_event(
            MouseDownEvent {
                position: window.find("canvas-body").bounds().center(),
                button: MouseButton::Middle,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert!(h.view.read(cx).editor.viewport_interaction_active());
        window.press("escape", cx);
        assert!(h.view.read(cx).editor.viewport_interaction_active());
        window.press("alt-x", cx);
        assert!(!h.view.read(cx).editor.viewport_interaction_active());
    })
    .unwrap();
}

#[gpui_kit::test]
fn popover_keeps_keyboard_priority_when_pointer_returns_to_viewport(cx: &mut TestAppContext) {
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("mesh-tree-filter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.context_stack().iter().any(|c| c.contains("Popover")));
        let before = h.view.read(cx).editor.view();
        window.hover("canvas-body", cx);
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view(), before);
        window.press("escape", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn viewport_toolbar_retains_local_shortcut_and_menu_target(cx: &mut TestAppContext) {
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.hover("tool_hand", cx);
        let before = h.view.read(cx).editor.view().zoom;
        assert!(matches!(
            h.view.read(cx).hovered_target(window, cx),
            Some(crate::input_routing::LocalTarget::Viewport(_))
        ));
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view().zoom, before + 0.25);
        assert!(
            h.view
                .read(cx)
                .viewport_command(CommandId::ZoomIn, CommandSource::NativeMenu)
                .document()
                .is_some()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn hover_switches_instances_without_focus_and_exit_has_no_fallback(cx: &mut TestAppContext) {
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        let ids = h.view.update(cx, |view, cx| {
            view.install_viewport_scene(&[ViewportKind::Modeling; 2], window, cx)
        });
        window.render_frame(cx);
        let first = h.view.read(cx).viewport_surface(ids[0], cx).unwrap();
        let second = h.view.read(cx).viewport_surface(ids[1], cx).unwrap();
        window.focus(&second.read(cx).focus_handle(cx), cx);
        let position = first.read(cx).benchmark_center().unwrap();
        let before = h
            .view
            .read(cx)
            .editor
            .viewport_view(h.view.read(cx).editor.viewport_binding(ids[0]).unwrap())
            .unwrap()
            .zoom;
        let peer = h
            .view
            .read(cx)
            .editor
            .viewport_view(h.view.read(cx).editor.viewport_binding(ids[1]).unwrap())
            .unwrap();
        window.dispatch_event(
            MouseMoveEvent {
                position,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.press("cmd-=", cx);
        assert!(second.read(cx).focus_handle(cx).is_focused(window));
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .viewport_view(h.view.read(cx).editor.viewport_binding(ids[0]).unwrap())
                .unwrap()
                .zoom,
            before + 0.25
        );
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .viewport_view(h.view.read(cx).editor.viewport_binding(ids[1]).unwrap())
                .unwrap(),
            peer
        );
        window.dispatch_event(
            MouseExitEvent {
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.press("cmd-=", cx);
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .viewport_view(h.view.read(cx).editor.viewport_binding(ids[0]).unwrap())
                .unwrap()
                .zoom,
            before + 0.25
        );
        window.dispatch_event(
            MouseMoveEvent {
                position,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            KeyDownEvent {
                keystroke: Keystroke::parse("cmd-=").unwrap(),
                is_held: true,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .viewport_view(h.view.read(cx).editor.viewport_binding(ids[0]).unwrap())
                .unwrap()
                .zoom,
            before + 0.5
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn timeline_hover_uses_its_binding_and_text_focus_keeps_its_keys(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let surface = h
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .active_viewport()
            .clone();
        window.focus(&surface.read(cx).focus_handle(cx), cx);
        window.hover("timeline-graph", cx);
        window.press("right", cx);
        assert!((h.view.read(cx).editor.animation().unwrap().edit_time - 1. / 60.).abs() < 1e-6);
        assert!(surface.read(cx).focus_handle(cx).is_focused(window));
        window.hover("canvas-body", cx);
        window.press("right", cx);
        assert!((h.view.read(cx).editor.animation().unwrap().edit_time - 1. / 60.).abs() < 1e-6);
        window.click("timeline-search", cx);
        window.hover("canvas-body", cx);
        let tool = h.view.read(cx).editor.active_tool();
        window.press("g", cx);
        assert_eq!(h.view.read(cx).editor.active_tool(), tool);
        assert!(window.has_focused_input(cx));
        let saved = h
            .view
            .read(cx)
            .viewport_command(CommandId::ZoomIn, CommandSource::NativeMenu);
        assert!(saved.document().is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn chrome_and_popups_do_not_reuse_the_previous_viewport(cx: &mut TestAppContext) {
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.hover("canvas-body", cx);
        let before = h.view.read(cx).editor.view();
        let tool = h.view.read(cx).editor.active_tool();
        window.hover("workspace-animation", cx);
        window.press("cmd-=", cx);
        window.press("g", cx);
        assert_eq!(h.view.read(cx).editor.view(), before);
        assert_eq!(h.view.read(cx).editor.active_tool(), tool);
        window.hover("object-property-name", cx);
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view(), before);
        assert_eq!(
            h.view
                .read(cx)
                .viewport_command(CommandId::ZoomIn, CommandSource::NativeMenu)
                .target,
            editor_core::CommandTarget::Unavailable
        );
        window.right_click("canvas-body", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_some());
        let before = h.view.read(cx).editor.view();
        let tool = h.view.read(cx).editor.active_tool();
        window.press("g", cx);
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view(), before);
        assert_eq!(h.view.read(cx).editor.active_tool(), tool);
        window.press("escape", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn captured_navigation_keeps_its_owner_across_another_viewport(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, point, px};
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        let ids = h.view.update(cx, |view, cx| {
            view.install_viewport_scene(&[ViewportKind::Modeling; 2], window, cx)
        });
        window.render_frame(cx);
        let first = h.view.read(cx).viewport_surface(ids[0], cx).unwrap();
        let second = h.view.read(cx).viewport_surface(ids[1], cx).unwrap();
        let origin = second.read(cx).benchmark_center().unwrap();
        let destination = first.read(cx).benchmark_center().unwrap();
        let peer = h
            .view
            .read(cx)
            .editor
            .viewport_view(h.view.read(cx).editor.viewport_binding(ids[0]).unwrap())
            .unwrap();
        window.dispatch_event(
            MouseDownEvent {
                position: origin,
                button: MouseButton::Middle,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: destination,
                pressed_button: Some(MouseButton::Middle),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .viewport_interaction_binding()
                .unwrap()
                .viewport,
            ids[1]
        );
        window.press("escape", cx);
        assert!(!h.view.read(cx).editor.viewport_interaction_active());
        let after = h
            .view
            .read(cx)
            .editor
            .viewport_view(h.view.read(cx).editor.viewport_binding(ids[1]).unwrap())
            .unwrap();
        window.dispatch_event(
            MouseMoveEvent {
                position: destination + point(px(40.), px(0.)),
                pressed_button: Some(MouseButton::Middle),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                position: destination,
                button: MouseButton::Middle,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .viewport_view(h.view.read(cx).editor.viewport_binding(ids[1]).unwrap())
                .unwrap(),
            after
        );
        assert_eq!(
            h.view
                .read(cx)
                .editor
                .viewport_view(h.view.read(cx).editor.viewport_binding(ids[0]).unwrap())
                .unwrap(),
            peer
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn destroying_a_timeline_unregisters_its_binding(cx: &mut TestAppContext) {
    let h = setup(cx);
    let id = cx
        .update_window(h.window, |_, window, cx| {
            let timeline = crate::panels::timeline::Timeline::new(h.view.downgrade(), window, cx);
            let id = timeline.read(cx).id;
            h.view
                .update(cx, |view, _| view.editor.set_timeline_visible(id, true));
            assert!(h.view.read(cx).editor.timeline_binding(id).is_some());
            drop(timeline);
            id
        })
        .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_binding(id).is_none())
    });
}

#[gpui_kit::test]
fn undeclared_focus_keeps_keys_without_a_named_control_context(cx: &mut TestAppContext) {
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        let control = cx.focus_handle();
        window.focus(&control, cx);
        window.hover("canvas-body", cx);
        let before = h.view.read(cx).editor.view();
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view(), before);
        let focus = h.view.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view().zoom, before.zoom + 0.25);
    })
    .unwrap();
}

#[gpui_kit::test]
fn popover_in_another_window_does_not_capture_this_windows_keyboard(cx: &mut TestAppContext) {
    let first = setup(cx);
    let second = setup(cx);
    cx.update_window(first.window, |_, window, cx| {
        window.click("mesh-tree-filter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(second.window, |_, window, cx| {
        let focus = second.view.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.hover("canvas-body", cx);
        let before = second.view.read(cx).editor.view().zoom;
        window.press("cmd-=", cx);
        assert_eq!(second.view.read(cx).editor.view().zoom, before + 0.25);
    })
    .unwrap();
}

#[gpui_kit::test]
fn toolkit_drag_cancellation_only_releases_the_captured_operation(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::*;
    struct DragPreview;
    impl Render for DragPreview {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            Empty
        }
    }
    struct DragProbe;
    impl Render for DragProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .id("drag-source")
                    .test_support()
                    .size(px(100.))
                    .on_drag((), |_, _, _, cx| cx.new(|_| DragPreview)),
            )
        }
    }
    let window = cx.add_window(|_, _| DragProbe);
    cx.update_window(*window, |_, window, cx| {
        window.render_frame(cx);
        let position = window.find("drag-source").bounds().center();
        press(window, position, cx);
        slide(window, position + point(px(20.), px(0.)), false, cx);
        let old = cx.active_drag_token().expect("first drag");
        release(window, position, cx);
        window.render_frame(cx);
        press(window, position, cx);
        slide(window, position + point(px(20.), px(0.)), false, cx);
        let current = cx.active_drag_token().expect("replacement drag");
        assert!(!cx.stop_drag(&old, window));
        assert!(cx.has_active_drag());
        assert!(cx.stop_drag(&current, window));
        assert!(!cx.has_active_drag());
        assert!(!cx.stop_drag(&current, window));
    })
    .unwrap();
}

#[gpui_kit::test]
fn closing_blocking_ui_keeps_the_current_pointer_location(cx: &mut TestAppContext) {
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.hover("canvas-body", cx);
        let before = h.view.read(cx).editor.view().zoom;
        h.view.update(cx, |view, cx| {
            view.modal = true;
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view().zoom, before);
        h.view.update(cx, |view, cx| {
            view.modal = false;
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.press("cmd-=", cx);
        assert_eq!(h.view.read(cx).editor.view().zoom, before + 0.25);
    })
    .unwrap();
}

#[gpui_kit::test]
fn status_bar_hints_stay_quiet_until_modal_tools_or_active_captures(cx: &mut TestAppContext) {
    use crate::workspace::status_bar::{HintContext, TimelineCaptureKind};
    use editor_core::ToolId;
    use gpui_kit::{MouseButton, MouseDownEvent, component::kbd::Kbd};

    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);

    cx.update_window(h.window, |_, window, cx| {
        let status_bar = h.view.read(cx).status_bar.clone();

        // 1. Quiet by default in Select tool, even when hovering across panels and viewport.
        assert_eq!(status_bar.read(cx).hint_context(), Some(HintContext::Quiet));
        assert!(status_bar.read(cx).hints().is_empty());
        window.hover("canvas-body", cx);
        window.render_frame(cx);
        assert_eq!(status_bar.read(cx).hint_context(), Some(HintContext::Quiet));
        assert!(status_bar.read(cx).hints().is_empty());

        // 2. Non-default Modeling tools (Glue / Hand) display concise tool hints.
        window.click("tool_glue", cx);
        window.render_frame(cx);
        assert_eq!(
            status_bar.read(cx).hint_context(),
            Some(HintContext::ModelingTool(ToolId::Glue))
        );
        assert_eq!(status_bar.read(cx).hints().len(), 2);

        // 3. Active viewport capture overrides tool hints and reflects custom keybindings.
        for binding in &mut cx.global_mut::<crate::settings::Settings>().bindings {
            if binding.command == CommandId::CancelGesture {
                binding.key = "alt-x".into();
            }
        }
        window.dispatch_event(
            MouseDownEvent {
                position: window.find("canvas-body").bounds().center(),
                button: MouseButton::Middle,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            status_bar.read(cx).hint_context(),
            Some(HintContext::ViewportCapture { panning: true })
        );
        let expected_cancel_key = Kbd::format(&Keystroke::parse("alt-x").unwrap());
        assert!(
            status_bar
                .read(cx)
                .hints()
                .iter()
                .any(|(key, _)| key == &expected_cancel_key),
            "expected custom CancelGesture key {expected_cancel_key} in {:?}",
            status_bar.read(cx).hints()
        );

        window.press("alt-x", cx);
        window.render_frame(cx);
        assert_eq!(
            status_bar.read(cx).hint_context(),
            Some(HintContext::ModelingTool(ToolId::Glue))
        );

        // 4. Animation workspace stays quiet until Timeline pan mode or an active capture starts.
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        assert_eq!(status_bar.read(cx).hint_context(), Some(HintContext::Quiet));
        assert!(status_bar.read(cx).hints().is_empty());

        window.click("timeline-pan", cx);
        window.render_frame(cx);
        assert_eq!(
            status_bar.read(cx).hint_context(),
            Some(HintContext::TimelinePanTool)
        );
        window.click("timeline-pan", cx);
        window.render_frame(cx);
        assert_eq!(status_bar.read(cx).hint_context(), Some(HintContext::Quiet));

        let ruler_center = window.find("timeline-ruler").bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                position: ruler_center,
                button: MouseButton::Left,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            status_bar.read(cx).hint_context(),
            Some(HintContext::TimelineCapture(TimelineCaptureKind::Seek))
        );

        window.press("escape", cx);
        window.render_frame(cx);
        assert_eq!(status_bar.read(cx).hint_context(), Some(HintContext::Quiet));

        // 5. Status messages and edit errors route through StatusBar with semantic error flag.
        h.view.update(cx, |view, cx| {
            view.set_status_message("Saved snapshot", cx);
        });
        assert_eq!(
            status_bar.read(cx).displayed_message(),
            Some(("Saved snapshot", false))
        );
        h.view.update(cx, |view, cx| {
            view.set_edit_error("Invalid operation", cx);
        });
        assert_eq!(
            status_bar.read(cx).displayed_message(),
            Some(("Invalid operation", true))
        );
        h.view.update(cx, |view, cx| {
            view.clear_edit_error(cx);
        });
        assert_eq!(
            status_bar.read(cx).displayed_message(),
            Some(("Saved snapshot", false))
        );
    })
    .unwrap();
}

