use super::super::{Harness, setup};
use super::common::present_canvas;
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};
use zipzi_editor_core::{EditorCommand, ModelPoint, ObjectKind, ObjectRef};

fn positions(h: &Harness, object: &ObjectRef, cx: &mut TestAppContext) -> Vec<ModelPoint> {
    present_canvas(h, cx);
    h.view.read_with(cx, |view, _| {
        view.modeling_frame_for(view.viewport_slots[0])
            .unwrap()
            .canvas
            .geometry
            .mesh(&object.id)
            .unwrap()
            .vertices
            .iter()
            .map(|(_, p)| *p)
            .collect()
    })
}
#[gpui_kit::test]
fn arrows_nudge_canvas_objects_and_points_but_leave_text_carets_alone(cx: &mut TestAppContext) {
    let h = setup(cx);
    let object = h.view.update(cx, |view, cx| {
        let object = ObjectRef::new(ObjectKind::Mesh, &view.editor.layers()[0].id);
        view.editor
            .execute(EditorCommand::SelectObject(object.clone()))
            .unwrap();
        view.redraw(cx);
        object
    });
    let before = positions(&h, &object, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        let focus = h
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .active_viewport()
            .read(cx)
            .focus
            .clone();
        window.focus(&focus, cx);
        for key in ["right", "shift-up", "alt-left"] {
            window.press(key, cx);
            window.render_frame(cx);
        }
    })
    .unwrap();
    let after = positions(&h, &object, cx);
    for (a, b) in before.iter().zip(&after) {
        assert!((b.x - a.x - 0.9).abs() < 0.002, "{a:?} {b:?}");
        assert!((b.y - a.y - 10.).abs() < 0.002);
    }
    cx.update_window(h.window, |_, window, cx| {
        let p = h.view.read(cx).editor.overlay().vertices[0].position;
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
        let overlay = h.view.read(cx).editor.overlay();
        assert!(overlay.warp_frame_lines.is_empty());
        assert!(overlay.handles.is_empty());
        window.press("shift-right", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let local = positions(&h, &object, cx);
    assert!((local[0].x - after[0].x - 10.).abs() < 0.002);
    assert_eq!(&local[1..], &after[1..]);
    super::super::properties::fill(&h, "property-input-name", "Editing", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("left", cx);
        window.press("shift-up", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(positions(&h, &object, cx), local);
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error)
    });
}

#[gpui_kit::test]
fn frame_fine_scaling_freezes_mouse_down_modifiers(cx: &mut TestAppContext) {
    use gpui_kit::{
        InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    };
    let h = setup(cx);
    let object = h.view.update(cx, |view, cx| {
        let object = ObjectRef::new(ObjectKind::Mesh, &view.editor.layers()[0].id);
        view.editor
            .execute(EditorCommand::SelectObject(object.clone()))
            .unwrap();
        view.redraw(cx);
        object
    });
    let before = positions(&h, &object, cx);
    let center = ModelPoint {
        x: (before[0].x + before[2].x) / 2.,
        y: (before[0].y + before[2].y) / 2.,
    };
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        let (start, scale) = h.view.read_with(cx, |view, _| {
            (
                view.editor.overlay().handles[4].position,
                view.modeling_frame_for(view.viewport_slots[0])
                    .unwrap()
                    .canvas
                    .transform
                    .scale,
            )
        });
        let origin = window.find("canvas-body").bounds().origin;
        let from = origin + point(px(start.x), px(start.y));
        let end = from
            + point(
                px((before[2].x - center.x) * 0.237 * scale),
                px((center.y - before[0].y) * 0.237 * scale),
            );
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Modifiers {
                    shift: true,
                    alt: true,
                    ..Default::default()
                },
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers: Modifiers::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    for (a, b) in before.iter().zip(positions(&h, &object, cx)) {
        assert!(
            (b.x - center.x - (a.x - center.x) * 1.24).abs() < 0.002,
            "{a:?} {b:?}"
        );
        assert!((b.y - center.y - (a.y - center.y) * 1.24).abs() < 0.002);
    }
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}
