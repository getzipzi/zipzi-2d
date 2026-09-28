use super::setup;
use crate::workspace::toolbar::ToolbarItemConfig;
use gpui_kit::assets::IconName;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, InputEvent as _, TestAppContext, point, px};

#[gpui_kit::test]
fn hand_tool_drags_canvas_and_select_tool_restores_mode(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_hand", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(view.dock.as_ref().unwrap().canvas.read(cx).hand_tool());
        });

        let canvas = window.find("canvas-body").bounds();
        let from = canvas.center();
        let to = point(from.x + px(40.), from.y + px(20.));
        let before = harness.view.read_with(cx, |view, _| view.editor.view().pan);
        window.drag(from, to, cx);
        let after = harness.view.read_with(cx, |view, _| view.editor.view().pan);
        assert_eq!(after.x - before.x, 40.);
        assert_eq!(after.y - before.y, 20.);

        window.click("tool_select", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(!view.dock.as_ref().unwrap().canvas.read(cx).hand_tool());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn space_pan_ignores_key_repeats_until_release(cx: &mut TestAppContext) {
    use gpui_kit::{
        KeyDownEvent, KeyUpEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent,
    };

    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let canvas_focus = harness.view.read_with(cx, |view, cx| {
            view.dock.as_ref().unwrap().canvas.read(cx).focus.clone()
        });
        window.focus(&canvas_focus, cx);
        let space = Keystroke::parse("space").unwrap();
        window.dispatch_event(
            KeyDownEvent {
                keystroke: space.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);

        let from = window.find("canvas-body").bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: from + point(px(20.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert!(!window.last_input_was_keyboard());
        let repeated = window.dispatch_event(
            KeyDownEvent {
                keystroke: space.clone(),
                is_held: true,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        assert!(!repeated.propagate);
        assert!(!window.last_input_was_keyboard());
        assert_eq!(
            harness.view.read_with(cx, |v, _| v.editor.view().pan.x),
            20.
        );

        window.dispatch_event(
            KeyUpEvent {
                keystroke: space.clone(),
            }
            .to_platform_input(),
            cx,
        );
        let repeated_after_release = window.dispatch_event(
            KeyDownEvent {
                keystroke: space,
                is_held: true,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        assert!(repeated_after_release.propagate);
        assert!(window.last_input_was_keyboard());
    })
    .unwrap();
}

#[gpui_kit::test]
fn space_after_document_switch_enters_temporary_hand_mode(cx: &mut TestAppContext) {
    use gpui_kit::{KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent};

    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let first_id = harness
            .view
            .read_with(cx, |view, _| view.editor.active_id().unwrap());
        window.press("cmd-n", cx);
        window.render_frame(cx);
        window.click(format!("tab-item-{first_id:?}"), cx);
        let editor_focus = harness.view.read_with(cx, |view, _| view.focus.clone());
        assert!(editor_focus.is_focused(window));

        window.dispatch_event(
            KeyDownEvent {
                keystroke: Keystroke::parse("space").unwrap(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);

        let from = window.find("canvas-body").bounds().center();
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseMoveEvent {
                position: from + point(px(20.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.view().pan.x),
            20.
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn space_in_property_input_keeps_input_focus(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("property-name", cx);
        window.press("space", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(
                view.dock
                    .as_ref()
                    .unwrap()
                    .properties
                    .read(cx)
                    .is_focused(window, cx)
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn floating_toolbar_and_zoom_controls(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        // Verify top bar is removed
        assert!(
            window.try_find("app-toolbar").is_none(),
            "top bar app-toolbar should be removed"
        );

        // Verify floating toolbar exists on canvas
        assert!(
            window.try_find("canvas-floating-toolbar").is_some(),
            "canvas-floating-toolbar should exist"
        );
        assert!(window.try_find("tool_select").is_some());
        assert!(window.try_find("tool_mesh").is_some());
        assert!(window.try_find("tool_deformer").is_some());
        assert!(window.try_find("tool_hand").is_some());

        // A configured button becomes visible through registration.
        let canvas = harness
            .view
            .read_with(cx, |view, _| view.dock.as_ref().unwrap().canvas.clone());
        canvas.update(cx, |canvas, cx| {
            let item = ToolbarItemConfig {
                id: "registered-toolbar-item",
                icon: IconName::Hand,
                tooltip_key: "editor.tool_hand",
                selected: |_| false,
                enabled: |_, modal| !modal,
                action: None,
            };
            assert!(canvas.toolbar.register(item).is_ok());
            assert!(canvas.toolbar.register(item).is_err());
            cx.notify();
        });

        // Verify floating zoom bar exists on canvas
        assert!(
            window.try_find("canvas-floating-zoom").is_some(),
            "canvas-floating-zoom should exist"
        );
        assert!(window.try_find("btn_zoom_out").is_some());
        assert!(window.try_find("zoom-percentage").is_some());
        assert!(window.try_find("btn_zoom_in").is_some());

        // Test zoom buttons
        let initial_zoom = harness
            .view
            .read_with(cx, |view, _| view.editor.view().zoom);
        window.click("btn_zoom_in", cx);
        let zoomed_in = harness
            .view
            .read_with(cx, |view, _| view.editor.view().zoom);
        assert!(zoomed_in > initial_zoom);

        window.click("btn_zoom_out", cx);
        let zoomed_out = harness
            .view
            .read_with(cx, |view, _| view.editor.view().zoom);
        assert!(zoomed_out < zoomed_in);
    })
    .unwrap();

    cx.update_window(harness.window, |_, window, _| {
        assert!(window.try_find("registered-toolbar-item").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn welcome_text_centered_when_no_active_document(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        // Initially active document is loaded, welcome is not displayed
        assert!(window.try_find("canvas-welcome").is_none());

        // Switch to empty editor (active is None)
        harness.view.update(cx, |view, cx| {
            view.editor = editor_core::Editor::empty();
            cx.notify();
        });
        window.render_frame(cx);

        // canvas-welcome is displayed and covers canvas-body
        let canvas_body = window.find("canvas-body").bounds();
        let welcome = window.find("canvas-welcome").bounds();
        assert_eq!(welcome.origin, canvas_body.origin);
        assert_eq!(welcome.size, canvas_body.size);
    })
    .unwrap();
}

#[gpui_kit::test]
fn continuous_pan_does_not_notify_document_panels(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent};
    use std::{cell::Cell, rc::Rc};
    let harness = setup(cx);
    let from = cx
        .update_window(harness.window, |_, window, cx| {
            window.click("tool_hand", cx);
            let from = window.find("canvas-body").bounds().center();
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            from
        })
        .unwrap();
    let notifications = Rc::new(Cell::new(0));
    let observer_count = notifications.clone();
    let _observe = cx.update(|cx| {
        cx.observe(&harness.view, move |_, _| {
            observer_count.set(observer_count.get() + 1);
        })
    });
    for index in 1..=32 {
        cx.update_window(harness.window, |_, window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position: from + point(px(index as f32), px(0.)),
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.draw(cx).clear(cx);
        })
        .unwrap();
    }
    assert_eq!(
        notifications.get(),
        0,
        "pan must leave document observers idle"
    );
    assert_eq!(
        harness.view.read_with(cx, |v, _| v.editor.view().pan.x),
        32.
    );
}

/// CPU input dispatch + GPUI layout/paint only. Run explicitly with a local
/// model; GPU production/composition is measured by the Metal benchmark.
#[gpui_kit::test]
#[ignore]
fn canvas_ui_cpu_benchmark(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent};
    use std::time::Instant;
    let harness = setup(cx);
    let path = std::env::var("KASANE_BENCH_MODEL").expect("KASANE_BENCH_MODEL=model3 path");
    let mut editor = editor_core::Editor::empty();
    let task = editor
        .prepare_load(editor_core::LoadRequest::Model3(path.into()))
        .unwrap()
        .unwrap();
    editor.finish_load(task.run()).unwrap();
    let from = cx
        .update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor = editor;
                cx.notify();
            });
            window.render_frame(cx);
            // Prime a real image/geometry pair so timed paints include the
            // surface and editable overlay, as well as the surrounding UI.
            harness.view.update(cx, |view, cx| {
                objc2::rc::autoreleasepool(|_| {
                    let preview = view.editor.preview().unwrap();
                    let canvas = preview.canvas.unwrap();
                    let (width, height) = canvas.transform.metrics.physical_size().unwrap();
                    let config = editor_render::MetalTargetConfig {
                        width,
                        height,
                        format: metal::MTLPixelFormat::RGBA8Unorm,
                    };
                    let device = metal::Device::system_default().unwrap();
                    let queue = device.new_command_queue();
                    let mut renderer =
                        editor_render::MetalSceneRenderer::new(device, config).unwrap();
                    let submitted = renderer
                        .submit_transformed(&queue, config, &preview.scene, canvas.transform)
                        .unwrap();
                    submitted.command.wait_until_completed();
                    assert!(view.editor.present_frame(canvas.clone()));
                    view.frame = Some(crate::preview::Presented {
                        image: submitted.image,
                        canvas,
                    });
                    cx.notify();
                });
            });
            window.render_frame(cx);
            window.click("tool_hand", cx);
            let from = window.find("canvas-body").bounds().center();
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            from
        })
        .unwrap();
    let mut samples = Vec::new();
    let mut dispatch_samples = Vec::new();
    for index in 0..110 {
        let start = Instant::now();
        let mut dispatch_ms = 0.;
        cx.update_window(harness.window, |_, window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position: from + point(px((index as f32 * 0.1).sin() * 80.), px(0.)),
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            dispatch_ms = start.elapsed().as_secs_f64() * 1000.;
            window.draw(cx).clear(cx);
        })
        .unwrap();
        if index >= 10 {
            samples.push(start.elapsed().as_secs_f64() * 1000.);
            dispatch_samples.push(dispatch_ms);
        }
    }
    samples.sort_by(f64::total_cmp);
    dispatch_samples.sort_by(f64::total_cmp);
    println!(
        "GPUI CPU pan / 100 samples: p50={:.3}ms p95={:.3}ms p99={:.3}ms",
        samples[49], samples[94], samples[98]
    );
    println!(
        "GPUI CPU dispatch / 100 samples: p50={:.3}ms p95={:.3}ms p99={:.3}ms",
        dispatch_samples[49], dispatch_samples[94], dispatch_samples[98]
    );
}
