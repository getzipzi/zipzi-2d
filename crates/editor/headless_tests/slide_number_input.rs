use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, TestAppContext, Window, point, px,
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
    let mut document = editor_core::Document::new_blank().unwrap();
    let ranges = [(0., 1.), (-30., 30.), (0.13, 0.91), (-100., 100.)];
    let ids: Vec<_> = ranges
        .iter()
        .enumerate()
        .map(|(index, &(min, max))| {
            document
                .create_parameter(editor_core::NewParameter {
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
