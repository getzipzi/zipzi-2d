use super::super::{Harness, setup};
use super::common::present_canvas;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, TestAppContext, Window, point, px,
};
use zipzi_editor_core::{ToolId, ViewportPoint};

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

#[gpui_kit::test]
fn select_all_shortcut_selects_canvas_vertices_and_leaves_text_selection_to_inputs(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::{WindowExt, input::AnyInputState};
    for tool in [ToolId::Select, ToolId::Lasso] {
        let (document, assets) =
            super::super::document_fixture::sample_document_with_binding(true).unwrap();
        let parameter = document.parameters()[0].id.clone();
        let harness = super::super::setup_with_document(cx, document, assets);
        present_canvas(&harness, cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.click(
                if tool == ToolId::Lasso {
                    "tool_lasso"
                } else {
                    "tool_select"
                },
                cx,
            );
            let vertex = harness.view.read(cx).editor.overlay().vertices[0].position;
            window.click_at("canvas-body", point(px(vertex.x), px(vertex.y)), cx);
            window.press("cmd-a", cx);
            window.render_frame(cx);
            let vertices = harness.view.read(cx).editor.overlay().vertices;
            assert_eq!(vertices.len(), 4);
            assert!(vertices.iter().all(|v| v.selected && v.editable));
            // Leave a partial canvas selection so text Select All cannot silently change it.
            let other = vertices[1].position;
            window.click_with_options(
                "canvas-body",
                gpui_kit::test::ClickOptions::new()
                    .with_offset(point(px(other.x), px(other.y)))
                    .with_modifiers(Modifiers {
                        shift: true,
                        ..Default::default()
                    }),
                cx,
            );
            let before = selected(&harness, cx);
            assert_eq!(before.len(), 3);
            window.click(format!("parameter-value-{parameter}"), cx);
            window.press("cmd-a", cx);
            let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
                panic!("input not focused")
            };
            assert_eq!(
                input.read(cx).selected_range(),
                0..input.read(cx).value().len()
            );
            assert_eq!(selected(&harness, cx), before);
            let view = harness.view.read(cx);
            assert_eq!(view.editor.active_tool(), tool);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        })
        .unwrap();
    }
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
    let is_lasso = view.editor.active_tool() == ToolId::Lasso;
    assert_eq!(!view.editor.overlay().lasso.is_empty(), is_lasso);
    assert_eq!(view.editor.overlay().marquee.is_some(), !is_lasso);
    assert!(!view.editor.document().unwrap().has_pending_edit());
    assert_eq!(
        crate::workspace::status_bar::resolve_hint_context(view, cx),
        &[
            zipzi_editor_core::InteractionHint::Drag(if is_lasso {
                zipzi_editor_core::HintAction::LassoSelect
            } else {
                zipzi_editor_core::HintAction::Transform
            }),
            zipzi_editor_core::InteractionHint::Shift(zipzi_editor_core::HintAction::AddSelection),
            zipzi_editor_core::InteractionHint::Alt(zipzi_editor_core::HintAction::RemoveSelection),
            zipzi_editor_core::InteractionHint::Command(
                zipzi_editor_core::CommandId::CancelGesture
            ),
        ][..]
    );
    if finish {
        // No final Move: native Up must supply the last polygon vertex.
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: at(if is_lasso { -12. } else { 12. }, 12.),
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
fn selection_tools_native_modifiers_and_shortcuts_preserve_components(cx: &mut TestAppContext) {
    for tool in [ToolId::Select, ToolId::Lasso] {
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
            window.click(
                if tool == ToolId::Lasso {
                    "tool_lasso"
                } else {
                    "tool_select"
                },
                cx,
            );
            assert_eq!(harness.view.read(cx).editor.active_tool(), tool);
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
                    shift: true,
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
                    zipzi_editor_core::InteractionHint::Drag(
                        zipzi_editor_core::HintAction::Transform
                    ),
                    zipzi_editor_core::InteractionHint::Command(
                        zipzi_editor_core::CommandId::CancelGesture
                    )
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
