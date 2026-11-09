use super::super::{Harness, setup};
use super::common::present_canvas;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    App, AppContext as _, BorrowAppContext as _, InputEvent as _, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    TestAppContext, Window, point, px,
};
use zipzi_editor_core::{EditorCommand, ToolId, ViewportPoint};

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
fn brush_select_size_field_edits_settings_and_keeps_text_keys_out_of_canvas(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::{WindowExt, input::AnyInputState};
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_brush_select", cx);
        window.click("brush-selection-size", cx);
        assert!(window.focused_input(cx).is_some());
        window.input("96", cx);
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("size input not focused");
        };
        assert_eq!(input.read(cx).value().parse::<f32>().unwrap(), 96.);
        window.press("enter", cx);
    })
    .unwrap();
    // InputState subscriptions publish after the native event boundary.
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .brush_selection_settings()
                .diameter,
            96.
        );
        window.press("secondary-a", cx);
        window.press("b", cx);
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("size input not focused");
        };
        assert_eq!(input.read(cx).value().parse::<f32>().unwrap(), 96.);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.input_router.brush_size_key.get().is_none());
        assert_eq!(view.editor.active_tool(), ToolId::BrushSelect);
        assert_eq!(view.editor.brush_selection_settings().diameter, 96.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

fn down(
    window: &mut Window,
    cx: &mut App,
    point: gpui_kit::Point<gpui_kit::Pixels>,
    modifiers: Modifiers,
) {
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position: point,
            modifiers,
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
}
fn up(
    window: &mut Window,
    cx: &mut App,
    point: gpui_kit::Point<gpui_kit::Pixels>,
    modifiers: Modifiers,
) {
    window.dispatch_event(
        MouseUpEvent {
            button: MouseButton::Left,
            position: point,
            modifiers,
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
}

#[gpui_kit::test]
fn brush_select_native_paint_add_erase_cancel_and_tool_shortcuts(cx: &mut TestAppContext) {
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
        window.click("tool_brush_select", cx);
        assert!(window.find("brush-selection-settings").visible());
        assert_eq!(
            harness.view.read(cx).editor.active_tool(),
            ToolId::BrushSelect
        );
        harness.view.update(cx, |view, _| {
            view.editor
                .execute(EditorCommand::SetBrushSelectionSettings(
                    zipzi_editor_core::BrushSelectionSettings {
                        diameter: 8.,
                        weight: 1.,
                        ..Default::default()
                    },
                ))
                .unwrap()
        });
        window.render_frame(cx);
        let origin = window.find("canvas-body").bounds().origin;
        let at = |p: ViewportPoint| origin + point(px(p.x), px(p.y));
        for (vertex, modifiers, count) in [
            (&vertices[0], Modifiers::default(), 1),
            (
                &vertices[1],
                Modifiers {
                    shift: true,
                    ..Default::default()
                },
                2,
            ),
            (
                &vertices[0],
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
                1,
            ),
        ] {
            down(window, cx, at(vertex.position), modifiers);
            assert_eq!(selected(&harness, cx).len(), count);
            up(window, cx, at(vertex.position), modifiers);
            window.render_frame(cx);
        }
        let before = selected(&harness, cx);
        down(window, cx, at(vertices[0].position), Modifiers::default());
        assert_ne!(selected(&harness, cx), before);
        window.press("escape", cx);
        assert_eq!(selected(&harness, cx), before);
        up(window, cx, at(vertices[0].position), Modifiers::default());
        window.press("v", cx);
        assert_eq!(harness.view.read(cx).editor.active_tool(), ToolId::Select);
        assert_eq!(selected(&harness, cx), before);
        window.press("l", cx);
        assert_eq!(harness.view.read(cx).editor.active_tool(), ToolId::Lasso);
        window.press("b", cx);
        assert_eq!(
            harness.view.read(cx).editor.active_tool(),
            ToolId::BrushSelect
        );
        assert_eq!(selected(&harness, cx), before);
        let view = harness.view.read(cx);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(
            view.editor.document().unwrap().selected_vertex_positions(),
            original
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn brush_select_native_b_drag_changes_size_without_paint_and_escape_restores_it(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_brush_select", cx);
        let vertex = harness.view.read(cx).editor.overlay().vertices[0].clone();
        let origin = window.find("canvas-body").bounds().origin;
        let start = origin + point(px(vertex.position.x), px(vertex.position.y));
        let end = start + point(px(20.), px(0.));
        let size = harness
            .view
            .read(cx)
            .editor
            .brush_selection_settings()
            .diameter;
        let before = selected(&harness, cx);
        for cancel in [true, false] {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("b").unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
            down(window, cx, start, Modifiers::default());
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
            assert_eq!(
                harness
                    .view
                    .read(cx)
                    .editor
                    .brush_selection_settings()
                    .diameter,
                size + 40.
            );
            assert_eq!(selected(&harness, cx), before);
            if cancel {
                window.press("escape", cx);
                assert_eq!(
                    harness
                        .view
                        .read(cx)
                        .editor
                        .brush_selection_settings()
                        .diameter,
                    size
                );
            }
            up(window, cx, end, Modifiers::default());
            window.dispatch_event(
                KeyUpEvent {
                    keystroke: Keystroke::parse("b").unwrap(),
                }
                .to_platform_input(),
                cx,
            );
            assert!(!harness.view.read(cx).editor.gesture_active());
        }
        // Releasing B must restore painting on the following stroke.
        down(window, cx, start, Modifiers::default());
        up(window, cx, start, Modifiers::default());
        assert!(selected(&harness, cx).contains(&vertex.id));
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn weighted_brush_native_cmd_drag_moves_by_accumulated_weight_without_repainting(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_brush_select", cx);
        harness.view.update(cx, |view, _| {
            view.editor
                .execute(EditorCommand::SetBrushSelectionSettings(
                    zipzi_editor_core::BrushSelectionSettings {
                        diameter: 8.,
                        weight: 0.25,
                        hardness: 1.,
                        ..Default::default()
                    },
                ))
                .unwrap()
        });
        let vertex = harness.view.read(cx).editor.overlay().vertices[0].clone();
        let zoom = harness.view.read(cx).editor.view().zoom;
        let before = harness
            .view
            .read(cx)
            .editor
            .document()
            .unwrap()
            .selected_vertex_positions();
        let origin = window.find("canvas-body").bounds().origin;
        let start = origin + point(px(vertex.position.x), px(vertex.position.y));
        for (shift, expected) in [(false, 0.25), (true, 0.4375), (false, 0.25), (true, 0.4375)] {
            let modifiers = Modifiers {
                shift,
                ..Default::default()
            };
            down(window, cx, start, modifiers);
            up(window, cx, start, modifiers);
            let weights = harness
                .view
                .read(cx)
                .editor
                .overlay()
                .selection_weights
                .unwrap();
            assert_eq!(weights.points.len(), 1);
            assert!((weights.points[0].1 - expected).abs() < 1e-6);
        }
        harness.view.update(cx, |view, _| {
            view.editor
                .execute(EditorCommand::SetBrushSelectionSettings(
                    zipzi_editor_core::BrushSelectionSettings {
                        weight: 0.9,
                        ..view.editor.brush_selection_settings()
                    },
                ))
                .unwrap();
        });
        let modifiers = Modifiers {
            platform: true,
            ..Default::default()
        };
        down(window, cx, start, modifiers);
        window.dispatch_event(
            MouseMoveEvent {
                position: start + point(px(40.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers,
            }
            .to_platform_input(),
            cx,
        );
        up(window, cx, start + point(px(40.), px(0.)), modifiers);
        let view = harness.view.read(cx);
        let after = view.editor.document().unwrap().selected_vertex_positions();
        assert!((after[0].1.x - before[0].1.x - 17.5 / zoom).abs() < 0.001);
        assert_eq!(view.editor.active_tool(), ToolId::BrushSelect);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert_eq!(view.editor.brush_selection_settings().weight, 0.9);
    })
    .unwrap();
}

#[gpui_kit::test]
fn brush_deform_mode_native_drag_and_cmd_miss_preserve_old_weights(cx: &mut TestAppContext) {
    for cancel in [false, true] {
        let harness = setup(cx);
        present_canvas(&harness, cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.click("tool_brush_select", cx);
            harness.view.update(cx, |view, _| {
                view.editor
                    .execute(EditorCommand::SetBrushSelectionSettings(
                        zipzi_editor_core::BrushSelectionSettings {
                            diameter: 40.,
                            weight: 0.25,
                            hardness: 1.,
                            ..Default::default()
                        },
                    ))
                    .unwrap();
            });
            let vertices = harness.view.read(cx).editor.overlay().vertices;
            let origin = window.find("canvas-body").bounds().origin;
            let old = origin + point(px(vertices[2].position.x), px(vertices[2].position.y));
            down(window, cx, old, Modifiers::default());
            up(window, cx, old, Modifiers::default());
            let zoom = harness.view.read(cx).editor.view().zoom;
            let before = harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .selected_vertex_positions();
            let start =
                origin + point(px(vertices[0].position.x + 10.), px(vertices[0].position.y));
            let cmd = Modifiers {
                platform: true,
                ..Default::default()
            };
            down(window, cx, start, cmd);
            assert!(!harness.view.read(cx).editor.gesture_active());
            up(window, cx, start, cmd);
            assert_eq!(
                harness
                    .view
                    .read(cx)
                    .editor
                    .document()
                    .unwrap()
                    .selected_vertex_positions(),
                before
            );
            for (button, expected) in [
                ("brush-mode-deform", zipzi_editor_core::BrushMode::Deform),
                ("brush-mode-inflate", zipzi_editor_core::BrushMode::Inflate),
                ("brush-mode-deform", zipzi_editor_core::BrushMode::Deform),
                ("brush-mode-deform", zipzi_editor_core::BrushMode::Select),
                ("brush-mode-deform", zipzi_editor_core::BrushMode::Deform),
            ] {
                window.click(button, cx);
                assert_eq!(harness.view.read(cx).editor.brush_mode(), expected);
            }
            down(window, cx, start, Modifiers::default());
            assert_eq!(
                harness.view.read(cx).editor.overlay().active_gesture,
                Some(zipzi_editor_core::CanvasGestureKind::Move)
            );
            assert_eq!(selected(&harness, cx), vec![vertices[0].id]);
            let end = start + point(px(40.), px(0.));
            window.dispatch_event(
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
            let after = harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .selected_vertex_positions();
            assert!((after[0].1.x - before[0].1.x - 10. / zoom).abs() < 0.001);
            if cancel {
                window.press("escape", cx);
            }
            up(window, cx, end, Modifiers::default());
            let view = harness.view.read(cx);
            assert_eq!(
                view.editor.document().unwrap().selected_vertex_positions(),
                if cancel { before } else { after }
            );
            assert_eq!(selected(&harness, cx), vec![vertices[2].id]);
            let weights = view.editor.overlay().selection_weights.unwrap();
            assert_eq!(weights.points.len(), 1);
            assert_eq!(weights.points[0].1, 0.25);
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (usize::from(!cancel), 0)
            );
            assert!(!view.editor.gesture_active());
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn brush_cmd_cursor_updates_without_pointer_motion_and_tracks_the_captured_operation(
    cx: &mut TestAppContext,
) {
    use crate::tool_cursors::ToolCursor;
    use gpui_kit::ModifiersChangedEvent;
    use zipzi_editor_core::CanvasSelectionOperation as O;
    fn modifiers(window: &mut Window, cx: &mut App, platform: bool) {
        window.dispatch_event(
            ModifiersChangedEvent {
                modifiers: Modifiers {
                    platform,
                    ..Default::default()
                },
                capslock: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
    }
    fn cursor(harness: &Harness, window: &mut Window, cx: &mut App) -> Option<ToolCursor> {
        window.render_frame(cx);
        let view = harness.view.read(cx);
        view.viewport_surface(view.viewport_slots[0], cx)
            .unwrap()
            .read(cx)
            .tool_cursor_kind
    }
    let harness = setup(cx);
    present_canvas(&harness, cx);
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    let check = |visual: &mut gpui_kit::VisualTestContext, expected| {
        visual.run_until_parked();
        visual.update(|window, cx| {
            assert_eq!(
                cursor(&harness, window, cx),
                Some(expected),
                "active={}, modifiers={:?}, gesture={}, tool={:?}",
                window.is_window_active(),
                window.modifiers(),
                harness.view.read(cx).editor.gesture_active(),
                harness.view.read(cx).editor.active_tool()
            );
        });
    };
    let start = visual.update(|window, cx| {
        window.activate_window();
        window.click("tool_brush_select", cx);
        let vertex = harness.view.read(cx).editor.overlay().vertices[0].position;
        let start = window.find("canvas-body").bounds().origin + point(px(vertex.x), px(vertex.y));
        down(window, cx, start, Modifiers::default());
        up(window, cx, start, Modifiers::default());
        start
    });
    check(&mut visual, ToolCursor::Brush(O::Replace));
    // Stationary modifier changes update idle feedback, but captured painting
    // retains both its cursor operation and the existing range preview semantics.
    for (pressed, operation) in [
        (
            Modifiers {
                shift: true,
                ..Default::default()
            },
            O::Add,
        ),
        (
            Modifiers {
                alt: true,
                ..Default::default()
            },
            O::Remove,
        ),
    ] {
        visual.update(|window, cx| {
            window.dispatch_event(
                ModifiersChangedEvent {
                    modifiers: pressed,
                    capslock: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
        });
        check(&mut visual, ToolCursor::Brush(operation));
        visual.update(|window, cx| down(window, cx, start, pressed));
        visual.update(|window, cx| modifiers(window, cx, false));
        check(&mut visual, ToolCursor::Brush(operation));
        visual.update(|_, cx| {
            assert_eq!(
                harness
                    .view
                    .read(cx)
                    .editor
                    .overlay()
                    .selection_brush
                    .unwrap()
                    .subtract,
                operation == O::Remove
            );
        });
        visual.update(|window, cx| up(window, cx, start, Modifiers::default()));
        check(&mut visual, ToolCursor::Brush(O::Replace));
    }
    // Each native event finishes before inspecting the resulting rendered cursor.
    // No mouse move is needed when only the keyboard modifiers change.
    visual.update(|window, cx| modifiers(window, cx, true));
    check(&mut visual, ToolCursor::Move);
    visual.update(|window, cx| modifiers(window, cx, false));
    check(&mut visual, ToolCursor::Brush(O::Replace));
    visual.update(|window, cx| down(window, cx, start, Modifiers::default()));
    visual.update(|window, cx| modifiers(window, cx, true));
    check(&mut visual, ToolCursor::Brush(O::Replace));
    let cmd = Modifiers {
        platform: true,
        ..Default::default()
    };
    visual.update(|window, cx| up(window, cx, start, cmd));
    check(&mut visual, ToolCursor::Move);
    visual.update(|window, cx| down(window, cx, start, cmd));
    check(&mut visual, ToolCursor::Move);
    visual.update(|window, cx| modifiers(window, cx, false));
    check(&mut visual, ToolCursor::Move);
    visual.update(|window, cx| up(window, cx, start, Modifiers::default()));
    check(&mut visual, ToolCursor::Brush(O::Replace));
    visual.update(|window, cx| {
        modifiers(window, cx, true);
        down(window, cx, start, cmd);
    });
    check(&mut visual, ToolCursor::Move);
    visual.deactivate_window();
    visual.update(|window, cx| {
        assert!(!harness.view.read(cx).editor.gesture_active());
        assert_eq!(
            cursor(&harness, window, cx),
            Some(ToolCursor::Brush(O::Replace))
        );
        window.activate_window();
        modifiers(window, cx, false);
        up(window, cx, start, Modifiers::default());
        assert_eq!(
            cursor(&harness, window, cx),
            Some(ToolCursor::Brush(O::Replace))
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    });
}

#[gpui_kit::test]
fn brush_select_toolbar_fits_labels_and_maximum_values_at_all_scales(cx: &mut TestAppContext) {
    use crate::settings::{Settings, UiScale};
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_brush_select", cx);
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::SetBrushSelectionSettings(
                    zipzi_editor_core::BrushSelectionSettings {
                        weight: 1.,
                        diameter: zipzi_editor_core::BrushSelectionSettings::MAX_DIAMETER,
                        ..Default::default()
                    },
                ))
                .unwrap();
            cx.notify();
        });
    })
    .unwrap();
    for scale in UiScale::ALL {
        cx.update_window(harness.window, |_, window, cx| {
            cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale).unwrap());
            crate::theme::change(crate::theme::ColorTheme::Dark, cx);
            window.refresh();
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let canvas = window.find("canvas-body").bounds();
            let bar = window.find("brush-selection-settings").bounds();
            let primary = window.find("canvas-toolbar").bounds();
            assert!(bar.left() >= canvas.left() && bar.right() <= canvas.right());
            assert!(
                bar.left() >= primary.right(),
                "{scale:?}: brush bar overlaps the main toolbar"
            );
            let fields = [
                "brush-mode-deform",
                "brush-mode-inflate",
                "brush-selection-weight-label",
                "brush-selection-weight",
                "brush-selection-size-label",
                "brush-selection-size",
                "brush-selection-cycle-shape",
                "brush-selection-more-button",
            ]
            .map(|id| window.find(id).bounds());
            for (index, field) in fields.iter().enumerate() {
                assert!(
                    field.left() >= bar.left() && field.right() <= bar.right(),
                    "{scale:?}: {field:?} outside {bar:?}"
                );
                assert!(field.top() >= bar.top() && field.bottom() <= bar.bottom());
                assert!(fields[index + 1..].iter().all(|other| {
                    field.right() <= other.left()
                        || other.right() <= field.left()
                        || field.bottom() <= other.top()
                        || other.bottom() <= field.top()
                }));
            }
            for (label, input, min_width) in
                [(fields[2], fields[3], 72.), (fields[4], fields[5], 80.)]
            {
                assert!((label.center().y - input.center().y).abs().as_f32() < 0.5);
                assert!(input.size.width.as_f32() >= min_width * scale.factor());
            }
            window.click("brush-selection-more-button", cx);
            window.render_frame(cx);
            let details = window.find("brush-selection-details").bounds();
            assert!(details.size.width.as_f32() <= 260. * scale.factor() + 1.);
            assert!(details.size.height.as_f32() <= 160. * scale.factor() + 1.);
            assert!(window.find("brush-selection-shape").visible());
            assert!(window.find("brush-selection-hardness").visible());
            assert!(window.find("brush-selection-angle").visible());
            assert!(window.find("brush-selection-color").visible());
            window.press("escape", cx);
            window.render_frame(cx);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn brush_color_picker_interactions_keep_settings_open_and_close_independently(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_brush_select", cx);
        window.click("brush-selection-more-button", cx);
        window.click("brush-selection-color", cx);
        window.render_frame(cx);
        assert!(window.find("brush-selection-details").visible());
        assert!(window.find("color-picker-panel").visible());
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        let parent = window.find("brush-selection-details").bounds();
        let hex = window.find("color-picker-hex").bounds();
        assert!(
            !parent.contains(&hex.center()),
            "picker control must exercise a click outside its parent settings surface"
        );
        window.click("color-picker-hex", cx);
        window.render_frame(cx);
        assert!(window.find("brush-selection-details").visible());
        assert!(window.find("color-picker-panel").visible());
        window.click("color-picker-cancel", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find("color-picker-panel")
                .is_none_or(|panel| !panel.visible())
        );
        assert!(window.find("brush-selection-details").visible());
        window.click("brush-selection-more-button", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find("brush-selection-details")
                .is_none_or(|panel| !panel.visible())
        );
    })
    .unwrap();
}
