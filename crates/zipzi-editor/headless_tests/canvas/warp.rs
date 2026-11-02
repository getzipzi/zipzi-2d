use super::super::{Harness, document_fixture, setup_with_document};
use super::common::present_canvas;
use gpui_kit::{
    AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, TestAppContext, point, px, test::TestWindowExt,
};
use zipzi_editor_core::{EditorCommand, ObjectKind, ObjectRef, ViewportPoint};

fn setup_warp(cx: &mut TestAppContext) -> (Harness, ObjectRef) {
    let (document, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let object = document_fixture::object(ObjectKind::Deformer, 502);
    let h = setup_with_document(cx, document, assets);
    h.view.update(cx, |view, cx| {
        view.editor
            .execute(EditorCommand::SelectObject(object.clone()))
            .unwrap();
        view.editor.reset_view();
        view.editor.pan_by(ViewportPoint { x: -128., y: 128. });
        view.redraw(cx);
    });
    present_canvas(&h, cx);
    (h, object)
}

fn points(h: &Harness, object: &ObjectRef, cx: &TestAppContext) -> serde_json::Value {
    h.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .object_index()
            .get(object)
            .unwrap()
            .properties["warp"]["points"]
            .clone()
    })
}

#[gpui_kit::test]
fn warp_grid_and_bezier_controls_share_one_surface_without_mode_switches(cx: &mut TestAppContext) {
    use zipzi_editor_core::OverlayVertexKind as K;
    let (h, object) = setup_warp(cx);
    let before = points(&h, &object, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        for id in ["deformer-warp-bezier", "deformer-warp-fine"] {
            assert!(window.try_find(id).is_none());
        }
        let overlay = h.view.read(cx).editor.overlay();
        assert_eq!(
            overlay
                .vertices
                .iter()
                .filter(|v| v.kind == K::Warp)
                .count(),
            4
        );
        assert_eq!(
            overlay
                .vertices
                .iter()
                .filter(|v| v.kind == K::WarpGrid)
                .count(),
            4
        );
        assert!(!overlay.deformers[0].lines.is_empty());
        assert!(!overlay.bezier_lines.is_empty());
        assert_eq!(overlay.bezier_tangents.len(), 8);
        assert_eq!(overlay.handles.iter().filter(|h| h.tangent).count(), 8);
        let anchor = overlay
            .vertices
            .iter()
            .find(|v| v.kind == K::Warp)
            .unwrap()
            .position;
        window.click_at("canvas-body", point(px(anchor.x), px(anchor.y)), cx);
        window.render_frame(cx);
        let overlay = h.view.read(cx).editor.overlay();
        assert!(overlay.warp_frame_lines.is_empty());
        assert!(overlay.handles.iter().all(|h| h.tangent));
        assert!(
            overlay
                .vertices
                .iter()
                .any(|v| v.kind == K::Warp && v.selected)
        );
        assert!(
            overlay
                .vertices
                .iter()
                .filter(|v| v.kind == K::WarpGrid)
                .all(|v| !v.selected)
        );
    })
    .unwrap();
    assert_eq!(points(&h, &object, cx), before);
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn warp_native_rotation_handle_cancel_commit_and_history(cx: &mut TestAppContext) {
    let (h, object) = setup_warp(cx);
    let before = points(&h, &object, cx);
    for cancel in [true, false] {
        present_canvas(&h, cx);
        cx.update_window(h.window, |_, window, cx| {
            window.activate_window();
            let (start, center) = h.view.read_with(cx, |view, _| {
                let frame = view.modeling_frame_for(view.viewport_slots[0]).unwrap();
                let control = frame
                    .canvas
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == object)
                    .unwrap();
                let gizmo = frame
                    .canvas
                    .geometry
                    .deformer_gizmo(control, frame.canvas.transform)
                    .unwrap();
                (gizmo.handles.last().unwrap().1, gizmo.center)
            });
            let origin = window.find("canvas-body").bounds().origin;
            let from = origin + point(px(start.x), px(start.y));
            let end = origin
                + point(
                    px(center.x + start.y - center.y),
                    px(center.y - start.x + center.x),
                );
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers: Modifiers::default(),
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
            h.view.read_with(cx, |view, _| {
                assert_ne!(
                    view.editor
                        .document()
                        .unwrap()
                        .object_index()
                        .get(&object)
                        .unwrap()
                        .properties["warp"]["points"],
                    before
                );
            });
            if cancel {
                window.press("escape", cx);
            }
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
        h.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert!(!view.editor.gesture_active());
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (usize::from(!cancel), 0)
            );
        });
        if cancel {
            assert_eq!(points(&h, &object, cx), before);
        } else {
            assert_ne!(points(&h, &object, cx), before);
            h.view.update(cx, |view, cx| {
                view.editor.execute(EditorCommand::Undo).unwrap();
                view.redraw(cx);
            });
            assert_eq!(points(&h, &object, cx), before);
        }
    }
}

#[gpui_kit::test]
fn warp_native_shift_drag_keeps_selection_and_holds_axis_while_shift_is_held(
    cx: &mut TestAppContext,
) {
    let (h, object) = setup_warp(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        let start = h
            .view
            .read_with(cx, |view, _| view.editor.overlay().vertices[0].position);
        window.click_at("canvas-body", point(px(start.x), px(start.y)), cx);
        let origin = window.find("canvas-body").bounds().origin;
        let from = origin + point(px(start.x), px(start.y));
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        for offset in [(12., 4.), (20., 45.)] {
            window.dispatch_event(
                MouseMoveEvent {
                    position: from + point(px(offset.0), px(offset.1)),
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                }
                .to_platform_input(),
                cx,
            );
        }
        window.render_frame(cx);
        h.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.overlay().warp_transform_points, Some(1))
        });
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: from + point(px(20.), px(45.)),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    let after = points(&h, &object, cx);
    assert!((after[0]["x"].as_f64().unwrap() - 20.).abs() < 0.02);
    assert_eq!(after[0]["y"], 0.);
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(
            view.editor
                .warp_transform_target(&object)
                .unwrap()
                .components
                .len(),
            1
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}
