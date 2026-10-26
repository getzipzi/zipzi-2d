use super::super::setup;
use super::common::*;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, PinchEvent, ScrollDelta, ScrollWheelEvent, TestAppContext,
    point, px,
};

#[gpui_kit::test]
fn bound_parameter_slider_stays_quiet_until_a_canvas_edit_is_attempted(cx: &mut TestAppContext) {
    let (document, assets) =
        super::super::document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].id.clone();
    assert_preview_slider_quiet_until_edit(
        cx,
        document,
        assets,
        parameter,
        &["stored keyform", "Pose"],
    );
}

#[gpui_kit::test]
fn blendshape_parameter_slider_stays_quiet_until_a_canvas_edit_is_attempted(
    cx: &mut TestAppContext,
) {
    let (document, assets) = super::super::document_fixture::rectangle_document(true).unwrap();
    let parameter = document
        .parameters()
        .into_iter()
        .find(|p| p.name == "Blend")
        .unwrap()
        .id;
    assert_preview_slider_quiet_until_edit(cx, document, assets, parameter, &["BlendShape"]);
}

#[gpui_kit::test]
fn glue_parameter_slider_and_connected_vertex_edit_stay_available(cx: &mut TestAppContext) {
    let (mut document, assets) = super::super::document_fixture::rectangle_document(true).unwrap();
    document
        .select(&uuid::Uuid::from_u128(909).to_string())
        .unwrap();
    let parameter = document
        .parameters()
        .into_iter()
        .find(|p| p.name == "Pose")
        .unwrap()
        .id;
    assert_preview_slider_quiet_until_edit(cx, document, assets, parameter, &[]);
}

#[gpui_kit::test]
fn blendshape_slider_selects_a_delta_keyform_for_canvas_editing(cx: &mut TestAppContext) {
    let (document, assets) = super::super::document_fixture::rectangle_document(true).unwrap();
    let parameter = document
        .parameters()
        .into_iter()
        .find(|p| p.name == "Blend")
        .unwrap()
        .id;
    assert_preview_slider_quiet_until_edit(cx, document, assets, parameter, &[]);
}

#[gpui_kit::test]
fn blendshape_slider_zero_keyform_allows_base_editing(cx: &mut TestAppContext) {
    let (document, assets) = super::super::document_fixture::rectangle_document(true).unwrap();
    let parameter = document
        .parameters()
        .into_iter()
        .find(|p| p.name == "Blend")
        .unwrap()
        .id;
    assert_preview_slider_quiet_until_edit_at(cx, document, assets, parameter, &[], Some(0.));
}

fn assert_preview_slider_quiet_until_edit(
    cx: &mut TestAppContext,
    document: zipzi_editor_core::Document,
    assets: tempfile::TempDir,
    parameter: String,
    expected_reason: &[&str],
) {
    assert_preview_slider_quiet_until_edit_at(
        cx,
        document,
        assets,
        parameter,
        expected_reason,
        None,
    );
}

fn assert_preview_slider_quiet_until_edit_at(
    cx: &mut TestAppContext,
    document: zipzi_editor_core::Document,
    assets: tempfile::TempDir,
    parameter: String,
    expected_reason: &[&str],
    final_fraction: Option<f32>,
) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
    let is_blend = document
        .parameters()
        .iter()
        .find(|p| p.id == parameter)
        .unwrap()
        .name
        == "Blend";
    let final_fraction = final_fraction.unwrap_or(if is_blend && expected_reason.is_empty() {
        1.
    } else {
        0.65
    });
    let harness = super::super::setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    present_canvas(&harness, cx);
    harness
        .view
        .update(cx, |view, cx| view.set_status_message("Ready", cx));
    let track = cx
        .update_window(harness.window, |_, window, _| {
            window
                .within(format!("parameter-row-{parameter}"))
                .find("slider-bar-container")
                .bounds()
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: point(track.left() + px(1.), track.center().y),
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    for fraction in [0.35, 0.65] {
        cx.update_window(harness.window, |_, window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position: point(track.left() + track.size.width * fraction, track.center().y),
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        cx.run_until_parked();
        present_canvas(&harness, cx);
        harness.view.update(cx, |view, cx| {
            for frame in view.viewport_frames.values_mut() {
                frame.error = None;
            }
            // Frame adoption synchronizes these notices in the real preview worker.
            view.sync_viewport_notice(cx);
            crate::workspace::status_bar::StatusBar::sync(&view.status_bar, view, cx);
            let value = view
                .editor
                .document()
                .unwrap()
                .parameters()
                .into_iter()
                .find(|p| p.id == parameter)
                .unwrap()
                .value;
            assert!(value > 0. && value < 1.);
            assert_eq!(
                view.editor.overlay().unavailable.is_some(),
                !expected_reason.is_empty() || is_blend
            );
            let blocked = !expected_reason.is_empty() || is_blend;
            assert!(
                view.editor
                    .overlay()
                    .vertices
                    .iter()
                    .all(|v| v.editable == !blocked)
            );
            assert!(view.edit_error.is_none());
            assert_eq!(
                view.status_bar.read(cx).displayed_message(),
                Some(("Ready", false))
            );
        });
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position: point(
                    track.left() + track.size.width * final_fraction,
                    track.center().y,
                ),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: point(
                    track.left() + track.size.width * final_fraction,
                    track.center().y,
                ),
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let p = harness
            .view
            .read_with(cx, |view, _| view.editor.overlay().vertices[0].position);
        let start = window.find("canvas-body").bounds().origin + point(px(p.x), px(p.y));
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        harness
            .view
            .read_with(cx, |view, _| assert!(view.edit_error.is_none()));
        window.dispatch_event(
            MouseMoveEvent {
                position: start + point(px(-20.), px(10.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        harness.view.read_with(cx, |view, _| {
            if expected_reason.is_empty() {
                assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
                assert!(view.editor.document().unwrap().has_pending_edit());
                return;
            }
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert!(!view.editor.document().unwrap().has_pending_edit());
            assert!(view.editor.overlay().vertices.iter().all(|v| !v.selected));
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: start + point(px(-20.), px(10.)),
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        if expected_reason.is_empty() {
            harness.view.read_with(cx, |view, _| {
                assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
                assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            });
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn viewport_transform_delta_updates_status_bar_and_clears_on_finish_or_cancel(
    cx: &mut TestAppContext,
) {
    use zipzi_editor_core::{
        CanvasInput, DeformerHandle, EditorCommand, ObjectKind, ObjectRef, ViewportPoint,
    };
    let (document, assets) =
        super::super::document_fixture::sample_document_with_scene_objects().unwrap();
    let harness = super::super::setup_with_document(cx, document, assets);
    let object = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(502).to_string());
    for cancel in [false, true] {
        harness.view.update(cx, |view, _| {
            view.editor
                .execute(EditorCommand::SelectObject(object.clone()))
                .unwrap();
        });
        present_canvas(&harness, cx);
        harness.view.update(cx, |view, cx| {
            // Synthetic frames have no running preview worker.
            for frame in view.viewport_frames.values_mut() {
                frame.error = None;
            }
            view.set_status_message("Ready", cx);
            let frame = view.modeling_frame().unwrap().canvas.clone();
            let control = frame
                .geometry
                .controls
                .iter()
                .find(|control| control.object == object)
                .unwrap();
            let gizmo = frame
                .geometry
                .deformer_gizmo(control, frame.transform)
                .unwrap();
            let start = gizmo
                .handles
                .iter()
                .find(|(handle, _)| *handle == DeformerHandle::Scale(4))
                .unwrap()
                .1;
            let token = view
                .editor
                .begin_canvas_gesture(frame.version.binding, start, false, false)
                .unwrap()
                .capture
                .unwrap();
            let mut previous = None;
            let mut end = start;
            for distance in [20., 40.] {
                end = ViewportPoint {
                    x: start.x + distance,
                    y: start.y + distance,
                };
                view.editor
                    .update_canvas_gesture(
                        token,
                        CanvasInput::Move {
                            point: end,
                            left_down: true,
                        },
                    )
                    .unwrap();
                let delta = view
                    .editor
                    .overlay_for_viewport(token.binding)
                    .interaction_label
                    .unwrap();
                assert!(delta.contains('%'));
                assert_ne!(previous.as_ref(), Some(&delta));
                crate::workspace::status_bar::StatusBar::sync(&view.status_bar, view, cx);
                assert_eq!(
                    view.status_bar.read(cx).displayed_message(),
                    Some((delta.as_str(), false))
                );
                previous = Some(delta);
            }
            view.set_edit_error("Operation failed", cx);
            assert_eq!(
                view.status_bar.read(cx).displayed_message(),
                Some(("Operation failed", true))
            );
            view.clear_edit_error(cx);
            assert_eq!(
                view.status_bar.read(cx).displayed_message(),
                Some((previous.as_deref().unwrap(), false))
            );
            view.editor
                .update_canvas_gesture(
                    token,
                    if cancel {
                        CanvasInput::Cancel
                    } else {
                        CanvasInput::Up { point: end }
                    },
                )
                .unwrap();
            crate::workspace::status_bar::StatusBar::sync(&view.status_bar, view, cx);
            assert_eq!(
                view.status_bar.read(cx).displayed_message(),
                Some(("Ready", false))
            );
        });
    }
}

#[gpui_kit::test]
fn viewport_editing_notice_uses_timed_status_message_without_a_canvas_banner(
    cx: &mut TestAppContext,
) {
    use zipzi_editor_core::{EditorCommand, ObjectKind, ObjectRef};
    let harness = setup(cx);
    present_canvas(&harness, cx);
    let object = harness.view.read_with(cx, |view, _| {
        ObjectRef::new(ObjectKind::Mesh, view.editor.layers()[0].id.clone())
    });
    harness.view.update(cx, |view, cx| {
        // This test presents a synthetic frame without starting the preview worker.
        for frame in view.viewport_frames.values_mut() {
            frame.error = None;
        }
        view.editor
            .execute(EditorCommand::SelectCanvasObjects(vec![object.clone()]))
            .unwrap();
        view.editor
            .execute(EditorCommand::SetObjectsLocked {
                objects: vec![object.clone()],
                locked: true,
            })
            .unwrap();
        view.sync_viewport_notice(cx);
    });
    let reason = harness.view.read_with(cx, |view, cx| {
        let reason = view.editor.overlay().unavailable.unwrap();
        assert_eq!(
            view.status_bar.read(cx).displayed_message(),
            Some((reason.as_str(), false))
        );
        reason
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("canvas-vertex-notice").is_none());
        assert!(window.find("app-status-message").visible());
    })
    .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(crate::EditorView::STATUS_MESSAGE_TIMEOUT);
    cx.run_until_parked();
    harness.view.update(cx, |view, cx| {
        view.sync_viewport_notice(cx);
        assert_eq!(
            view.editor.overlay().unavailable.as_deref(),
            Some(reason.as_str())
        );
        assert!(view.status_bar.read(cx).displayed_message().is_none());
        view.editor
            .execute(EditorCommand::SetObjectsLocked {
                objects: vec![object.clone()],
                locked: false,
            })
            .unwrap();
        view.sync_viewport_notice(cx);
        view.set_edit_error("Operation failed", cx);
        view.editor
            .execute(EditorCommand::SetObjectsLocked {
                objects: vec![object],
                locked: true,
            })
            .unwrap();
        view.sync_viewport_notice(cx);
        assert_eq!(
            view.status_bar.read(cx).displayed_message(),
            Some(("Operation failed", true))
        );
    });
}

#[gpui_kit::test]
fn hover_without_a_presented_frame_keeps_document_observers_idle(cx: &mut TestAppContext) {
    use std::{cell::Cell, rc::Rc};

    let harness = setup(cx);
    assert!(
        harness
            .view
            .read_with(cx, |view, _| view.modeling_frame().is_none())
    );
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _observe =
        cx.update(|cx| cx.observe(&harness.view, move |_, _| observed.set(observed.get() + 1)));
    cx.update_window(harness.window, |_, window, cx| {
        window.hover("canvas-body", cx);
        window.dispatch_event(
            gpui_kit::MouseExitEvent {
                position: window.find("canvas-body").bounds().center(),
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(notifications.get(), 0);
    harness.view.read_with(cx, |view, _| {
        assert!(!view.editor.gesture_active());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn losing_the_presented_frame_rolls_back_a_live_edit_and_notifies_observers(
    cx: &mut TestAppContext,
) {
    use std::{cell::Cell, rc::Rc};

    let harness = setup(cx);
    present_canvas(&harness, cx);
    let original = harness.view.update(cx, |view, _| {
        let original = view.editor.document().unwrap().selected_vertex_positions();
        let start = view.editor.overlay().vertices[1].position;
        let binding = view.editor.primary_viewport_binding().unwrap();
        let token = view
            .editor
            .begin_canvas_gesture(binding, start, false, false)
            .unwrap()
            .capture
            .unwrap();
        assert!(
            view.editor
                .update_canvas_gesture(
                    token,
                    zipzi_editor_core::CanvasInput::Move {
                        point: zipzi_editor_core::ViewportPoint {
                            x: start.x - 24.,
                            y: start.y + 12.,
                        },
                        left_down: true,
                    }
                )
                .unwrap()
                .model_changed
        );
        assert!(view.editor.gesture_active());
        assert_ne!(
            view.editor.document().unwrap().selected_vertex_positions(),
            original
        );
        view.viewport_frames
            .entry(view.viewport_slots[0])
            .or_default()
            .modeling = None;
        original
    });
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _observe =
        cx.update(|cx| cx.observe(&harness.view, move |_, _| observed.set(observed.get() + 1)));
    cx.update_window(harness.window, |_, window, cx| {
        window.hover("canvas-body", cx)
    })
    .unwrap();
    assert!(notifications.get() > 0);
    harness.view.read_with(cx, |view, _| {
        assert!(!view.editor.gesture_active());
        assert_eq!(
            view.editor.document().unwrap().selected_vertex_positions(),
            original
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
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
                view.modeling_frame()
                    .as_ref()
                    .unwrap()
                    .canvas
                    .transform
                    .to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 128. }),
            )
        });
        window.click_at("canvas-body", gpui_kit::point(px(point.x), px(point.y)), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().selected_id(),
                Some(top.as_str())
            );
            assert_eq!(view.editor.active_tool(), zipzi_editor_core::ToolId::Select);
            assert_eq!(view.editor.overlay().vertices.len(), 4);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        window.press("e", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            zipzi_editor_core::ToolId::Select
        );
        window.press("v", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            zipzi_editor_core::ToolId::Select
        );
        window.press("d", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            zipzi_editor_core::ToolId::Select
        );
        window.click("tool_glue", cx);
        assert_eq!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.active_tool()),
            zipzi_editor_core::ToolId::Glue
        );
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            let checked = |id| {
                view.menu_state.commands[zipzi_editor_core::COMMANDS
                    .iter()
                    .position(|spec| spec.id == id)
                    .unwrap()]
                .checked
            };
            assert!(checked(zipzi_editor_core::CommandId::GlueTool));
            assert!(!checked(zipzi_editor_core::CommandId::SelectTool));
        });
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
        let center = harness.view.read_with(cx, |view, _| {
            view.modeling_frame()
                .unwrap()
                .canvas
                .transform
                .to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 128. })
        });
        window.click_with_options(
            "canvas-body",
            gpui_kit::test::ClickOptions::new()
                .with_offset(point(px(center.x), px(center.y)))
                .with_button(gpui_kit::MouseButton::Right),
            cx,
        );
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
                    .active_viewport()
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
    use zipzi_editor_core::{CommandId, DeformerVisibility};
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
            let center = harness.view.read_with(cx, |view, _| {
                view.modeling_frame()
                    .unwrap()
                    .canvas
                    .transform
                    .to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 128. })
            });
            window.click_with_options(
                "canvas-body",
                gpui_kit::test::ClickOptions::new()
                    .with_offset(point(px(center.x), px(center.y)))
                    .with_button(gpui_kit::MouseButton::Right),
                cx,
            );
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
                        .active_viewport()
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
            let before_navigation = harness.view.read_with(cx, |view, _| {
                (
                    view.editor.view(),
                    view.editor.document().unwrap().selected_vertex_positions(),
                )
            });
            for event in [
                ScrollWheelEvent {
                    position: to,
                    delta: ScrollDelta::Pixels(point(px(30.), px(20.))),
                    ..Default::default()
                }
                .to_platform_input(),
                PinchEvent {
                    position: to,
                    delta: 0.2,
                    ..Default::default()
                }
                .to_platform_input(),
            ] {
                window.dispatch_event(event, cx);
                window.render_frame(cx);
            }
            harness.view.read_with(cx, |view, _| {
                assert_eq!(view.editor.view().zoom, before_navigation.0.zoom);
                assert_eq!(view.editor.view().pan, before_navigation.0.pan);
                assert_eq!(
                    view.editor.document().unwrap().selected_vertex_positions(),
                    before_navigation.1
                );
                assert!(view.editor.document().unwrap().has_pending_edit());
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
    use zipzi_core::{Canvas, Transform, TransformData, Vec2, WarpTransform};
    use zipzi_editor_core::{Document, EditorCommand, ObjectKind, ObjectRef, ToolId};
    let assets = tempfile::tempdir().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let mut session = zipzi_sdk::AuthoringSession::new(
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
                    bezier: None,
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
    let harness =
        super::super::setup_with_document(cx, Document::open(&saved.manifest).unwrap(), assets);
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
            assert_eq!(
                view.editor
                    .overlay()
                    .handles
                    .iter()
                    .filter(|h| !h.round)
                    .count(),
                8
            );
            assert_eq!(
                view.editor
                    .overlay()
                    .handles
                    .iter()
                    .filter(|h| h.round)
                    .count(),
                8
            );
        });
        let bar = window.find("deformer-actions").bounds();
        let viewport = window.find("canvas-body").bounds();
        assert!((bar.center().x - viewport.center().x).abs() <= px(0.5));
        let scale = crate::ui::compact::scale(cx);
        assert!((bar.top() - viewport.top()).abs() <= px(0.5));
        for id in [
            "deformer-ancestors",
            "deformer-warp-mode",
            "deformer-grid",
            "deformer-help",
        ] {
            let button = window.find(id).bounds();
            assert!((button.size.height.as_f32() - 20. * scale).abs() <= 0.5);
        }
        window.click("deformer-warp-mode", cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.warp_fine());
            assert!(view.editor.overlay().handles.iter().all(|h| !h.round));
        });
        window.click("deformer-warp-mode", cx);
        window.click("deformer-grid", cx);
        assert!(
            harness
                .view
                .read_with(cx, |view, _| view.editor.deformer_grid())
        );
        let start = harness.view.read_with(cx, |view, _| {
            view.editor
                .overlay()
                .handles
                .iter()
                .find(|h| h.round)
                .unwrap()
                .position
        });
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
            view.modeling_frame()
                .as_ref()
                .unwrap()
                .canvas
                .geometry
                .controls[0]
                .points
                .clone()
        });
        window.hover("canvas-body", cx);
        window.press("h", cx);
        let bounds = window.find("canvas-body").bounds();
        let start = bounds.center();
        window.drag(start, start + point(px(20.), px(0.)), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            assert_eq!(
                view.modeling_frame()
                    .as_ref()
                    .unwrap()
                    .canvas
                    .geometry
                    .controls[0]
                    .points,
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

#[gpui_kit::test]
fn a_cancelled_capture_does_not_swallow_the_next_pointer_down(cx: &mut TestAppContext) {
    let harness = setup(cx);
    present_canvas(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        let start = harness.view.read(cx).editor.overlay().vertices[1].position;
        let position = window.find("canvas-body").bounds().origin + point(px(start.x), px(start.y));
        let press = gpui_kit::MouseDownEvent {
            button: gpui_kit::MouseButton::Left,
            position,
            modifiers: Default::default(),
            click_count: 1,
            first_mouse: false,
        };
        window.dispatch_event(press.clone().to_platform_input(), cx);
        assert!(harness.view.read(cx).editor.gesture_active());
        let canvas = harness
            .view
            .read(cx)
            .modeling_frame()
            .unwrap()
            .image
            .clone();
        harness.view.update(cx, |view, _| {
            let document = view.editor.active_id().unwrap();
            view.editor
                .end_editing(document, zipzi_editor_core::EditingLifecycle::BlockingUi)
                .unwrap();
        });
        assert!(!harness.view.read(cx).editor.gesture_active());
        harness.view.update(cx, |view, _| {
            let frame = view.editor.preview().unwrap().canvas.unwrap();
            assert!(view.editor.present_frame(frame.clone()));
            view.viewport_frames
                .entry(frame.version.binding.viewport)
                .or_default()
                .modeling = Some(crate::preview::Presented {
                image: canvas,
                canvas: frame.clone(),
            });
        });
        window.dispatch_event(press.to_platform_input(), cx);
        assert!(harness.view.read(cx).editor.gesture_active());
        window.dispatch_event(
            gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        assert!(!harness.view.read(cx).editor.gesture_active());
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
fn escape_under_canvas_select_clears_all_selection(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent};
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    present_canvas(&harness, cx);

    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_select", cx);
        assert_eq!(
            harness.view.read(cx).editor.active_tool(),
            zipzi_editor_core::ToolId::Select
        );

        let start = harness
            .view
            .read_with(cx, |view, _| view.editor.overlay().vertices[1].position);
        let from = window.find("canvas-body").bounds().origin + point(px(start.x), px(start.y));
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
            MouseUpEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);

        harness.view.read_with(cx, |view, _| {
            assert!(
                !view
                    .editor
                    .document()
                    .unwrap()
                    .context()
                    .selections
                    .is_empty()
            );
            assert!(
                !view
                    .editor
                    .document()
                    .unwrap()
                    .selected_vertex_positions()
                    .is_empty()
            );
        });

        window.press("escape", cx);
        window.render_frame(cx);

        harness.view.read_with(cx, |view, _| {
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .context()
                    .selections
                    .is_empty()
            );
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .selected_vertex_positions()
                    .is_empty()
            );
            assert!(view.editor.overlay().vertices.is_empty());
        });

        // Re-select by clicking the vertex again.
        window.click("tool_select", cx);
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
            MouseUpEvent {
                button: MouseButton::Left,
                position: from,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);

        harness.view.read_with(cx, |view, _| {
            assert!(
                !view
                    .editor
                    .document()
                    .unwrap()
                    .context()
                    .selections
                    .is_empty()
            );
        });

        // Switch to Hand tool: pressing escape does NOT clear selection.
        window.press("h", cx);
        assert_eq!(
            harness.view.read(cx).editor.active_tool(),
            zipzi_editor_core::ToolId::Hand
        );
        window.press("escape", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(
                !view
                    .editor
                    .document()
                    .unwrap()
                    .context()
                    .selections
                    .is_empty()
            );
        });

        // Switch back to Select tool.
        window.press("v", cx);
        assert_eq!(
            harness.view.read(cx).editor.active_tool(),
            zipzi_editor_core::ToolId::Select
        );

        // Shortcut cmd-d triggers Deselect and clears selection.
        window.press("cmd-d", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .context()
                    .selections
                    .is_empty()
            );
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .selected_vertex_positions()
                    .is_empty()
            );
            assert!(view.editor.overlay().vertices.is_empty());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn glue_restricted_point_explains_its_parameter_on_hover_without_an_error(cx: &mut TestAppContext) {
    use gpui_kit::MouseMoveEvent;
    let (mut document, assets) = super::super::document_fixture::rectangle_document(true).unwrap();
    document
        .set_preview_parameter(&uuid::Uuid::from_u128(904).to_string(), 0.5)
        .unwrap();
    let harness = super::super::setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    present_canvas(&harness, cx);
    harness.view.update(cx, |view, cx| {
        for frame in view.viewport_frames.values_mut() {
            frame.error = None;
        }
        view.set_status_message("Ready", cx);
        assert!(
            view.editor.overlay().glues[0]
                .points
                .iter()
                .any(|p| p.state == zipzi_editor_core::GluePointState::Restricted)
        );
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let p = harness
            .view
            .read_with(cx, |view, _| view.editor.overlay().vertices[0].position);
        let position = window.find("canvas-body").bounds().origin + point(px(p.x), px(p.y));
        window.dispatch_event(
            MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        harness.view.read_with(cx, |view, cx| {
            let overlay = view.editor.overlay();
            let hint = overlay
                .point_unavailable
                .as_deref()
                .expect("restricted Glue hover must explain its cause");
            assert!(hint.contains("Pose") && hint.contains("0.5"), "{hint}");
            assert!(view.edit_error.is_none());
            assert_eq!(
                view.status_bar.read(cx).displayed_message(),
                Some((hint, false))
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
    })
    .unwrap();
    harness.view.update(cx, |view, cx| {
        let binding = view.modeling_frame().unwrap().canvas.version.binding;
        view.editor.hover_canvas(binding, None).unwrap();
        crate::workspace::status_bar::StatusBar::sync(&view.status_bar, view, cx);
        assert_eq!(
            view.status_bar.read(cx).displayed_message(),
            Some(("Ready", false))
        );
    });
}
