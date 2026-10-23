use super::super::{Harness, setup};
use super::common::present_canvas;
use editor_core::{ToolId, ViewportPoint};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, TestAppContext, Window, point, px,
};

fn selected(harness: &Harness, cx: &App) -> Vec<u32> {
    harness
        .view
        .read(cx)
        .editor
        .overlay()
        .vertices
        .iter()
        .filter(|v| v.selected)
        .map(|v| v.id)
        .collect()
}

fn draw(
    harness: &Harness,
    center: ViewportPoint,
    modifiers: Modifiers,
    finish: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let origin = window.find("canvas-body").bounds().origin;
    let at = |x, y| origin + point(px(center.x + x), px(center.y + y));
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position: at(-12., -12.),
            modifiers,
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
    for (x, y) in [(12., -12.), (12., 12.)] {
        window.dispatch_event(
            MouseMoveEvent {
                position: at(x, y),
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    }
    let view = harness.view.read(cx);
    assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    assert!(view.editor.gesture_active());
    assert!(!view.editor.overlay().lasso.is_empty());
    assert!(view.editor.overlay().marquee.is_none());
    assert!(!view.editor.document().unwrap().has_pending_edit());
    assert_eq!(
        crate::workspace::status_bar::resolve_hint_context(view, cx),
        &[
            editor_core::InteractionHint::Drag(editor_core::HintAction::LassoSelect),
            editor_core::InteractionHint::Shift(editor_core::HintAction::AddSelection),
            editor_core::InteractionHint::Alt(editor_core::HintAction::RemoveSelection),
            editor_core::InteractionHint::Command(editor_core::CommandId::CancelGesture),
        ][..]
    );
    if finish {
        // No final Move: native Up must supply the last polygon vertex.
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: at(-12., 12.),
                modifiers,
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    }
}

#[gpui_kit::test]
fn toolbar_lasso_native_modifiers_and_select_shortcut_preserve_components(cx: &mut TestAppContext) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        let vertices = harness.view.read(cx).editor.overlay().vertices;
        let original = harness
            .view
            .read(cx)
            .editor
            .document()
            .unwrap()
            .selected_vertex_positions();
        let evaluations = harness.view.read(cx).editor.evaluation_count();
        window.click("tool_lasso", cx);
        assert_eq!(harness.view.read(cx).editor.active_tool(), ToolId::Lasso);
        draw(
            &harness,
            vertices[0].position,
            Modifiers::default(),
            true,
            window,
            cx,
        );
        assert_eq!(selected(&harness, cx), vec![vertices[0].id]);
        draw(
            &harness,
            vertices[1].position,
            Modifiers {
                shift: true,
                ..Default::default()
            },
            true,
            window,
            cx,
        );
        assert_eq!(selected(&harness, cx).len(), 2);
        draw(
            &harness,
            vertices[0].position,
            Modifiers {
                alt: true,
                ..Default::default()
            },
            true,
            window,
            cx,
        );
        assert_eq!(selected(&harness, cx), vec![vertices[1].id]);
        window.press("v", cx);
        assert_eq!(harness.view.read(cx).editor.active_tool(), ToolId::Select);
        assert_eq!(selected(&harness, cx), vec![vertices[1].id]);
        window.press("l", cx);
        assert_eq!(harness.view.read(cx).editor.active_tool(), ToolId::Lasso);
        assert_eq!(selected(&harness, cx), vec![vertices[1].id]);
        let view = harness.view.read(cx);
        assert_eq!(
            view.editor.document().unwrap().selected_vertex_positions(),
            original
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert_eq!(view.editor.evaluation_count(), evaluations);
        assert!(!view.editor.gesture_active());
    })
    .unwrap();
}

#[gpui_kit::test]
fn lasso_hover_then_vertex_drag_uses_select_editing_and_preserves_the_tool(
    cx: &mut TestAppContext,
) {
    use crate::workspace::status_bar::resolve_hint_context;
    for cancel in [false, true] {
        let harness = setup(cx);
        present_canvas(&harness, cx);
        cx.update_window(harness.window, |_, window, cx| {
            let vertices = harness.view.read(cx).editor.overlay().vertices;
            let original = harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .selected_vertex_positions();
            window.click("tool_lasso", cx);
            draw(
                &harness,
                vertices[0].position,
                Modifiers::default(),
                true,
                window,
                cx,
            );
            draw(
                &harness,
                vertices[1].position,
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                true,
                window,
                cx,
            );
            assert_eq!(selected(&harness, cx).len(), 2);
            let origin = window.find("canvas-body").bounds().origin;
            let start = origin + point(px(vertices[0].position.x), px(vertices[0].position.y));
            let end = start + point(px(24.), px(12.));
            window.dispatch_event(
                MouseMoveEvent {
                    position: start,
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let overlay = harness.view.read(cx).editor.overlay();
            assert!(
                overlay
                    .vertices
                    .iter()
                    .any(|v| v.id == vertices[0].id && v.hovered)
            );
            assert_eq!(
                overlay.active_component.as_ref().map(|c| c.id),
                Some(vertices[0].id)
            );
            assert_eq!(harness.view.read(cx).editor.active_tool(), ToolId::Lasso);
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let view = harness.view.read(cx);
            assert!(!view.editor.document().unwrap().has_pending_edit());
            assert_eq!(
                resolve_hint_context(view, cx),
                &[
                    editor_core::InteractionHint::Drag(editor_core::HintAction::Transform),
                    editor_core::InteractionHint::Command(editor_core::CommandId::CancelGesture)
                ][..]
            );
            window.dispatch_event(
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let view = harness.view.read(cx);
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert!(view.editor.document().unwrap().has_pending_edit());
            assert!(view.editor.overlay().lasso.is_empty());
            assert!(view.editor.overlay().marquee.is_none());
            assert_eq!(
                view.editor
                    .overlay()
                    .active_component
                    .as_ref()
                    .map(|c| c.id),
                Some(vertices[0].id)
            );
            let positions = view.editor.document().unwrap().selected_vertex_positions();
            assert_eq!(
                positions
                    .iter()
                    .zip(&original)
                    .filter(|(a, b)| a != b)
                    .count(),
                2
            );
            if cancel {
                window.press("escape", cx);
            } else {
                window.dispatch_event(
                    MouseUpEvent {
                        button: MouseButton::Left,
                        position: end,
                        modifiers: Default::default(),
                        click_count: 1,
                    }
                    .to_platform_input(),
                    cx,
                );
            }
            window.render_frame(cx);
            let view = harness.view.read(cx);
            assert_eq!(view.editor.active_tool(), ToolId::Lasso);
            assert!(!view.editor.gesture_active());
            assert!(!view.editor.document().unwrap().has_pending_edit());
            assert_eq!(selected(&harness, cx).len(), 2);
            assert_eq!(
                view.editor.document().unwrap().selected_vertex_positions(),
                if cancel { original } else { positions }
            );
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (usize::from(!cancel), 0)
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn lasso_blank_click_clears_native_object_and_component_selection(cx: &mut TestAppContext) {
    for (shift, alt) in [(false, false), (true, false), (false, true), (true, true)] {
        let harness = setup(cx);
        present_canvas(&harness, cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.click("tool_lasso", cx);
            let v = harness.view.read(cx).editor.overlay().vertices[0].position;
            draw(&harness, v, Modifiers::default(), true, window, cx);
            assert_eq!(selected(&harness, cx).len(), 1);
            let bounds = window.find("canvas-body").bounds();
            let blank = bounds.bottom_right() - point(px(8.), px(8.));
            let modifiers = Modifiers {
                shift,
                alt,
                ..Default::default()
            };
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: blank,
                    modifiers,
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            assert_eq!(selected(&harness, cx).len(), 1);
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: blank,
                    modifiers,
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let view = harness.view.read(cx);
            let doc = view.editor.document().unwrap();
            assert!(doc.context().selections.is_empty());
            assert!(doc.context().selection.is_none());
            assert!(doc.context().inspected.is_none());
            assert!(selected(&harness, cx).is_empty());
            assert!(!view.editor.gesture_active());
            assert_eq!(doc.history_lengths(), (0, 0));
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn lasso_escape_switch_and_focus_loss_discard_native_capture(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_lasso", cx);
        let v = harness.view.read(cx).editor.overlay().vertices[0].position;
        draw(&harness, v, Modifiers::default(), true, window, cx);
    })
    .unwrap();
    for cancel in 0..3 {
        cx.update_window(harness.window, |_, window, cx| {
            window.click("tool_lasso", cx);
            let vertices = harness.view.read(cx).editor.overlay().vertices;
            let before = selected(&harness, cx);
            draw(
                &harness,
                vertices[1].position,
                Modifiers::default(),
                false,
                window,
                cx,
            );
            assert_eq!(selected(&harness, cx), before);
            match cancel {
                0 => window.press("escape", cx),
                1 => window.press("v", cx),
                _ => {
                    let focus = harness.view.read(cx).focus.clone();
                    window.focus(&focus, cx);
                }
            }
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let view = harness.view.read(cx);
            assert!(!view.editor.gesture_active(), "cancel {cancel}");
            assert!(view.editor.overlay().lasso.is_empty());
            assert_eq!(selected(&harness, cx).len(), 1);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        })
        .unwrap();
    }
}
