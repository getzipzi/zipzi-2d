use super::super::setup;
use crate::viewports::toolbar::ToolbarItemConfig;
use gpui_kit::assets::IconName;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, PinchEvent, ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase, point, px,
};

#[gpui_kit::test]
fn hand_tool_drags_canvas_and_select_tool_restores_mode(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_hand", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(
                view.dock
                    .as_ref()
                    .unwrap()
                    .active_viewport()
                    .read(cx)
                    .hand_tool(cx)
            );
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
            assert!(
                !view
                    .dock
                    .as_ref()
                    .unwrap()
                    .active_viewport()
                    .read(cx)
                    .hand_tool(cx)
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn space_never_enters_temporary_hand_and_repeats_are_suppressed(cx: &mut TestAppContext) {
    use gpui_kit::{
        KeyDownEvent, KeyUpEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent,
    };
    let harness = setup(cx);
    let initial_pan = harness.view.read_with(cx, |view, _| view.editor.view().pan);
    cx.update_window(harness.window, |_, window, cx| {
        let focus = harness.view.read_with(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .active_viewport()
                .read(cx)
                .focus
                .clone()
        });
        window.focus(&focus, cx);
        for held in [false, true, true] {
            let result = window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("space").unwrap(),
                    is_held: held,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
            if held {
                assert!(!result.propagate);
            }
        }
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
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: from + point(px(20.), px(0.)),
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            KeyUpEvent {
                keystroke: Keystroke::parse("space").unwrap(),
            }
            .to_platform_input(),
            cx,
        );
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.view().pan, initial_pan);
            assert_eq!(
                view.editor.workspace(),
                editor_core::WorkspaceKind::Modeling
            );
            assert_eq!(view.editor.active_tool(), editor_core::ToolId::Select);
            assert!(
                !view
                    .editor
                    .command_available(view.editor.command(editor_core::CommandId::PlayPause))
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn space_after_document_switch_does_not_pan(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let initial_pan = harness.view.read_with(cx, |view, _| view.editor.view().pan);
    cx.update_window(harness.window, |_, window, cx| {
        let first = harness
            .view
            .read_with(cx, |view, _| view.editor.active_id().unwrap());
        window.press("cmd-n", cx);
        window.render_frame(cx);
        window.click("project-select", cx);
        window.click(format!("project-select-{first:?}"), cx);
        window.press("space", cx);
        window.render_frame(cx);
        let from = window.find("canvas-body").bounds().center();
        window.drag(from, from + point(px(20.), px(0.)), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.view().pan, initial_pan);
            assert_eq!(
                view.editor.workspace(),
                editor_core::WorkspaceKind::Modeling
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn space_in_parameter_input_keeps_input_focus(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let (document, assets) =
        super::super::document_fixture::sample_document_with_parameters(1).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let harness = super::super::setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{parameter}"), cx);
        window.press("space", cx);
        assert!(window.has_focused_input(cx));
        harness.view.read_with(cx, |view, cx| {
            assert!(
                !view
                    .dock
                    .as_ref()
                    .unwrap()
                    .active_viewport()
                    .read(cx)
                    .hand_tool(cx)
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn canvas_owns_floating_tools_and_right_aligned_header_zoom_controls(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let initial_pan = harness.view.read_with(cx, |view, _| view.editor.view().pan);
    cx.update_window(harness.window, |_, window, cx| {
        let top = window.find("workspace-titlebar").bounds();
        let tools = window.find("canvas-toolbar").bounds();
        let canvas = window.find("canvas-body").bounds();
        assert!(top.bottom() <= canvas.top());
        assert!(canvas.contains(&tools.origin));
        assert!(tools.right() <= canvas.right());
        assert!(tools.bottom() <= canvas.bottom());
        assert!(tools.size.height > tools.size.width);
        assert!(window.try_find("workspace-toolbar").is_none());
        assert!(top.contains(&window.find("workspace-layout-menu").bounds().center()));
        assert!(window.try_find("canvas-floating-toolbar").is_none());
        assert!(window.try_find("tool_select").is_some());
        assert!(window.try_find("tool_mesh").is_none());
        assert!(window.try_find("tool_deformer").is_none());
        assert!(window.try_find("tool_hand").is_some());

        // A configured button becomes visible through registration.
        harness.view.update(cx, |view, cx| {
            let item = ToolbarItemConfig {
                id: "registered-toolbar-item",
                icon: IconName::Hand,
                command: editor_core::CommandId::ToggleDeformers,
            };
            view.dock
                .as_ref()
                .unwrap()
                .active_viewport()
                .update(cx, |canvas, cx| {
                    assert!(canvas.toolbar.register(item).is_ok());
                    assert!(canvas.toolbar.register(item).is_err());
                    cx.notify();
                });
            cx.notify();
        });

        assert!(window.try_find("canvas-floating-zoom").is_none());
        let status = window.find("app-status-bar").bounds();
        let zoom = window.find("canvas-zoom-toolbar").bounds();
        let controls = window.find("canvas-zoom-controls").bounds();
        let background = window.find("btn_transparency_grid").bounds();
        assert_eq!(background.left(), zoom.left());
        assert!(background.right() < controls.left());
        assert!((background.center().y - zoom.center().y).abs() <= px(1.));
        assert!(zoom.bottom() <= canvas.top());
        assert!(zoom.top() >= top.bottom());
        assert!(controls.left() >= zoom.left());
        assert!((controls.right() - zoom.right()).abs() <= px(1.));
        assert!((controls.center().y - zoom.center().y).abs() <= px(1.));
        assert!(zoom.bottom() < status.top());
        assert!(window.try_find("status-zoom").is_none());
        assert!(window.try_find("btn_zoom_out").is_some());
        assert!(window.try_find("zoom-percentage").is_some());
        assert!(window.try_find("btn_zoom_in").is_some());
        let segments = [
            "btn_zoom_out",
            "zoom-percentage",
            "btn_zoom_in",
            "btn_zoom_fit",
        ]
        .map(|id| window.find(id).bounds());
        for pair in segments.windows(2) {
            assert_eq!(pair[0].right(), pair[1].left());
        }
        assert_eq!(segments[0].left(), controls.left() + px(1.));
        assert_eq!(segments[3].right(), controls.right() - px(1.));
        assert_eq!(controls.size.height, px(20.));

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

        cx.set_reduce_motion(true);
        // Test scroll wheel zooming centered at mouse cursor
        let before_scroll_pan = harness.view.read_with(cx, |view, _| view.editor.view().pan);
        let canvas_bounds = window.find("canvas-body").bounds();
        let mouse_pos = canvas_bounds.origin + point(px(100.), px(50.));
        window.dispatch_event(
            ScrollWheelEvent {
                position: mouse_pos,
                delta: ScrollDelta::Lines(point(0., 1.)),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        let after_scroll_zoom = harness
            .view
            .read_with(cx, |view, _| view.editor.view().zoom);
        let after_scroll_pan = harness.view.read_with(cx, |view, _| view.editor.view().pan);
        assert!(after_scroll_zoom > zoomed_out);
        assert_ne!(after_scroll_pan, before_scroll_pan);
        window.click("btn_zoom_fit", cx);
        assert_eq!(
            harness.view.read_with(cx, |view, _| view.editor.view().pan),
            initial_pan
        );
        harness.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        });
        window.render_frame(cx);
        let before_disabled = harness.view.read(cx).editor.view().zoom;
        for id in ["btn_zoom_out", "btn_zoom_in", "btn_zoom_fit"] {
            window.click(id, cx);
            assert_eq!(harness.view.read(cx).editor.view().zoom, before_disabled);
        }
        harness.view.update(cx, |view, cx| {
            view.modal = false;
            cx.notify();
        });
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
fn trackpad_scroll_pans_both_axes_without_notifying_document_panels(cx: &mut TestAppContext) {
    use std::{cell::Cell, rc::Rc};

    let harness = setup(cx);
    let (position, before, selection) = cx
        .update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor.zoom_by(1.);
                cx.notify();
            });
            window.render_frame(cx);
            (
                window.find("canvas-body").bounds().center(),
                harness.view.read_with(cx, |view, _| view.editor.view()),
                harness.view.read_with(cx, |view, _| {
                    view.editor.document().unwrap().context().selections.clone()
                }),
            )
        })
        .unwrap();
    let notifications = Rc::new(Cell::new(0));
    let observer_count = notifications.clone();
    let _observe = cx.update(|cx| {
        cx.observe(&harness.view, move |_, _| {
            observer_count.set(observer_count.get() + 1);
        })
    });
    for (delta, phase) in [
        ((40.25, -20.5), TouchPhase::Started),
        ((0.05, 0.), TouchPhase::Moved),
        ((0., 0.), TouchPhase::Ended),
        // Native momentum continues to arrive after the fingers are lifted.
        ((-4., 3.5), TouchPhase::Moved),
        ((0., 0.), TouchPhase::Ended),
        ((100., 100.), TouchPhase::Cancelled),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(delta.0), px(delta.1))),
                    touch_phase: phase,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
        })
        .unwrap();
    }
    harness.view.read_with(cx, |view, _| {
        let after = view.editor.view();
        assert_eq!(after.zoom, before.zoom);
        assert!((after.pan.x - before.pan.x - 36.3).abs() < 1e-4);
        assert_eq!(after.pan.y - before.pan.y, -17.);
        assert_eq!(
            view.editor.document().unwrap().context().selections,
            selection
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    assert_eq!(
        notifications.get(),
        0,
        "trackpad pan must leave document observers idle"
    );
}

#[gpui_kit::test]
fn trackpad_pinch_scales_relative_to_zoom_and_preserves_pointer_anchor(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.set_zoom_at(2., None);
            view.editor
                .pan_by(editor_core::ViewportPoint { x: 37., y: -12. });
            cx.notify();
        });
        window.render_frame(cx);
        let bounds = window.find("canvas-body").bounds();
        let anchor = editor_core::ViewportPoint { x: 100., y: 100. };
        let position = bounds.origin + point(px(anchor.x), px(anchor.y));
        let model_point = harness.view.update(cx, |view, _| {
            view.editor
                .preview()
                .unwrap()
                .canvas
                .unwrap()
                .transform
                .to_model(anchor)
        });
        for (delta, phase, expected_zoom) in [
            (0., TouchPhase::Started, 2.),
            (0.1, TouchPhase::Moved, 2.2),
            (-0.25, TouchPhase::Moved, 1.65),
            (0.2, TouchPhase::Ended, 1.98),
            (100., TouchPhase::Moved, 32.),
            (-0.9999, TouchPhase::Moved, 0.04),
            (0.5, TouchPhase::Cancelled, 0.04),
            (f32::NAN, TouchPhase::Moved, 0.04),
            (-1., TouchPhase::Moved, 0.04),
        ] {
            window.dispatch_event(
                PinchEvent {
                    position,
                    delta,
                    phase,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            harness.view.update(cx, |view, _| {
                assert!((view.editor.view().zoom - expected_zoom).abs() < 1e-5);
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
                assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
            });
        }
        window.dispatch_event(
            PinchEvent {
                position: bounds.origin - point(px(20.), px(20.)),
                delta: 0.5,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.view().zoom),
            0.04
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn control_or_command_trackpad_scroll_zooms_at_pointer(cx: &mut TestAppContext) {
    use gpui_kit::Modifiers;

    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, _| {
            view.editor.set_zoom_at(2., None);
        });
        let anchor = editor_core::ViewportPoint { x: 100., y: 100. };
        let position =
            window.find("canvas-body").bounds().origin + point(px(anchor.x), px(anchor.y));
        for modifiers in [
            Modifiers {
                control: true,
                ..Default::default()
            },
            Modifiers {
                platform: true,
                ..Default::default()
            },
        ] {
            let (before, model_point) = harness.view.update(cx, |view, _| {
                (
                    view.editor.view().zoom,
                    view.editor
                        .preview()
                        .unwrap()
                        .canvas
                        .unwrap()
                        .transform
                        .to_model(anchor),
                )
            });
            window.dispatch_event(
                ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Pixels(point(px(80.), px(16.))),
                    modifiers,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            harness.view.update(cx, |view, _| {
                assert!((view.editor.view().zoom - before * 1.1).abs() < 1e-5);
                let after = view
                    .editor
                    .preview()
                    .unwrap()
                    .canvas
                    .unwrap()
                    .transform
                    .to_viewport(model_point);
                assert!((after.x - anchor.x).abs() < 1e-3);
                assert!((after.y - anchor.y).abs() < 1e-3);
            });
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn continuous_pan_does_not_notify_document_panels(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent};
    use std::{cell::Cell, rc::Rc};
    let harness = setup(cx);
    let initial_pan = harness.view.read_with(cx, |view, _| view.editor.view().pan);
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
        initial_pan.x + 32.
    );
}

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
                    view.viewport_frames
                        .entry(view.viewport_slots[0])
                        .or_default()
                        .modeling = Some(crate::preview::Presented {
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
    let workload = std::env::var("KASANE_BENCH_WORKLOAD").unwrap_or_else(|_| "pan".into());
    assert!(matches!(workload.as_str(), "pan" | "idle"));
    for index in 0..110 {
        let start = Instant::now();
        let mut dispatch_ms = 0.;
        cx.update_window(harness.window, |_, window, cx| {
            if workload == "pan" {
                window.dispatch_event(
                    MouseMoveEvent {
                        position: from + point(px((index as f32 * 0.1).sin() * 80.), px(0.)),
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
            }
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
        "GPUI CPU {workload} / 100 samples: p50={:.3}ms p95={:.3}ms p99={:.3}ms",
        samples[49], samples[94], samples[98]
    );
    println!(
        "GPUI CPU dispatch / 100 samples: p50={:.3}ms p95={:.3}ms p99={:.3}ms",
        dispatch_samples[49], dispatch_samples[94], dispatch_samples[98]
    );
}
