use super::setup;
use crate::workspace::toolbar::ToolbarItemConfig;
use gpui_kit::assets::IconName;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px,
};

#[gpui_kit::test]
fn hand_tool_drags_canvas_and_select_tool_restores_mode(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_hand", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(view.dock.as_ref().unwrap().canvas.read(cx).hand_tool(cx));
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
            assert!(!view.dock.as_ref().unwrap().canvas.read(cx).hand_tool(cx));
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
fn space_in_parameter_input_keeps_input_focus(cx: &mut TestAppContext) {
    use gpui_kit::component::WindowExt as _;
    let (document, assets) = super::document_fixture::sample_document_with_parameters(1).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let harness = super::setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{parameter}"), cx);
        window.press("space", cx);
        assert!(window.has_focused_input(cx));
        harness.view.read_with(cx, |view, cx| {
            assert!(!view.dock.as_ref().unwrap().canvas.read(cx).hand_tool(cx));
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
        assert!(window.try_find("tool_mesh").is_none());
        assert!(window.try_find("tool_deformer").is_none());
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
                selected: |_, _| false,
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

/// Present a tiny immutable texture with the real CPU frame, so UI picking is
/// exercised through the same image/geometry gate as production.
fn present_canvas(harness: &super::Harness, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let frame = view.editor.preview().unwrap().canvas.unwrap();
            let device = metal::Device::system_default().unwrap();
            let descriptor = metal::TextureDescriptor::new();
            descriptor.set_width(1);
            descriptor.set_height(1);
            descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
            let image = device.new_texture(&descriptor);
            assert!(view.editor.present_frame(frame.clone()));
            view.frame = Some(crate::preview::Presented {
                image,
                canvas: frame,
            });
            cx.notify();
        });
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn surface_click_and_tool_shortcuts_use_document_selection(cx: &mut TestAppContext) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        let (top, point) = harness.view.read_with(cx, |view, _| {
            (
                view.editor
                    .layers()
                    .into_iter()
                    .find(|l| l.name == "Second")
                    .unwrap()
                    .id,
                view.frame
                    .as_ref()
                    .unwrap()
                    .canvas
                    .transform
                    .to_canvas(editor_core::ModelPoint { x: 128., y: 128. }),
            )
        });
        window.click_at("canvas-body", gpui_kit::point(px(point.x), px(point.y)), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().selected_id(),
                Some(top.as_str())
            );
            assert_eq!(view.editor.active_tool(), editor_core::ToolId::Select);
            assert_eq!(view.editor.overlay().vertices.len(), 4);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        window.press("e", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            editor_core::ToolId::Select
        );
        window.press("v", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            editor_core::ToolId::Select
        );
        window.press("d", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            editor_core::ToolId::Select
        );
        window.click("tool_glue", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            editor_core::ToolId::Glue
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn overlap_menu_selects_a_candidate_and_keeps_history(cx: &mut TestAppContext) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    let original = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().context().selection.clone()
    });
    let top = harness.view.read_with(cx, |view, _| {
        view.editor
            .layers()
            .into_iter()
            .find(|l| l.name == "Second")
            .unwrap()
            .id
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.right_click("canvas-body", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_some());
        assert_eq!(
            harness.view.read_with(cx, |view, _| view
                .editor
                .document()
                .unwrap()
                .context()
                .selection
                .clone()),
            original
        );
        window.click_at("popup-menu", point(px(20.), px(18.)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_none());
        harness.view.read_with(cx, |view, cx| {
            assert!(
                view.dock
                    .as_ref()
                    .unwrap()
                    .canvas
                    .read(cx)
                    .focus
                    .is_focused(window)
            );
            assert_eq!(
                view.editor.document().unwrap().selected_id(),
                Some(top.as_str())
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn helper_menu_toggles_preserve_selection_history_and_canvas_focus(cx: &mut TestAppContext) {
    use editor_core::{CommandId, DeformerVisibility};
    let harness = setup(cx);
    present_canvas(&harness, cx);
    let original = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().context().selections.clone()
    });
    for (index, expected) in [
        (
            3usize,
            DeformerVisibility {
                ancestors: true,
                all: false,
            },
        ),
        (
            4usize,
            DeformerVisibility {
                ancestors: true,
                all: true,
            },
        ),
        (
            3usize,
            DeformerVisibility {
                ancestors: false,
                all: true,
            },
        ),
        (4usize, DeformerVisibility::default()),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            window.right_click("canvas-body", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            // Two overlapping mesh candidates, a separator, then the two switches.
            let mut menu = window.within("popup-menu");
            for (row, command) in [
                (3usize, CommandId::ToggleAncestorDeformers),
                (4usize, CommandId::ToggleDeformers),
            ] {
                assert_eq!(
                    menu.find(row).label(),
                    Some(rust_i18n::t!(command.spec().label).as_ref())
                );
            }
            menu.click(index, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("popup-menu").is_none());
            harness.view.read_with(cx, |view, cx| {
                assert_eq!(view.editor.deformer_visibility(), expected);
                assert_eq!(view.menu_state.deformer_visibility, expected);
                assert_eq!(
                    view.editor.document().unwrap().context().selections,
                    original
                );
                assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
                assert!(
                    view.dock
                        .as_ref()
                        .unwrap()
                        .canvas
                        .read(cx)
                        .focus
                        .is_focused(window)
                );
            });
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn vertex_drag_commits_once_and_shortcut_undo_redo_restores_geometry(cx: &mut TestAppContext) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    let original = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().selected_vertex_positions()
    });
    cx.update_window(harness.window, |_, window, cx| {
        let start = harness
            .view
            .read_with(cx, |view, _| view.editor.overlay().vertices[1].position);
        let origin = window.find("canvas-body").bounds().origin;
        let from = origin + point(px(start.x), px(start.y));
        window.drag(from, from + point(px(-24.), px(12.)), cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            let positions = view.editor.document().unwrap().selected_vertex_positions();
            assert_ne!(positions[1], original[1]);
            for i in [0, 2, 3] {
                assert_eq!(positions[i], original[i]);
            }
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            assert!(!view.editor.gesture_active());
        });
        window.press("cmd-z", cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().selected_vertex_positions(),
                original
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 1));
        });
        window.press("cmd-shift-z", cx);
        harness.view.read_with(cx, |view, _| {
            assert_ne!(
                view.editor.document().unwrap().selected_vertex_positions(),
                original
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn escape_tool_switch_and_focus_loss_cancel_a_live_vertex_drag(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    let original = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().selected_vertex_positions()
    });
    for cancel in 0..3 {
        present_canvas(&harness, cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.click("tool_select", cx);
            let start = harness
                .view
                .read_with(cx, |view, _| view.editor.overlay().vertices[1].position);
            let from = window.find("canvas-body").bounds().origin + point(px(start.x), px(start.y));
            let to = from + point(px(-24.), px(12.));
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
                    position: to,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            harness.view.read_with(cx, |view, _| {
                assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
                assert!(view.editor.document().unwrap().has_pending_edit());
                assert_ne!(
                    view.editor.document().unwrap().selected_vertex_positions(),
                    original
                );
            });
            match cancel {
                0 => window.press("escape", cx),
                1 => window.press("h", cx),
                _ => {
                    let focus = harness.view.read_with(cx, |view, _| view.focus.clone());
                    window.focus(&focus, cx);
                }
            }
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            harness.view.read_with(cx, |view, _| {
                assert!(
                    !view.editor.gesture_active(),
                    "cancel path {cancel} left a gesture active"
                );
                assert_eq!(
                    view.editor.document().unwrap().selected_vertex_positions(),
                    original,
                    "cancel path {cancel} must roll back before mouse release"
                );
            });
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: window.find("canvas-body").bounds().center(),
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            harness.view.read_with(cx, |view, _| {
                assert_eq!(
                    view.editor.document().unwrap().selected_vertex_positions(),
                    original
                );
                assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
                assert!(!view.editor.gesture_active());
                assert!(!view.editor.document().unwrap().has_pending_edit());
            });
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn selecting_deformer_shows_handles_without_edit_mode_or_transform_dialog(cx: &mut TestAppContext) {
    use editor_core::{Document, EditorCommand, ObjectKind, ObjectRef, ToolId};
    use kasane_core::{Canvas, Transform, TransformData, Vec2, WarpTransform};
    let assets = tempfile::tempdir().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut session = kasane_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )
    .unwrap();
    session
        .edit("fixture", None, |edit| {
            edit.create_transform(Transform {
                id: id.clone(),
                name: "Warp".into(),
                data: TransformData::Warp(WarpTransform {
                    rows: 1,
                    columns: 1,
                    quad: true,
                    points: vec![
                        Vec2::new(32., 32.),
                        Vec2::new(224., 32.),
                        Vec2::new(32., 224.),
                        Vec2::new(224., 224.),
                    ],
                }),
                ..Default::default()
            })
        })
        .unwrap();
    let saved = session
        .save_project(&assets.path().join("model"), None)
        .unwrap();
    let harness = super::setup_with_document(cx, Document::open(&saved.manifest).unwrap(), assets);
    harness.view.update(cx, |view, cx| {
        view.editor
            .execute(EditorCommand::SelectObject(ObjectRef::new(
                ObjectKind::Deformer,
                &id,
            )))
            .unwrap();
        cx.notify();
    });
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(window.try_find("tool_deformer").is_none());
        assert!(window.try_find("deformer-adjust").is_none());
        assert!(
            window
                .try_find("deformer-input-editor.deformer_angle")
                .is_none()
        );
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.active_tool(), ToolId::Select);
            assert_eq!(view.editor.overlay().vertices.len(), 4);
            assert_eq!(view.editor.overlay().handles.len(), 9);
        });
        window.click("deformer-grid", cx);
        assert!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.deformer_grid())
        );
        let start = harness
            .view
            .read_with(cx, |view, _| view.editor.overlay().vertices[3].position);
        let bounds = window.find("canvas-body").bounds();
        let start = bounds.origin + point(px(start.x), px(start.y));
        window.drag(start, start + point(px(20.), px(0.)), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.active_tool(), ToolId::Select);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        });
    })
    .unwrap();
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        let original = harness.view.read_with(cx, |view, _| {
            view.frame.as_ref().unwrap().canvas.geometry.controls[0]
                .points
                .clone()
        });
        window.click("tool_hand", cx);
        let bounds = window.find("canvas-body").bounds();
        let start = bounds.center();
        window.drag(start, start + point(px(20.), px(0.)), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            assert_eq!(
                view.frame.as_ref().unwrap().canvas.geometry.controls[0].points,
                original
            );
        });
        window.click("tool_select", cx);
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::SelectObjects(Vec::new()))
                .unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("deformer-actions").is_none());
    })
    .unwrap();
}
