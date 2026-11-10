use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, RelativeMouseMoveEvent, TestAppContext, Window, point, px,
};

pub(super) fn press(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
}

pub(super) fn slide(window: &mut Window, position: Point<Pixels>, shift: bool, cx: &mut App) {
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers {
                shift,
                ..Default::default()
            },
        }
        .to_platform_input(),
        cx,
    );
}

pub(super) fn release(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
}

fn value(harness: &Harness, cx: &mut TestAppContext) -> f32 {
    harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().parameters()[0].value
    })
}

#[gpui_kit::test]
fn number_slide_preserves_fractional_movement_through_percent_value_feedback(
    cx: &mut TestAppContext,
) {
    use zipzi_editor_core::{ObjectKind, property::deformer::SCALE};

    let (mut document, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let object = document_fixture::object(ObjectKind::Deformer, 501);
    document.navigate(object.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    super::properties::show(&harness, "property-input-scale", cx);
    let original = harness.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .property_values(&object)
            .unwrap()[&SCALE.key]
            .clone()
    });

    // Both paths traverse 113%, whose authoritative 1.13 * 100 feedback
    // formats differently from the emitted 113. Each frame must retain the
    // fractional movement even when mouse events cover less than one step.
    for stride in [1, 8] {
        let origin = cx
            .update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                let origin = window
                    .within("property-input-scale")
                    .find("slide-number-input")
                    .bounds()
                    .center();
                press(window, origin, cx);
                origin
            })
            .unwrap();
        for distances in [
            (stride..=160).step_by(stride).collect::<Vec<_>>(),
            (0..160).step_by(stride).rev().collect(),
        ] {
            let endpoint = *distances.last().unwrap();
            for distance in distances {
                cx.update_window(harness.window, |_, window, cx| {
                    slide(
                        window,
                        origin + point(px(distance as f32), px(0.)),
                        false,
                        cx,
                    );
                    window.render_frame(cx);
                })
                .unwrap();
            }
            harness.view.read_with(cx, |view, _| {
                let doc = view.editor.document().unwrap();
                let preview = doc.object_index().get(&object).unwrap().properties["rotation"]
                    ["pose"]["scale"]
                    .as_f64()
                    .unwrap();
                let expected = if endpoint == 160 { 1.2 } else { 1. };
                assert!(
                    (preview - expected).abs() < 1e-6,
                    "stride {stride}, endpoint {endpoint}: {preview} != {expected}",
                );
            });
        }
        cx.update_window(harness.window, |_, window, cx| release(window, origin, cx))
            .unwrap();
        harness.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(doc.property_values(&object).unwrap()[&SCALE.key], original);
        });
    }
}

#[gpui_kit::test]
fn number_drag_keeps_escape_and_blocks_viewport_commands_outside_its_panel(
    cx: &mut TestAppContext,
) {
    let (document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let id = document.parameters()[0].id.clone();
    let original = document.parameters()[0].value;
    let harness = setup_with_document(cx, document, assets);
    let (origin, destination, view_before) = cx
        .update_window(harness.window, |_, window, cx| {
            let origin = window
                .find(format!("parameter-value-{id}"))
                .bounds()
                .center();
            let destination = window.find("canvas-body").bounds().center() + point(px(80.), px(0.));
            let view_before = harness.view.read(cx).editor.view();
            press(window, origin, cx);
            (origin, destination, view_before)
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(20.), px(0.)), false, cx);
    })
    .unwrap();
    assert_ne!(value(&harness, cx), original);
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, destination, false, cx);
        window.press("secondary-=", cx);
        assert_eq!(harness.view.read(cx).editor.view(), view_before);
        window.press("escape", cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), original);
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, destination + point(px(40.), px(0.)), false, cx);
        release(window, destination, cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), original);
}

#[gpui_kit::test]
fn number_slide_is_relative_precise_clamped_and_stops_on_release(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            let origin = window
                .find(format!("parameter-value-{id}"))
                .bounds()
                .center();
            let row = window.find(format!("parameter-row-{id}")).bounds();
            let field = window
                .within(format!("parameter-value-{id}"))
                .find("slide-number-input")
                .bounds();
            assert_eq!(field.size.height, px(18.));
            assert_eq!(field.top() - row.top(), px(2.));
            assert_eq!(row.bottom() - field.bottom(), px(2.));
            press(window, origin, cx);
            origin
        })
        .unwrap();
    assert_eq!(value(&harness, cx), 0.);
    // Ignore small hand movement, so a click does not become an edit.
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(2.), px(0.)), false, cx)
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.);
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(20.), px(0.)), false, cx)
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.05);
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(40.), px(0.)), true, cx)
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.055);
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(60.), px(0.)), false, cx)
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.105);
    // Capture continues outside the field and clamps at the endpoint.
    let outside = origin + point(px(1000.), px(100.));
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, outside, false, cx)
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 1.);
    // Reversing responds immediately, without traversing accumulated overshoot.
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, outside - point(px(20.), px(0.)), false, cx)
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.95);
    cx.update_window(harness.window, |_, window, cx| {
        release(window, outside, cx);
        slide(window, origin, false, cx);
        assert!(!cx.has_active_drag());
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.95);
    harness.view.read_with(cx, |view, _| {
        assert!(!view.editor.document().unwrap().modified())
    });
}

#[gpui_kit::test]
fn number_slide_accumulates_relative_motion_with_a_stationary_pointer(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let pointer = cx
        .update_window(harness.window, |_, window, cx| {
            let origin = window
                .find(format!("parameter-value-{id}"))
                .bounds()
                .center();
            press(window, origin, cx);
            let pointer = origin + point(px(20.), px(0.));
            slide(window, pointer, false, cx);
            pointer
        })
        .unwrap();
    let relative_move = |dx, window: &mut Window, cx: &mut App| {
        window.dispatch_event(
            RelativeMouseMoveEvent {
                delta: point(px(dx), px(12.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(window.mouse_position(), pointer);
    };

    for expected in [0.15, 0.25, 0.35] {
        cx.update_window(harness.window, |_, window, cx| {
            relative_move(400., window, cx)
        })
        .unwrap();
        assert_eq!(value(&harness, cx), expected);
    }
    cx.update_window(harness.window, |_, window, cx| {
        relative_move(-200., window, cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.30);
    cx.update_window(harness.window, |_, window, cx| {
        release(window, pointer, cx);
        relative_move(400., window, cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.30);
}

#[gpui_kit::test]
fn escape_restores_the_precise_value_before_sliding(cx: &mut TestAppContext) {
    let (document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let start = cx
        .update_window(harness.window, |_, window, cx| {
            let position = window
                .find(format!("parameter-value-{id}"))
                .bounds()
                .center();
            press(window, position, cx);
            position
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, start + point(px(20.), px(0.)), false, cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.169);
    cx.update_window(harness.window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    assert_eq!(value(&harness, cx), 0.13);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-key-{id}-0.375"), cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.375);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let origin = window
                .find(format!("parameter-value-{id}"))
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(20.), px(0.)), false, cx)
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.414);
    cx.update_window(harness.window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    assert_eq!(value(&harness, cx), 0.375);
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(40.), px(0.)), false, cx);
        release(window, origin, cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.375);
}

#[gpui_kit::test]
fn modal_disables_an_active_number_slide(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            let origin = window
                .find(format!("parameter-value-{id}"))
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        });
        window.render_frame(cx);
        slide(window, origin + point(px(80.), px(0.)), false, cx);
        release(window, origin, cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.modal = false;
            cx.notify();
        });
        window.render_frame(cx);
        slide(window, origin + point(px(80.), px(0.)), false, cx);
    })
    .unwrap();
    assert_eq!(value(&harness, cx), 0.);
}

#[gpui_kit::test]
fn number_slide_stops_when_the_window_loses_focus(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let origin = visual.update(|window, cx| {
        window.activate_window();
        let origin = window
            .find(format!("parameter-value-{id}"))
            .bounds()
            .center();
        press(window, origin, cx);
        origin
    });
    visual.update(|window, cx| slide(window, origin + point(px(20.), px(0.)), false, cx));
    assert_eq!(value(&harness, cx), 0.05);
    visual.deactivate_window();
    visual.update(|window, cx| {
        window.activate_window();
        slide(window, origin + point(px(80.), px(0.)), false, cx);
        release(window, origin, cx);
    });
    assert_eq!(value(&harness, cx), 0.05);
}

#[gpui_kit::test]
fn different_parameter_ranges_cover_min_to_max_over_the_same_distance(cx: &mut TestAppContext) {
    let mut document = zipzi_editor_core::Document::new_blank().unwrap();
    let ranges = [(0., 1.), (-30., 30.), (0.13, 0.91), (-100., 100.)];
    let ids: Vec<_> = ranges
        .iter()
        .enumerate()
        .map(|(index, &(min, max))| {
            document
                .create_parameter(zipzi_editor_core::NewParameter {
                    name: format!("Range {index}"),
                    runtime_id: format!("Range{index}"),
                    minimum: min,
                    maximum: max,
                    default_value: min,
                    ..Default::default()
                })
                .unwrap()
        })
        .collect();
    let harness = setup_with_document(cx, document, tempfile::tempdir().unwrap());
    for (id, (min, max)) in ids.iter().zip(ranges) {
        let origin = cx
            .update_window(harness.window, |_, window, cx| {
                let origin = window
                    .find(format!("parameter-value-{id}"))
                    .bounds()
                    .center();
                press(window, origin, cx);
                origin
            })
            .unwrap();
        for (distance, expected) in [
            (100., min + (max - min) / 4.),
            (200., min + (max - min) / 2.),
            (400., max),
            (200., min + (max - min) / 2.),
            (0., min),
        ] {
            cx.update_window(harness.window, |_, window, cx| {
                slide(window, origin + point(px(distance), px(0.)), false, cx);
            })
            .unwrap();
            harness.view.read_with(cx, |view, _| {
                let parameter = view
                    .editor
                    .document()
                    .unwrap()
                    .parameters()
                    .into_iter()
                    .find(|parameter| &parameter.id == id)
                    .unwrap();
                assert!(
                    (parameter.value - expected).abs() < 0.00001,
                    "range {min}..{max}, distance {distance}: {} != {expected}",
                    parameter.value
                );
            });
        }
        cx.update_window(harness.window, |_, window, cx| release(window, origin, cx))
            .unwrap();
    }
}
