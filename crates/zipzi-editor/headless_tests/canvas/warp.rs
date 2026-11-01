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
        view.editor.execute(EditorCommand::ToggleWarpFine).unwrap();
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
fn warp_modes_show_the_active_choice_and_restore_tangent_handles_without_editing(
    cx: &mut TestAppContext,
) {
    let (h, object) = setup_warp(cx);
    let before = points(&h, &object, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        assert_eq!(window.find("deformer-warp-bezier").label(), Some("Bezier"));
        assert_eq!(
            window.find("deformer-warp-fine").label(),
            Some("Division points")
        );
        assert_eq!(window.find("deformer-warp-fine").checked(), Some(true));
        assert_eq!(window.find("deformer-warp-bezier").checked(), Some(false));
        assert!(h.view.read(cx).editor.overlay().gizmo_lines.is_empty());

        window.click("deformer-warp-bezier", cx);
        window.render_frame(cx);
        assert_eq!(window.find("deformer-warp-bezier").checked(), Some(true));
        assert_eq!(window.find("deformer-warp-fine").checked(), Some(false));
        let overlay = h.view.read(cx).editor.overlay();
        assert_eq!(overlay.vertices.len(), 4);
        assert_eq!(overlay.gizmo_lines.len(), 8);
        assert_eq!(overlay.handles.iter().filter(|h| h.round).count(), 9);

        let anchor = overlay.vertices[0].position;
        window.click_at("canvas-body", point(px(anchor.x), px(anchor.y)), cx);
        window.render_frame(cx);
        assert!(h.view.read(cx).editor.overlay().vertices[0].selected);
        window.click("deformer-warp-bezier", cx);
        window.render_frame(cx);
        assert!(h.view.read(cx).editor.overlay().vertices[0].selected);

        for id in ["deformer-warp-fine", "deformer-warp-fine"] {
            window.click(id, cx);
            window.render_frame(cx);
            assert_eq!(window.find("deformer-warp-fine").checked(), Some(true));
            assert!(h.view.read(cx).editor.overlay().gizmo_lines.is_empty());
        }
        window.click("deformer-warp-bezier", cx);
        window.render_frame(cx);
        assert_eq!(h.view.read(cx).editor.overlay().gizmo_lines.len(), 8);
    })
    .unwrap();
    assert_eq!(points(&h, &object, cx), before);
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert!(!view.editor.document().unwrap().has_pending_edit());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn warp_native_frame_menu_and_rotation_adjust_only_the_viewport(cx: &mut TestAppContext) {
    let (h, object) = setup_warp(cx);
    let before = points(&h, &object, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.click("deformer-frame", cx);
        window.render_frame(cx);
        window.click("warp-frame-adjust", cx);
        window.render_frame(cx);
    })
    .unwrap();
    for cancel in [true, false] {
        present_canvas(&h, cx);
        cx.update_window(h.window, |_, window, cx| {
            let (start, center) = h.view.read_with(cx, |view, _| {
                let binding = view.editor.primary_viewport_binding().unwrap();
                let center = view
                    .modeling_frame_for(binding.viewport)
                    .unwrap()
                    .canvas
                    .transform
                    .to_viewport(
                        view.editor
                            .warp_frame_info(binding, &object)
                            .unwrap()
                            .frame
                            .center,
                    );
                (
                    view.editor.overlay().handles.last().unwrap().position,
                    center,
                )
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
                assert!(!view.editor.document().unwrap().has_pending_edit());
                assert!(view.editor.overlay().warp_frame_adjusting);
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
            let info = view
                .editor
                .warp_frame_info(view.editor.primary_viewport_binding().unwrap(), &object)
                .unwrap();
            let expected = if cancel {
                0.
            } else {
                std::f32::consts::FRAC_PI_2
            };
            assert!((info.frame.radians - expected).abs() < 0.001);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        assert_eq!(points(&h, &object, cx), before);
    }
    cx.update_window(h.window, |_, window, cx| {
        window.click("deformer-frame", cx);
        window.render_frame(cx);
        window.click("warp-frame-visible", cx);
        window.render_frame(cx);
        h.view.read_with(cx, |view, _| {
            assert!(view.editor.overlay().handles.is_empty())
        });
    })
    .unwrap();
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
fn warp_native_option_scaling_and_whole_toggle_preserve_the_point_selection(
    cx: &mut TestAppContext,
) {
    let (h, object) = setup_warp(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        let first = h
            .view
            .read_with(cx, |view, _| view.editor.overlay().vertices[0].position);
        window.click_at("canvas-body", point(px(first.x), px(first.y)), cx);
        window.render_frame(cx);
        assert_eq!(
            h.view.read_with(cx, |view, _| view
                .editor
                .warp_transform_target(&object)
                .unwrap()
                .components
                .len()),
            1
        );
        window.click("deformer-whole", cx);
        window.render_frame(cx);
    })
    .unwrap();
    present_canvas(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        let (start, delta) = h.view.read_with(cx, |view, _| {
            let frame = view.modeling_frame_for(view.viewport_slots[0]).unwrap();
            let control = frame
                .canvas
                .geometry
                .controls
                .iter()
                .find(|c| c.object == object)
                .unwrap();
            let g = frame
                .canvas
                .geometry
                .deformer_gizmo(control, frame.canvas.transform)
                .unwrap();
            (g.handles[3].1, (g.scale_anchor(3).0.x - g.center.x) * 0.5)
        });
        let from = window.find("canvas-body").bounds().origin + point(px(start.x), px(start.y));
        let end = from + point(px(delta), px(30.));
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Modifiers {
                    alt: true,
                    ..Default::default()
                },
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        // Later modifier changes cannot turn this into an opposite-corner scale.
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
        window.click("deformer-whole", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let after = points(&h, &object, cx);
    assert!((after[0]["x"].as_f64().unwrap() + 64.).abs() < 0.02);
    assert!((after[1]["x"].as_f64().unwrap() - 320.).abs() < 0.02);
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

#[gpui_kit::test]
fn warp_native_shift_drag_keeps_selection_and_freezes_the_canvas_axis(cx: &mut TestAppContext) {
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
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
        }
        window.render_frame(cx);
        assert_eq!(
            window.find("deformer-whole").label(),
            Some("Selected points (1)")
        );
        h.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.overlay().warp_transform_points, Some(1))
        });
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: from + point(px(20.), px(45.)),
                modifiers: Modifiers::default(),
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
