use super::{document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point};

#[gpui_kit::test]
fn parameter_drag_tracks_outside_the_row_without_a_window_drag(cx: &mut TestAppContext) {
    use gpui_kit::{
        InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, px,
    };

    let (document, assets) = document_fixture::sample_document_with_parameters(2).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let track = cx
        .update_window(harness.window, |_, window, _| {
            window
                .within(format!("parameter-row-{id}"))
                .find("slider-bar-container")
                .bounds()
        })
        .unwrap();
    let revision = harness
        .view
        .read_with(cx, |view, _| view.editor.tree_revision());
    cx.update_window(harness.window, |_, window, cx| {
        // Press the thumb at its default value. Pressing alone must not jump it.
        let position = point(track.left() + px(1.), track.center().y);
        window.dispatch_event(
            MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        assert!(!cx.has_active_drag());
    })
    .unwrap();
    assert_eq!(
        harness
            .view
            .read_with(cx, |view, _| view.editor.tree_revision()),
        revision
    );

    // Move beyond both the row and the track; the value must clamp to max.
    let outside = point(track.right() + px(40.), track.bottom() + px(100.));
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position: outside,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert!(!cx.has_active_drag());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let parameters = view.editor.document().unwrap().parameters();
        assert_eq!(parameters[0].value, 1.);
        assert_eq!(parameters[1].value, 0.);
    });
    let revision = harness
        .view
        .read_with(cx, |view, _| view.editor.tree_revision());
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: outside,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    assert_eq!(
        harness
            .view
            .read_with(cx, |view, _| view.editor.tree_revision()),
        revision
    );

    // A move with a pressed button cannot resume a released slider.
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position: point(track.left() - px(20.), track.center().y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    assert_eq!(
        harness
            .view
            .read_with(cx, |view, _| view.editor.tree_revision()),
        revision
    );
}

#[gpui_kit::test]
fn parameter_drag_is_cancelled_when_the_window_loses_focus(cx: &mut TestAppContext) {
    use gpui_kit::{
        InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, VisualTestContext, px,
    };
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let mut visual = VisualTestContext::from_window(harness.window, cx);
    let track = visual.update(|window, cx| {
        window.activate_window();
        let track = window
            .within(format!("parameter-row-{id}"))
            .find("slider-bar-container")
            .bounds();
        let position = point(track.left() + track.size.width * 0.55, track.center().y);
        window.dispatch_event(
            MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        track
    });
    let revision = harness
        .view
        .read_with(cx, |view, _| view.editor.tree_revision());
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters()[0].value > 0.4);
    });
    visual.deactivate_window();
    visual.update(|window, cx| {
        window.activate_window();
        window.dispatch_event(
            MouseMoveEvent {
                position: point(track.left() - px(20.), track.center().y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
    });
    assert_eq!(
        harness
            .view
            .read_with(cx, |view, _| view.editor.tree_revision()),
        revision
    );
}

#[gpui_kit::test]
fn parameter_slider_and_input_update_the_live_preview(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters().remove(0);
    let id = parameter.id.clone();
    let harness = setup_with_document(cx, document, assets);

    let revision = harness
        .view
        .read_with(cx, |view, _| view.editor.tree_revision().unwrap());

    cx.update_window(harness.window, |_, window, cx| {
        let mut row = window.within(format!("parameter-row-{id}"));
        let track = row.find("slider-bar-container").bounds();
        row.click_at(
            "slider-bar-container",
            point(track.size.width * 0.55, track.size.height / 2.),
            cx,
        );
    })
    .unwrap();

    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.tree_revision(),
            Some(revision),
            "mouse-up must not evaluate the same parameter value again"
        );
        let value = view.editor.document().unwrap().parameters()[0].value;
        assert!((0.4..0.7).contains(&value), "parameter value = {value}");
    });

    // Clicking the same quantized slider step is also a no-op for the model.
    cx.update_window(harness.window, |_, window, cx| {
        let mut row = window.within(format!("parameter-row-{id}"));
        let track = row.find("slider-bar-container").bounds();
        row.click_at(
            "slider-bar-container",
            point(track.size.width * 0.55, track.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.tree_revision(), Some(revision));
    });

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{id}"), cx);
        window.press("cmd-a", cx);
        window.input("0", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();

    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().parameters()[0].value,
            parameter.default_value
        );
    });
}

#[gpui_kit::test]
fn parameter_slider_remains_editable_after_scrolling_cached_rows(cx: &mut TestAppContext) {
    use editor_core::Document;
    use gpui_kit::{ScrollDelta, px};
    use kasane_core::{Canvas, Parameter, Vec2};
    use kasane_sdk::AuthoringSession;
    use uuid::Uuid;

    let assets = tempfile::tempdir().unwrap();
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )
    .unwrap();
    let ids: Vec<_> = (0..128).map(|_| Uuid::new_v4().to_string()).collect();
    session
        .edit("Parameters", None, |edit| {
            for (index, id) in ids.iter().enumerate() {
                edit.create_parameter(Parameter {
                    id: id.clone(),
                    name: format!("Pose {index}"),
                    minimum: 0.,
                    maximum: 1.,
                    default_value: 0.,
                    ..Default::default()
                })?;
            }
            Ok(())
        })
        .unwrap();
    let receipt = session
        .save_project(&assets.path().join("parameters.kasane"), None)
        .unwrap();
    let harness = setup_with_document(cx, Document::open(&receipt.manifest).unwrap(), assets);
    let last = ids.last().unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(window.try_find(format!("parameter-value-{last}")).is_none());
        window.scroll(
            "parameters-panel",
            ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            cx,
        );
        let mut row = window.within(format!("parameter-row-{last}"));
        let track = row.find("slider-bar-container").bounds();
        row.click_at(
            "slider-bar-container",
            point(track.size.width * 0.75, track.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let parameters = view.editor.document().unwrap().parameters();
        assert!(parameters.last().unwrap().value > 0.5);
        assert!(parameters[..127].iter().all(|p| p.value == 0.));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{last}"), cx);
        window.press("cmd-a", cx);
        window.input("0", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.scroll(
            "parameters-panel",
            ScrollDelta::Pixels(point(px(0.), px(10000.))),
            cx,
        );
        let mut row = window.within(format!("parameter-row-{}", ids[0]));
        let track = row.find("slider-bar-container").bounds();
        row.click_at(
            "slider-bar-container",
            point(track.size.width * 0.55, track.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let parameters = view.editor.document().unwrap().parameters();
        assert!(parameters[0].value > 0.4);
        assert!(parameters[1..].iter().all(|p| p.value == 0.));
    });
}

#[gpui_kit::test]
fn unrelated_parameters_never_create_a_shared_keyform_grid(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(2).unwrap();
    let first_id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{first_id}"), cx);
        assert!(
            window
                .try_find(format!("parameter-link-{first_id}"))
                .is_none()
        );
        assert!(window.try_find("binding-keyform-pad").is_none());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let d = view.editor.document().unwrap();
        assert_eq!(d.context().parameter_focus.as_ref().unwrap().id, first_id);
        assert!(d.context().active_binding.is_none());
        assert!(d.parameters().iter().all(|p| p.value == 0.));
    });
}

#[gpui_kit::test]
fn parameter_blur_applies_exact_values_and_invalid_drafts_restore_display(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, input::AnyInputState};
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        window.click(format!("parameter-value-{id}"), cx);
        window.press("cmd-a", cx);
        window.input("0.375", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.blur(cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.375);
    });

    // An unedited, truncated value must not overwrite the exact preview on blur.
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{id}"), cx);
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("numeric input not focused")
        };
        assert_eq!(input.read(cx).value().as_ref(), "0.3");
        window.blur(cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.375);
    });

    for (index, draft) in ["1.25", "-0.25", "NaN", "inf", "-inf", "bad", ""]
        .into_iter()
        .enumerate()
    {
        cx.update_window(harness.window, |_, window, cx| {
            window.click(format!("parameter-value-{id}"), cx);
            window.press("cmd-a", cx);
            window.press("backspace", cx);
            if !draft.is_empty() {
                window.input(draft, cx);
            }
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            if index % 2 == 0 {
                window.press("enter", cx);
            } else {
                window.blur(cx);
                window.render_frame(cx);
            }
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.click(format!("parameter-value-{id}"), cx);
            let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
                panic!("numeric input not focused")
            };
            assert_eq!(input.read(cx).value().as_ref(), "0.3", "draft = {draft}");
        })
        .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.375);
            assert!(!view.editor.document().unwrap().modified());
        });
    }
}

#[gpui_kit::test]
fn key_click_keeps_numeric_input_synchronized(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, input::AnyInputState};
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    for (key, expected) in [(1, "1.0"), (0, "0.0")] {
        cx.update_window(harness.window, |_, window, cx| {
            window.click(format!("parameter-key-{id}-{key}"), cx);
            window.click(format!("parameter-value-{id}"), cx);
            let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
                panic!("numeric input not focused")
            };
            assert_eq!(input.read(cx).value().as_ref(), expected);
            window.press("enter", cx);
        })
        .unwrap();
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().parameters()[0].value,
                key as f32
            );
        });
    }
}

#[gpui_kit::test]
fn parameter_escape_discards_draft_before_blur(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{id}"), cx);
        window.press("cmd-a", cx);
        window.input("0.75", cx);
        window.press("escape", cx);
        window.blur(cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.);
    });
}

#[gpui_kit::test]
fn active_binding_survives_document_switch_without_changing_other_document(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let binding = document.object_index().bindings[0].reference.clone();
    document.open_binding(binding.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    let original = harness
        .view
        .read_with(cx, |v, _| v.editor.active_id().unwrap());
    cx.update_window(harness.window, |_, window, cx| {
        assert!(window.try_find("binding-keyform-pad").is_some());
        harness.view.update(cx, |v, cx| {
            v.editor.new_document().unwrap();
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("binding-keyform-pad").is_none());
        harness.view.update(cx, |v, cx| {
            assert!(
                v.editor
                    .document()
                    .unwrap()
                    .context()
                    .active_binding
                    .is_none()
            );
            v.editor.activate(original).unwrap();
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("binding-keyform-pad").is_some());
    })
    .unwrap();
    harness.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.document().unwrap().context().active_binding,
            Some(binding)
        )
    });
}

#[gpui_kit::test]
fn binding_key_click_preserves_authored_precision_and_evaluates_once(cx: &mut TestAppContext) {
    let (mut document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let binding = document.object_index().bindings[0].reference.clone();
    document.open_binding(binding).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let revision = harness.view.read_with(cx, |v, _| v.editor.tree_revision());
    cx.update_window(harness.window, |_, window, cx| {
        window.click("keyform-4", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .parameters()
                .iter()
                .all(|p| p.value == 0.375)
        );
        assert_eq!(v.editor.tree_revision(), revision);
        assert!(!v.editor.document().unwrap().modified());
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{id}"), cx);
        window.press("enter", cx);
        window.click("keyform-4", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .parameters()
                .iter()
                .all(|p| p.value == 0.375)
        );
        assert_eq!(v.editor.tree_revision(), revision);
    });
}

#[gpui_kit::test]
fn parameter_enter_applies_drafts_and_preserves_unedited_precision(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, input::AnyInputState};
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(window.try_find(format!("parameter-reset-{id}")).is_none());
        assert!(window.try_find(format!("parameter-apply-{id}")).is_none());
        window.click(format!("parameter-value-{id}"), cx);
        window.press("cmd-a", cx);
        window.input("0.625", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("numeric input not focused")
        };
        assert_eq!(input.read(cx).value().as_ref(), "0.6");
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.625)
    });
    // Explicitly typing the same text as the truncated display still applies it.
    cx.update_window(harness.window, |_, window, cx| {
        window.press("cmd-a", cx);
        window.input("0.6", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.6)
    });
}

#[gpui_kit::test]
fn parameter_number_input_steps_then_commits_on_enter_or_blur(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        window.click(format!("parameter-value-{id}"), cx);
        let input = window.within(format!("parameter-value-{id}"));
        for button in ["increment", "decrement"] {
            assert!(
                input
                    .try_find(button)
                    .is_none_or(|element| element.bounds().size.width == gpui_kit::px(0.))
            );
        }
        window.press("up", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.1);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.blur(cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.);
        assert!(!view.editor.document().unwrap().modified());
    });
}

#[gpui_kit::test]
fn modal_blocks_binding_keyform_changes(cx: &mut TestAppContext) {
    let (mut document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let binding = document.object_index().bindings[0].reference.clone();
    document.open_binding(binding).unwrap();
    let before = document.parameters();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |v, cx| {
            v.modal = true;
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("keyform-4", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().parameters(), before)
    });
}

#[gpui_kit::test]
fn single_key_click_preserves_authored_precision(cx: &mut TestAppContext) {
    let (document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-key-{id}-0.375"), cx);
        window.click(format!("parameter-key-{id}-0.375"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.375)
    });
}

#[gpui_kit::test]
fn parameter_binding_navigation_reveals_hidden_keyforms_without_losing_selection(
    cx: &mut TestAppContext,
) {
    use crate::docking::PanelKind;
    use crate::panels::DocumentPanelKind;
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let binding = document.object_index().bindings[0].clone();
    let selected = document.context().selection.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.dock.as_ref().unwrap().toggle(
                PanelKind::Document(DocumentPanelKind::Keyforms),
                window,
                cx,
            )
        });
        window.render_frame(cx);
        assert!(window.try_find("binding-keyform-pad").is_none());
        window.click(format!("parameter-name-{parameter}"), cx);
        harness.view.read_with(cx, |v, _| {
            assert_eq!(v.editor.document().unwrap().context().selection, selected)
        });
        window.click(
            format!(
                "parameter-binding-{:?}-{}",
                binding.reference.kind, binding.reference.id
            ),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |v, cx| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.context().active_binding, Some(binding.reference));
        assert_eq!(d.context().selection, selected);
        let dock = v.dock.as_ref().unwrap();
        assert!(
            dock.area
                .read(cx)
                .panel(gpui_kit::component::dock::PanelId::from(
                    dock.document_panels[&DocumentPanelKind::Keyforms].entity_id()
                ))
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn parameter_slider_refreshes_the_active_keyform_coordinate(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let binding = document.object_index().bindings[0].reference.clone();
    document.open_binding(binding).unwrap();
    let harness = setup_with_document(cx, document, assets);
    let before = cx
        .update_window(harness.window, |_, window, _| {
            window.find("keyform-preview-coordinate").bounds().center()
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let mut row = window.within(format!("parameter-row-{parameter}"));
        let track = row.find("slider-bar-container").bounds();
        row.click_at(
            "slider-bar-container",
            point(track.size.width * 0.7, track.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let after = window.find("keyform-preview-coordinate").bounds().center();
        assert!(after.x > before.x, "before {before:?}, after {after:?}");
        assert_eq!(after.y, before.y);
        harness.view.read_with(cx, |view, _| {
            assert!(!view.editor.document().unwrap().modified());
        });
    })
    .unwrap();
}
