use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};

fn canvas_cursor(h: &Harness, cx: &gpui_kit::App) -> Option<gpui_kit::CursorStyle> {
    let view = h.view.read(cx);
    view.viewport_surface(view.viewport_slots[0], cx)
        .unwrap()
        .read(cx)
        .tool_cursor
}
fn canvas_cursor_kind(h: &Harness, cx: &gpui_kit::App) -> Option<crate::tool_cursors::ToolCursor> {
    let view = h.view.read(cx);
    view.viewport_surface(view.viewport_slots[0], cx)
        .unwrap()
        .read(cx)
        .tool_cursor_kind
}

pub(super) fn present(h: &Harness, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let frame = view.editor.preview().unwrap().canvas.unwrap();

            assert!(view.editor.present_frame(frame.clone()));
            view.viewport_frames
                .entry(view.viewport_slots[0])
                .or_default()
                .modeling = Some(crate::preview::Presented {
                image: crate::preview::test_image(),
                canvas: frame,
            });
            cx.notify();
        });
        window.render_frame(cx);
    })
    .unwrap();
}
fn enter(cx: &mut TestAppContext) -> Harness {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool_manual_mesh", cx);
    })
    .unwrap();
    assert!(
        !cx.has_pending_prompt(),
        "Mesh Edit must open directly without source-image confirmation"
    );
    assert!(!h.view.read_with(cx, |v, _| v.modal));
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        (0, 0)
    );
    present(&h, cx);
    assert!(h.view.read_with(cx, |v, _| v.editor.manual_active()));
    h.view.read_with(cx, |view, cx| {
        let message = view
            .status_bar
            .read(cx)
            .displayed_message()
            .map(|(text, _)| text)
            .unwrap_or("");
        assert!(
            !message.contains('⌘'),
            "Mesh instructions should live in the tooltip, not the status bar"
        );
    });
    h
}

#[gpui_kit::test]
fn manual_mesh_v_exits_to_the_select_tool(cx: &mut TestAppContext) {
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("v", cx);
        assert!(!h.view.read(cx).editor.manual_active());
        assert_eq!(
            h.view.read(cx).editor.active_tool(),
            zipzi_editor_core::ToolId::Select
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_failed_click_explains_why_and_next_valid_click_recovers(cx: &mut TestAppContext) {
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        let transform = h.view.read(cx).modeling_frame().unwrap().canvas.transform;
        let p = transform.to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 244. });
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
        let view = h.view.read(cx);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(
            view.editor.canvas_notice().is_some(),
            "A rejected create click must show a reason after the mouse is released"
        );
        let p = transform.to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 128. });
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
        assert!(h.view.read(cx).editor.canvas_notice().is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_toolbar_only_shows_relevant_tools(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tool_manual_create").is_none());
        assert!(window.try_find("tool_manual_exit").is_none());
        assert!(window.try_find("mesh-edit-actions").is_none());
        assert!(window.try_find("tool_generate_mesh").is_none());
        window.click("tool_manual_mesh", cx);
    })
    .unwrap();
    assert!(!cx.has_pending_prompt());
    present(&h, cx);
    cx.update_window(h.window, |_, window, _| {
        assert!(window.find("tool_select").visible());
        assert!(window.find("tool_lasso").visible());
        assert!(window.try_find("tool_glue").is_none());
        assert!(window.find("mesh-edit-actions").visible());
        let secondary = window.find("mesh-edit-actions").bounds();
        for tool in ["tool_manual_create", "tool_manual_delete"] {
            assert!(window.try_find(tool).is_none());
        }
        assert!(window.find("tool_manual_source").visible());
        let primary = window.find("tool_manual_mesh").bounds().size;
        let tools = [
            "tool_manual_select",
            "tool_manual_lasso",
            "tool_generate_mesh",
            "tool_manual_reconnect",
            "tool_manual_pad_source",
            "tool_manual_source",
        ];
        for tool in tools {
            assert!(window.find(tool).visible());
            assert_eq!(window.find(tool).bounds().size, primary);
        }
        for pair in tools.windows(2) {
            assert!(window.find(pair[0]).bounds().right() <= window.find(pair[1]).bounds().left());
        }
        assert!(window.try_find("tool_manual_exit").is_none());
        assert!(window.try_find("manual-mesh-help").is_none());
        assert!(secondary.contains(&window.find("tool_generate_mesh").bounds().center()));
    })
    .unwrap();
    cx.update_window(h.window, |_, window, cx| window.click("tool_select", cx))
        .unwrap();
    present(&h, cx);
    cx.update_window(h.window, |_, window, _| {
        assert!(window.try_find("mesh-edit-actions").is_none());
        assert!(window.try_find("tool_generate_mesh").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_box_and_lasso_buttons_choose_their_mode_without_toggling_off(
    cx: &mut TestAppContext,
) {
    use zipzi_editor_core::ManualTool;
    let h = enter(cx);
    for (button, tool) in [
        ("tool_manual_lasso", ManualTool::Lasso),
        ("tool_manual_lasso", ManualTool::Lasso),
        ("tool_manual_select", ManualTool::Select),
        ("tool_manual_select", ManualTool::Select),
    ] {
        cx.update_window(h.window, |_, window, cx| {
            window.click(button, cx);
            assert!(h.view.read(cx).editor.manual_active());
            assert_eq!(h.view.read(cx).editor.manual_tool(), Some(tool));
            assert_eq!(
                h.view.read(cx).editor.document().unwrap().history_lengths(),
                (0, 0)
            );
        })
        .unwrap();
        present(&h, cx);
    }
}

#[gpui_kit::test]
fn manual_mesh_source_padding_button_updates_texture_and_undo_in_both_views(
    cx: &mut TestAppContext,
) {
    for source_view in [false, true] {
        let h = enter(cx);
        let original = h.view.read_with(cx, |view, _| {
            view.editor.manual_target().unwrap().texture.clone()
        });
        if source_view {
            cx.update_window(h.window, |_, window, cx| {
                window.click("tool_manual_source", cx)
            })
            .unwrap();
            present(&h, cx);
        }
        cx.update_window(h.window, |_, window, cx| {
            window.click("tool_manual_pad_source", cx)
        })
        .unwrap();
        present(&h, cx);
        h.view.read_with(cx, |view, _| {
            assert!(view.editor.manual_active());
            let texture = &view.editor.manual_target().unwrap().texture;
            assert_eq!(
                (texture.width, texture.height),
                (original.width + 8, original.height + 8)
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            assert!(view.edit_error.is_none());
        });
        cx.update_window(h.window, |_, window, cx| window.press("cmd-z", cx))
            .unwrap();
        present(&h, cx);
        h.view.read_with(cx, |view, _| {
            assert!(view.editor.manual_active());
            assert_eq!(
                view.editor.manual_target().unwrap().texture.rgba,
                original.rgba
            );
            assert_eq!(
                view.editor.manual_target().unwrap().texture.width,
                original.width
            );
        });
    }
}

#[gpui_kit::test]
fn manual_mesh_entry_reports_shared_source_texture(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool_manual_mesh", cx);
        assert!(!h.view.read(cx).editor.manual_active());
        assert!(
            h.view.read(cx).edit_error.is_some(),
            "Unsupported targets need an explanation, not an inert button"
        );
    })
    .unwrap();
    assert!(!cx.has_pending_prompt());
}

#[gpui_kit::test]
fn manual_mesh_tool_switch_retains_completed_edits_and_undo(cx: &mut TestAppContext) {
    use zipzi_editor_core::ToolId;
    let h = enter(cx);
    let point = zipzi_editor_core::ModelPoint { x: 128., y: 128. };
    pointer_gesture(&h, cx, point, point, gpui_kit::Modifiers::default());
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| window.click("tool_select", cx))
        .unwrap();
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(!h.view.read(cx).editor.manual_active());
        assert!(window.try_find("tool_manual_exit").is_none());
        assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 5);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
        window.press("cmd-z", cx);
        assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 4);
        window.click("tool_lasso", cx);
        assert_eq!(h.view.read(cx).editor.active_tool(), ToolId::Lasso);
    })
    .unwrap();
}

pub(super) fn pointer_gesture(
    h: &Harness,
    cx: &mut TestAppContext,
    from: zipzi_editor_core::ModelPoint,
    to: zipzi_editor_core::ModelPoint,
    modifiers: gpui_kit::Modifiers,
) {
    use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
    present(h, cx);
    cx.update_window(h.window, |_, window, cx| {
        let transform = h.view.read(cx).modeling_frame().unwrap().canvas.transform;
        let origin = window.find("canvas-body").bounds().origin;
        let screen = |p| {
            let p = transform.to_viewport(p);
            origin + point(px(p.x), px(p.y))
        };
        let start = screen(from);
        let end = screen(to);
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        if start != end {
            // Releasing Command mid-drag must keep the operation chosen on press.
            window.dispatch_event(
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
        }
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_plain_mouse_and_delete_key_leave_select_active(cx: &mut TestAppContext) {
    use gpui_kit::Modifiers;
    use zipzi_editor_core::{ManualTool, ModelPoint};
    for lasso in [false, true] {
        let h = enter(cx);
        if lasso {
            cx.update_window(h.window, |_, window, cx| {
                window.click("tool_manual_lasso", cx)
            })
            .unwrap();
            present(&h, cx);
        }
        let tool = Some(if lasso {
            ManualTool::Lasso
        } else {
            ManualTool::Select
        });
        let center = ModelPoint { x: 128., y: 128. };
        let moved = ModelPoint { x: 140., y: 140. };
        pointer_gesture(&h, cx, center, center, Modifiers::default());
        h.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.layers()[0].vertices, 5);
            assert_eq!(view.editor.manual_tool(), tool);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        });
        pointer_gesture(&h, cx, center, moved, Modifiers::default());
        h.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.layers()[0].vertices, 5);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        });
        cx.update_window(h.window, |_, window, cx| window.press("delete", cx))
            .unwrap();
        h.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.layers()[0].vertices, 4);
            assert_eq!(view.editor.manual_tool(), tool);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (3, 0));
        });
        present(&h, cx);
        cx.update_window(h.window, |_, window, cx| {
            window.press("cmd-z", cx);
            assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 5);
        })
        .unwrap();
        present(&h, cx);
        // Plain click + Backspace uses the existing selection deletion shortcut.
        pointer_gesture(&h, cx, moved, moved, Modifiers::default());
        cx.update_window(h.window, |_, window, cx| {
            window.press("backspace", cx);
            assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 4);
            assert_eq!(h.view.read(cx).editor.manual_tool(), tool);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn manual_mesh_primary_connect_keeps_gesture_after_modifier_release(cx: &mut TestAppContext) {
    use zipzi_editor_core::{ManualTool, ModelPoint};
    let h = enter(cx);
    pointer_gesture(
        &h,
        cx,
        ModelPoint { x: 24., y: 24. },
        ModelPoint { x: 128., y: 128. },
        gpui_kit::Modifiers {
            platform: true,
            ..Default::default()
        },
    );
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let host = doc.runtime_host();
        let snapshot =
            crate::test_fixture::published_snapshot(&host, doc.runtime_session_id()).unwrap();
        let mesh = snapshot
            .document()
            .mesh(&view.editor.manual_target().unwrap().object.id)
            .unwrap();
        assert_eq!(mesh.vertex_ids.len(), 5);
        assert_eq!(mesh.meshing.as_ref().unwrap().constraints.len(), 1);
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(view.editor.manual_tool(), Some(ManualTool::Select));
    });
}

#[gpui_kit::test]
fn manual_lasso_cursor_uses_the_shared_selection_operation(cx: &mut TestAppContext) {
    use crate::tool_cursors::ToolCursor;
    use gpui_kit::{
        InputEvent as _, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent,
        MouseMoveEvent,
    };
    use zipzi_editor_core::CanvasSelectionOperation as O;
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("tool_manual_lasso", cx)
    })
    .unwrap();
    for (modifiers, operation) in [
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
        // Tool changes and cancellation can retire a frame without writing geometry.
        present(&h, cx);
        cx.update_window(h.window, |_, window, cx| {
            let transform = h.view.read(cx).modeling_frame().unwrap().canvas.transform;
            let point_on_canvas =
                transform.to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 128. });
            let position = window.find("canvas-body").bounds().origin
                + point(px(point_on_canvas.x), px(point_on_canvas.y));
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                ModifiersChangedEvent {
                    modifiers,
                    capslock: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            assert_eq!(
                canvas_cursor_kind(&h, cx),
                Some(ToolCursor::Lasso(operation))
            );
            window.dispatch_event(
                MouseDownEvent {
                    position,
                    modifiers,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            assert!(h.view.read(cx).editor.gesture_active());
            window.dispatch_event(
                ModifiersChangedEvent {
                    modifiers: Modifiers::default(),
                    capslock: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            assert_eq!(
                canvas_cursor_kind(&h, cx),
                Some(ToolCursor::Lasso(operation))
            );
            window.press("escape", cx);
            window.render_frame(cx);
            assert_eq!(canvas_cursor_kind(&h, cx), Some(ToolCursor::MeshAdd));
            assert_eq!(
                h.view.read(cx).editor.document().unwrap().history_lengths(),
                (0, 0)
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn manual_mesh_delete_cursor_keeps_the_pressed_operation_until_cancel(cx: &mut TestAppContext) {
    use crate::tool_cursors::ToolCursor;
    use gpui_kit::{
        InputEvent as _, Modifiers, ModifiersChangedEvent, MouseButton, MouseDownEvent,
    };
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        let vertex = h.view.read(cx).editor.overlay().vertices[0].position;
        let position =
            window.find("canvas-body").bounds().origin + point(px(vertex.x), px(vertex.y));
        let modifiers = Modifiers {
            platform: true,
            shift: true,
            ..Default::default()
        };
        window.dispatch_event(
            MouseDownEvent {
                position,
                button: MouseButton::Left,
                modifiers,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(canvas_cursor_kind(&h, cx), Some(ToolCursor::MeshDelete));
        window.dispatch_event(
            ModifiersChangedEvent {
                modifiers: Modifiers::default(),
                capslock: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(canvas_cursor_kind(&h, cx), Some(ToolCursor::MeshDelete));
        assert_eq!(
            h.view.read(cx).editor.manual_tool(),
            Some(zipzi_editor_core::ManualTool::Select)
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert_ne!(canvas_cursor_kind(&h, cx), Some(ToolCursor::MeshDelete));
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_native_cursor_yields_to_shortcuts_and_modal(cx: &mut TestAppContext) {
    use gpui_kit::{
        CursorStyle, CustomCursor, InputEvent as _, Modifiers, ModifiersChangedEvent,
        MouseMoveEvent, size,
    };
    let h = enter(cx);
    // The test platform intentionally falls back during registration. Supply a
    // resource ID here to exercise viewport request routing independently of OS creation.
    let custom = CursorStyle::Custom(
        CustomCursor::from_svg(
            include_bytes!("../assets/cursors/mesh-add.svg"),
            size(24, 24),
            point(7, 7),
            CursorStyle::Crosshair,
        )
        .unwrap()
        .id(),
    );
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.tool_cursors.mesh_add = custom;
            let command = view
                .editor
                .command(zipzi_editor_core::CommandId::ManualCreate)
                .with_source(zipzi_editor_core::CommandSource::NativeMenu);
            view.run_invocation(command, window, cx);
        });
        let transform = h.view.read(cx).modeling_frame().unwrap().canvas.transform;
        let point_on_canvas =
            transform.to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 128. });
        let position = window.find("canvas-body").bounds().origin
            + point(px(point_on_canvas.x), px(point_on_canvas.y));
        window.dispatch_event(
            MouseMoveEvent {
                position,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        let cursor = |cx: &gpui_kit::App| {
            let view = h.view.read(cx);
            view.viewport_surface(view.viewport_slots[0], cx)
                .unwrap()
                .read(cx)
                .tool_cursor
        };
        assert_eq!(cursor(cx), Some(custom));
        for modifiers in [
            Modifiers {
                platform: true,
                shift: true,
                ..Default::default()
            },
            Modifiers::default(),
        ] {
            window.dispatch_event(
                ModifiersChangedEvent {
                    modifiers,
                    capslock: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            if modifiers.platform {
                assert_ne!(cursor(cx), Some(custom));
            } else {
                assert_eq!(cursor(cx), Some(custom));
            }
        }
        h.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(cursor(cx), None);
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_cmd_cursor_updates_without_hint_overlay_and_click_fixes_an_edge(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{
        CursorStyle, InputEvent as _, Modifiers, ModifiersChangedEvent, MouseButton,
        MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    };
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        let view = h.view.read(cx);
        let binding = view.editor.primary_viewport_binding().unwrap();
        let (a, b) = view.editor.overlay_for_viewport(binding).automatic_lines[0];
        let position = window.find("canvas-body").bounds().origin
            + point(px((a.x + b.x) * 0.5), px((a.y + b.y) * 0.5));
        window.dispatch_event(
            MouseMoveEvent {
                position,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("manual-mesh-connection-hint").is_none());
        let cmd = Modifiers {
            platform: true,
            ..Default::default()
        };
        // The pointer remains stationary while Command changes.
        window.dispatch_event(
            ModifiersChangedEvent {
                modifiers: cmd,
                capslock: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("manual-mesh-connection-hint").is_none());
        assert_eq!(canvas_cursor(&h, cx), Some(CursorStyle::PointingHand));
        assert_eq!(
            canvas_cursor_kind(&h, cx),
            Some(crate::tool_cursors::ToolCursor::MeshFixEdge)
        );
        window.dispatch_event(
            MouseDownEvent {
                position,
                button: MouseButton::Left,
                modifiers: cmd,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            ModifiersChangedEvent {
                modifiers: Modifiers::default(),
                capslock: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(canvas_cursor(&h, cx), Some(CursorStyle::PointingHand));
        assert_eq!(
            canvas_cursor_kind(&h, cx),
            Some(crate::tool_cursors::ToolCursor::MeshFixEdge)
        );
        window.dispatch_event(
            MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 4);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
    })
    .unwrap();
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.try_find("manual-mesh-connection-hint").is_none());
        let editor = &h.view.read(cx).editor;
        assert!(
            editor
                .overlay_for_viewport(editor.primary_viewport_binding().unwrap())
                .automatic_lines
                .is_empty()
        );
        window.dispatch_event(
            ModifiersChangedEvent {
                modifiers: Modifiers {
                    platform: true,
                    ..Default::default()
                },
                capslock: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert_eq!(
            canvas_cursor(&h, cx),
            Some(CursorStyle::OperationNotAllowed)
        );
        window.dispatch_event(
            ModifiersChangedEvent {
                modifiers: Modifiers::default(),
                capslock: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        assert!(window.try_find("manual-mesh-connection-hint").is_none());
        window.press("cmd-z", cx);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 1)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_connect_preview_tracks_target_after_cmd_release(cx: &mut TestAppContext) {
    use gpui_kit::{
        CursorStyle, InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent,
    };
    use zipzi_editor_core::{ManualMeshFeedback, ModelPoint};
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        let frame = h.view.read(cx).modeling_frame().unwrap().canvas.clone();
        let binding = frame.version.binding;
        let origin = window.find("canvas-body").bounds().origin;
        let screen = |p| {
            let p = frame.transform.to_viewport(p);
            origin + point(px(p.x), px(p.y))
        };
        let from = screen(ModelPoint { x: 24., y: 24. });
        window.dispatch_event(
            MouseDownEvent {
                position: from,
                button: MouseButton::Left,
                modifiers: Modifiers {
                    platform: true,
                    ..Default::default()
                },
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        for (target, expected, cursor) in [
            (
                ModelPoint { x: 80., y: 128. },
                ManualMeshFeedback::ConnectNewVertex,
                CursorStyle::PointingHand,
            ),
            (
                ModelPoint { x: 232., y: 232. },
                ManualMeshFeedback::ConnectVertex,
                CursorStyle::PointingHand,
            ),
        ] {
            window.dispatch_event(
                MouseMoveEvent {
                    position: screen(target),
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let overlay = h.view.read(cx).editor.overlay_for_viewport(binding);
            assert_eq!(overlay.manual_mesh_feedback, Some(expected));
            assert_eq!(overlay.gizmo_lines.len(), 1);
            assert_eq!(canvas_cursor(&h, cx), Some(cursor));
            assert_eq!(
                canvas_cursor_kind(&h, cx),
                Some(crate::tool_cursors::ToolCursor::MeshConnect {
                    new_vertex: expected == ManualMeshFeedback::ConnectNewVertex,
                })
            );
            assert!(window.try_find("manual-mesh-connection-hint").is_none());
            assert!(h.view.read(cx).status_bar.read(cx).hints().is_empty());
        }
        window.dispatch_event(
            MouseUpEvent {
                position: screen(ModelPoint { x: 232., y: 232. }),
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 4);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_right_click_opens_menu_without_editing_geometry(cx: &mut TestAppContext) {
    let h = enter(cx);
    let point = zipzi_editor_core::ModelPoint { x: 128., y: 128. };
    pointer_gesture(&h, cx, point, point, gpui_kit::Modifiers::default());
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        let p = h
            .view
            .read(cx)
            .modeling_frame()
            .unwrap()
            .canvas
            .transform
            .to_viewport(point);
        window.click_with_options(
            "canvas-body",
            gpui_kit::test::ClickOptions::new()
                .with_offset(gpui_kit::point(px(p.x), px(p.y)))
                .with_button(gpui_kit::MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_some());
        assert!(window.find("tool_manual_reconnect").visible());
        assert!(h.view.read(cx).editor.manual_active());
        assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 5);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
        let reconnect = window.within("popup-menu").find(1usize);
        // Disabled menu rows do not expose a disabled snapshot flag; verify
        // that clicking the unavailable action cannot mutate the document.
        window.within("popup-menu").click(2usize, cx);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
        assert!(h.view.read(cx).edit_error.is_none());
        assert_eq!(
            reconnect.label(),
            Some(crate::t!("editor.tool_manual_reconnect").as_ref())
        );
        assert_ne!(reconnect.disabled(), Some(true));
        window.within("popup-menu").click(1usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("popup-menu").is_none());
        assert!(h.view.read(cx).editor.manual_active());
        assert_eq!(h.view.read(cx).editor.layers()[0].vertices, 5);
        assert!(h.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_tree_selection_exits_and_selects_in_one_click(cx: &mut TestAppContext) {
    use zipzi_editor_core::{ObjectKind, ObjectRef};
    let h = enter(cx);
    let other = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(903).to_string());
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("parts-item-Part:{}", other.id), cx);
        let view = h.view.read(cx);
        assert!(!view.editor.manual_active());
        assert_eq!(
            view.editor.document().unwrap().context().selection,
            Some(other.clone())
        );
        assert_eq!(
            view.editor.active_inspection().unwrap().object(),
            Some(other)
        );
        assert!(view.edit_error.is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_property_rename_and_new_parameter_exit_and_complete(cx: &mut TestAppContext) {
    let h = enter(cx);
    let target = h
        .view
        .read_with(cx, |view, _| view.editor.manual_target().unwrap().object);
    super::properties::fill(&h, "property-input-name", "Renamed during Mesh Edit", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(!h.view.read(cx).editor.manual_active());
        window.click("tool_manual_mesh", cx);
        window.render_frame(cx);
        assert!(h.view.read(cx).editor.manual_active());
        super::create_panel_object(window, "parameters-create", 0, cx);
        window.render_frame(cx);
    })
    .unwrap();
    present(&h, cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert!(!view.editor.manual_active());
        assert_eq!(
            doc.context().inspected.as_ref().unwrap().kind,
            zipzi_editor_core::ObjectKind::Parameter
        );
        assert_eq!(doc.parameters().len(), 1);
        assert_eq!(
            doc.property_values(&target).unwrap()[&zipzi_editor_core::property::mesh::NAME.key],
            zipzi_editor_core::PropertyValue::Text("Renamed during Mesh Edit".into())
        );
        assert_eq!(doc.history_lengths(), (2, 0));
    });
}

#[gpui_kit::test]
fn manual_mesh_other_tool_buttons_and_shortcuts_exit_in_one_action(cx: &mut TestAppContext) {
    use zipzi_editor_core::ToolId;
    let h = enter(cx);
    for (button, shortcut, tool) in [
        (Some("tool_select"), "", ToolId::Select),
        (None, "h", ToolId::Hand),
        (Some("tool_lasso"), "", ToolId::Lasso),
        (None, "l", ToolId::Lasso),
        (None, "", ToolId::Glue),
    ] {
        cx.update_window(h.window, |_, window, cx| {
            if let Some(button) = button {
                window.click(button, cx);
            } else if !shortcut.is_empty() {
                window.press(shortcut, cx);
            } else {
                // Glue has no default key binding; use the native-menu command path.
                h.view.update(cx, |view, cx| {
                    let command = view
                        .editor
                        .command(zipzi_editor_core::CommandId::GlueTool)
                        .with_source(zipzi_editor_core::CommandSource::NativeMenu);
                    view.run_invocation(command, window, cx);
                });
            }
            assert!(
                !h.view.read(cx).editor.manual_active(),
                "{tool:?} did not exit Mesh Edit"
            );
            assert_eq!(h.view.read(cx).editor.active_tool(), tool);
            assert!(h.view.read(cx).edit_error.is_none());
            window.render_frame(cx);
            window.click("tool_manual_mesh", cx);
            assert!(h.view.read(cx).editor.manual_active());
        })
        .unwrap();
        present(&h, cx);
    }
}

#[gpui_kit::test]
fn manual_mesh_context_menu_sets_and_releases_constraints_with_undo(cx: &mut TestAppContext) {
    use zipzi_editor_core::ModelPoint;
    let h = enter(cx);
    pointer_gesture(
        &h,
        cx,
        ModelPoint { x: 24., y: 24. },
        ModelPoint { x: 128., y: 128. },
        gpui_kit::Modifiers {
            platform: true,
            ..Default::default()
        },
    );
    present(&h, cx);
    let mesh = |cx: &mut TestAppContext| {
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            let host = doc.runtime_host();
            let snapshot =
                crate::test_fixture::published_snapshot(&host, doc.runtime_session_id()).unwrap();
            snapshot
                .document()
                .mesh(&view.editor.manual_target().unwrap().object.id)
                .unwrap()
                .clone()
        })
    };
    let before = mesh(cx);
    assert_eq!(before.meshing.as_ref().unwrap().constraints.len(), 1);
    cx.update_window(h.window, |_, window, cx| {
        let p = h
            .view
            .read(cx)
            .modeling_frame()
            .unwrap()
            .canvas
            .transform
            .to_viewport(ModelPoint { x: 76., y: 76. });
        window.click_with_options(
            "canvas-body",
            gpui_kit::test::ClickOptions::new()
                .with_offset(point(px(p.x), px(p.y)))
                .with_button(gpui_kit::MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let release = window.within("popup-menu").find(2usize);
        assert_eq!(
            release.label(),
            Some(crate::t!("editor.manual_release_constraint").as_ref())
        );
        assert_ne!(release.disabled(), Some(true));
        window.within("popup-menu").click(2usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    let after = mesh(cx);
    assert_eq!(after.vertex_ids, before.vertex_ids);
    assert_eq!(after.base_positions, before.base_positions);
    assert_eq!(after.uvs, before.uvs);
    assert_eq!(after.triangles, before.triangles);
    assert!(after.meshing.as_ref().unwrap().constraints.is_empty());
    h.view.read_with(cx, |view, _| {
        assert!(view.editor.manual_active());
        assert!(view.edit_error.is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
    cx.update_window(h.window, |_, window, cx| window.press("cmd-z", cx))
        .unwrap();
    assert_eq!(mesh(cx).meshing, before.meshing);
    cx.update_window(h.window, |_, window, cx| window.press("cmd-shift-z", cx))
        .unwrap();
    assert_eq!(mesh(cx).meshing, after.meshing);

    // The released edge is now automatic; fix it again through the menu,
    // without Command or inserting either endpoint.
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        let p = h
            .view
            .read(cx)
            .modeling_frame()
            .unwrap()
            .canvas
            .transform
            .to_viewport(ModelPoint { x: 76., y: 76. });
        window.click_with_options(
            "canvas-body",
            gpui_kit::test::ClickOptions::new()
                .with_offset(point(px(p.x), px(p.y)))
                .with_button(gpui_kit::MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let fix = window.within("popup-menu").find(2usize);
        assert_eq!(
            fix.label(),
            Some(crate::t!("editor.manual_constrain_edge").as_ref())
        );
        window.within("popup-menu").click(2usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    let fixed = mesh(cx);
    assert_eq!(fixed.meshing, before.meshing);
    assert_eq!(fixed.vertex_ids, before.vertex_ids);
    assert_eq!(fixed.base_positions, before.base_positions);
    assert_eq!(fixed.uvs, before.uvs);
    assert_eq!(fixed.triangles, before.triangles);
    h.view.read_with(cx, |view, _| {
        assert!(view.editor.manual_active());
        assert!(view.edit_error.is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (3, 0));
    });
    cx.update_window(h.window, |_, window, cx| window.press("cmd-z", cx))
        .unwrap();
    assert_eq!(mesh(cx).meshing, after.meshing);
}

#[gpui_kit::test]
fn manual_mesh_mixed_dependencies_enter_edit_and_generate_interior(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::mixed_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool_manual_mesh", cx);
    })
    .unwrap();
    present(&h, cx);
    assert!(h.view.read_with(cx, |v, _| v.editor.manual_active()));
    cx.update_window(h.window, |_, window, cx| {
        let view = h.view.read(cx);
        let frame = &view.modeling_frame().unwrap().canvas;
        assert!(
            frame
                .geometry
                .source
                .viewport_frame
                .drawables
                .iter()
                .any(|d| d.id == "a8210000-0000-4000-8000-000000000004"
                    && d.visible
                    && d.texture_slot >= 0)
        );
        let mesh = frame
            .geometry
            .mesh(&view.editor.manual_target().unwrap().object.id)
            .unwrap();
        let p = frame.transform.to_viewport(zipzi_editor_core::ModelPoint {
            x: (mesh.vertices[0].1.x + mesh.vertices[1].1.x + mesh.vertices[4].1.x) / 3.,
            y: (mesh.vertices[0].1.y + mesh.vertices[1].1.y + mesh.vertices[4].1.y) / 3.,
        });
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
    })
    .unwrap();
    present(&h, cx);
    h.view.read_with(cx, |v, _| {
        let doc = v.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        let host = doc.runtime_host();
        let snapshot =
            crate::test_fixture::published_snapshot(&host, doc.runtime_session_id()).unwrap();
        let model = snapshot.document();
        let id = "a8210000-0000-4000-8000-000000000002";
        assert_eq!(model.mesh(id).unwrap().vertex_ids.len(), 6);
        assert_eq!(
            model.binding_for_mesh(id).unwrap().keyforms[0]
                .positions
                .len(),
            6
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("tool_generate_mesh", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, _| {
        assert!(window.find("mesh-generation-stage-interior").visible());
        assert!(window.find("mesh-generation-stage-edges").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_model_and_source_views_both_edit_and_keep_their_cameras(cx: &mut TestAppContext) {
    let h = enter(cx);
    let model_view = h.view.read_with(cx, |v, _| {
        let frame = &v.modeling_frame().unwrap().canvas;
        assert!(
            frame
                .geometry
                .source
                .viewport_frame
                .drawables
                .iter()
                .all(|d| !d.id.starts_with("__manual"))
        );
        assert!(
            v.editor
                .overlay_for_viewport(frame.version.binding)
                .image_bounds
                .is_empty()
        );
        assert!(!v.editor.viewport_isolated(frame.version.binding));
        v.editor.viewport_view(frame.version.binding).unwrap()
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click("tool_manual_source", cx);
    })
    .unwrap();
    present(&h, cx);
    h.view.read_with(cx, |v, _| {
        let frame = &v.modeling_frame().unwrap().canvas;
        assert_eq!(frame.geometry.source.viewport_frame.drawables.len(), 1);
        assert!(
            frame.geometry.source.viewport_frame.drawables[0]
                .id
                .starts_with("__manual")
        );
        assert_eq!(
            v.editor
                .overlay_for_viewport(frame.version.binding)
                .image_bounds
                .len(),
            4
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        let frame = &h.view.read(cx).modeling_frame().unwrap().canvas;
        let p = frame.transform.to_viewport(zipzi_editor_core::ModelPoint {
            x: 96. * 0.4,
            y: 64. * 0.3,
        });
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
    })
    .unwrap();
    present(&h, cx);
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        (1, 0)
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("tool_manual_source", cx);
    })
    .unwrap();
    present(&h, cx);
    h.view.read_with(cx, |v, _| {
        let frame = &v.modeling_frame().unwrap().canvas;
        assert_eq!(
            v.editor.viewport_view(frame.version.binding).unwrap(),
            model_view
        );
        assert_eq!(
            v.editor
                .overlay_for_viewport(frame.version.binding)
                .vertices
                .iter()
                .filter(|v| v.selected)
                .count(),
            1
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        let frame = &h.view.read(cx).modeling_frame().unwrap().canvas;
        let p = frame.transform.to_viewport(zipzi_editor_core::ModelPoint {
            x: 24. + 208. * 0.7,
            y: 24. + 208. * 0.6,
        });
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
    })
    .unwrap();
    present(&h, cx);
    assert_eq!(
        h.view
            .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths()),
        (2, 0)
    );
}

#[gpui_kit::test]
fn object_deletion_mesh_edit_context_menu_deletes_geometry(cx: &mut TestAppContext) {
    use gpui_kit::{Modifiers, MouseButton, test::ClickOptions};
    use zipzi_editor_core::ModelPoint;
    let h = enter(cx);
    let center = ModelPoint { x: 128., y: 128. };
    pointer_gesture(&h, cx, center, center, Modifiers::default());
    assert_eq!(
        h.view.read_with(cx, |v, _| v.editor.layers()[0].vertices),
        5
    );
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            "canvas-body",
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let i = (0..24usize)
            .find(|&i| {
                window.within("popup-menu").try_find(i).is_some_and(|item| {
                    item.label() == Some("Delete selected mesh points or edges")
                })
            })
            .unwrap();
        window.within("popup-menu").click(i, cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.editor.manual_active());
        assert_eq!(v.editor.layers()[0].vertices, 4);
        assert_eq!(v.editor.document().unwrap().history_lengths(), (2, 0));
    });
}
