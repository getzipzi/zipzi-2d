use super::super::setup;
use super::common::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, PinchEvent, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase, point, px,
};

#[gpui_kit::test]
fn wheel_zoom_interpolates_proportionally_and_keeps_anchor_on_every_frame(cx: &mut TestAppContext) {
    use std::{cell::Cell, rc::Rc};

    let harness = setup(cx);
    // Popup triggers measure their anchor during the first render and request
    // a follow-up frame, even while closed. Deliver that initialization before
    // observing navigation so it cannot be mistaken for a wheel-zoom update.
    cx.update_window(harness.window, |_, window, cx| {
        window.simulate_next_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let notifications = Rc::new(Cell::new(0));
    let count = notifications.clone();
    let _observe =
        cx.update(|cx| cx.observe(&harness.view, move |_, _| count.set(count.get() + 1)));
    let anchor = zipzi_editor_core::ViewportPoint { x: 120., y: 90. };
    let position = cx
        .update_window(harness.window, |_, window, _| {
            window.find("canvas-body").bounds().origin + point(px(anchor.x), px(anchor.y))
        })
        .unwrap();
    for initial in [0.2, 1., 8.] {
        let model_point = harness.view.update(cx, |view, _| {
            view.editor.set_zoom_at(initial, None);
            view.editor
                .pan_by(zipzi_editor_core::ViewportPoint { x: 37., y: -12. });
            view.editor
                .preview()
                .unwrap()
                .canvas
                .unwrap()
                .transform
                .to_model(anchor)
        });
        wheel(&harness, cx, position, 1.);
        let mut previous = initial;
        for _ in 0..4 {
            advance_zoom(&harness, cx, 20);
            assert_eq!(
                notifications.get(),
                0,
                "zoom frames must not notify document observers"
            );
            harness.view.update(cx, |view, _| {
                let zoom = view.editor.view().zoom;
                assert!(zoom > previous && zoom <= initial * 1.1 + 1e-5);
                previous = zoom;
                let after = view
                    .editor
                    .preview()
                    .unwrap()
                    .canvas
                    .unwrap()
                    .transform
                    .to_viewport(model_point);
                assert!((after.x - anchor.x).abs() < 1e-3, "x drifted: {after:?}");
                assert!((after.y - anchor.y).abs() < 1e-3, "y drifted: {after:?}");
            });
        }
        assert!((previous / initial - 1.1).abs() < 1e-5);
        wheel(&harness, cx, position, -1.);
        advance_zoom(&harness, cx, 80);
        assert!((navigation_view(&harness, cx).zoom - initial).abs() < 1e-5);
    }
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    assert_eq!(
        notifications.get(),
        0,
        "wheel animation must leave document observers idle"
    );
}

#[gpui_kit::test]
fn wheel_zoom_accumulates_input_and_reverses_without_finishing_old_target(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let initial = navigation_view(&harness, cx).zoom;
    let position = cx
        .update_window(harness.window, |_, window, _| {
            window.find("canvas-body").bounds().center()
        })
        .unwrap();
    wheel(&harness, cx, position, 1.);
    wheel(&harness, cx, position, 1.);
    advance_zoom(&harness, cx, 40);
    let halfway = navigation_view(&harness, cx).zoom;
    assert!(halfway > initial && halfway < initial * 1.21);
    advance_zoom(&harness, cx, 40);
    assert!((navigation_view(&harness, cx).zoom / initial - 1.21).abs() < 1e-5);

    wheel(&harness, cx, position, 4.);
    advance_zoom(&harness, cx, 20);
    let before_reverse = navigation_view(&harness, cx).zoom;
    wheel(&harness, cx, position, -1.);
    advance_zoom(&harness, cx, 20);
    assert!(navigation_view(&harness, cx).zoom < before_reverse);
    advance_zoom(&harness, cx, 60);
    assert!((navigation_view(&harness, cx).zoom - before_reverse / 1.1).abs() < 1e-5);
    let settled = navigation_view(&harness, cx).zoom;
    advance_zoom(&harness, cx, 100);
    assert_eq!(navigation_view(&harness, cx).zoom, settled);
}

#[gpui_kit::test]
fn wheel_zoom_stops_when_other_navigation_takes_over(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let position = cx
        .update_window(harness.window, |_, window, _| {
            window.find("canvas-body").bounds().center()
        })
        .unwrap();
    for takeover in ["pinch", "trackpad", "command", "cancel", "metrics", "modal"] {
        wheel(&harness, cx, position, 3.);
        advance_zoom(&harness, cx, 20);
        cx.update_window(harness.window, |_, window, cx| match takeover {
            "pinch" => {
                window.dispatch_event(
                    PinchEvent {
                        position,
                        delta: 0.,
                        phase: TouchPhase::Started,
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
            }
            "trackpad" => {
                window.dispatch_event(
                    ScrollWheelEvent {
                        position,
                        delta: ScrollDelta::Pixels(point(px(10.), px(-20.))),
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
            }
            "command" => harness.view.update(cx, |view, cx| {
                view.run_command(zipzi_editor_core::CommandId::ResetView, window, cx);
            }),
            "cancel" => {
                window.dispatch_event(
                    ScrollWheelEvent {
                        position,
                        touch_phase: TouchPhase::Cancelled,
                        ..Default::default()
                    }
                    .to_platform_input(),
                    cx,
                );
            }
            "metrics" => harness.view.update(cx, |view, _| {
                let mut metrics = view.editor.canvas_metrics();
                metrics.scale_factor += 0.5;
                view.editor.set_canvas_metrics(metrics);
            }),
            "modal" => harness.view.update(cx, |view, _| {
                view.modal = true;
            }),
            _ => unreachable!(),
        })
        .unwrap();
        let before = navigation_view(&harness, cx);
        advance_zoom(&harness, cx, 100);
        let after = navigation_view(&harness, cx);
        assert_eq!(after.zoom, before.zoom, "{takeover}");
        assert_eq!(after.pan, before.pan, "{takeover}");
        harness.view.update(cx, |view, _| {
            view.modal = false;
        });
    }
}

#[gpui_kit::test]
fn wheel_zoom_uses_the_new_pointer_anchor_when_retargeted(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let initial = navigation_view(&harness, cx).zoom;
    let bounds = cx
        .update_window(harness.window, |_, window, _| {
            window.find("canvas-body").bounds()
        })
        .unwrap();
    wheel(&harness, cx, bounds.center(), 1.);
    advance_zoom(&harness, cx, 20);
    let anchor = zipzi_editor_core::ViewportPoint { x: 90., y: 180. };
    let model = harness.view.update(cx, |view, _| {
        view.editor
            .preview()
            .unwrap()
            .canvas
            .unwrap()
            .transform
            .to_model(anchor)
    });
    wheel(
        &harness,
        cx,
        bounds.origin + point(px(anchor.x), px(anchor.y)),
        1.,
    );
    for _ in 0..4 {
        advance_zoom(&harness, cx, 20);
        let after = harness.view.update(cx, |view, _| {
            view.editor
                .preview()
                .unwrap()
                .canvas
                .unwrap()
                .transform
                .to_viewport(model)
        });
        assert!((after.x - anchor.x).abs() < 1e-3);
        assert!((after.y - anchor.y).abs() < 1e-3);
    }
    assert!((navigation_view(&harness, cx).zoom / initial - 1.21).abs() < 1e-5);
}

#[gpui_kit::test]
fn wheel_zoom_does_not_continue_in_another_document(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let first = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    let second = harness.view.update(cx, |view, _| {
        let second = view.editor.new_document().unwrap();
        view.editor.activate(first).unwrap();
        second
    });
    let position = cx
        .update_window(harness.window, |_, window, _| {
            window.find("canvas-body").bounds().center()
        })
        .unwrap();
    wheel(&harness, cx, position, 4.);
    advance_zoom(&harness, cx, 20);
    let first_zoom = navigation_view(&harness, cx).zoom;
    harness.view.update(cx, |view, _| {
        view.editor.activate(second).unwrap();
    });
    advance_zoom(&harness, cx, 100);
    assert_eq!(navigation_view(&harness, cx).zoom, 1.);
    harness.view.update(cx, |view, _| {
        view.editor.activate(first).unwrap();
    });
    advance_zoom(&harness, cx, 100);
    assert_eq!(navigation_view(&harness, cx).zoom, first_zoom);
}

#[gpui_kit::test]
fn reduced_motion_wheel_zoom_is_immediate_and_clamped(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        cx.set_reduce_motion(true);
        let position = window.find("canvas-body").bounds().center();
        harness.view.update(cx, |view, _| {
            view.editor.set_zoom_at(2., None);
        });
        window.dispatch_event(
            ScrollWheelEvent {
                position,
                delta: ScrollDelta::Lines(point(0., 1.)),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert!((harness.view.read(cx).editor.view().zoom - 2.2).abs() < 1e-5);
        window.dispatch_event(
            ScrollWheelEvent {
                position,
                delta: ScrollDelta::Lines(point(0., 10000.)),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            harness.view.read(cx).editor.view().zoom,
            zipzi_editor_core::MAX_ZOOM
        );
        window.dispatch_event(
            ScrollWheelEvent {
                position,
                delta: ScrollDelta::Lines(point(0., -10000.)),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            harness.view.read(cx).editor.view().zoom,
            zipzi_editor_core::MIN_ZOOM
        );
        window.dispatch_event(
            ScrollWheelEvent {
                position,
                delta: ScrollDelta::Lines(point(0., f32::NAN)),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    let before = navigation_view(&harness, cx).zoom;
    advance_zoom(&harness, cx, 100);
    assert_eq!(navigation_view(&harness, cx).zoom, before);
}
