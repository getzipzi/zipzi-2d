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
