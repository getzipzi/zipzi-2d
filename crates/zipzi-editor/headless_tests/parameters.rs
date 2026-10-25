use super::{document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point};

fn show_keyforms(harness: &super::Harness, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.reveal_keyforms(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
}

fn indirect_selection_document(
    bind_child: bool,
) -> (zipzi_editor_core::Document, tempfile::TempDir, String) {
    use zipzi_editor_core::{NewParameter, ObjectKind, ObjectRef, ParameterKeyformAction};
    let (mut document, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let parameter = document
        .create_parameter(NewParameter {
            name: "Ancestor driver".into(),
            runtime_id: "AncestorDriver".into(),
            minimum: -1.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })
        .unwrap();
    for id in if bind_child {
        vec![501, 502]
    } else {
        vec![501]
    } {
        document
            .select_object(ObjectRef::new(
                ObjectKind::Deformer,
                uuid::Uuid::from_u128(id).to_string(),
            ))
            .unwrap();
        document
            .edit_selected_parameter_keyforms(&parameter, ParameterKeyformAction::AddEndpoints)
            .unwrap();
    }
    let index = document.object_index();
    let child = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(502).to_string());
    let mesh = index
        .parameter(&parameter)
        .indirect
        .iter()
        .find(|path| path.target.kind == ObjectKind::Mesh && path.path.contains(&child))
        .unwrap()
        .target
        .clone();
    document.select_object(mesh).unwrap();
    document.focus_parameter(&parameter).unwrap();
    (document, assets, parameter)
}

#[gpui_kit::test]
fn parameter_indirect_button_selects_the_nearest_bound_ancestor(cx: &mut TestAppContext) {
    use zipzi_editor_core::{ObjectKind, ObjectRef};
    let (document, assets, parameter) = indirect_selection_document(true);
    let history = document.history_lengths();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-indirect-{parameter}"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        let nearest = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(502).to_string());
        assert_eq!(document.context().selections, vec![nearest.clone()]);
        assert_eq!(
            document.context().inspected.as_ref(),
            Some(&nearest),
            "the name click must not overwrite object inspection"
        );
        assert_eq!(
            document.context().parameter_focus.as_ref(),
            Some(&ObjectRef::new(ObjectKind::Parameter, &parameter))
        );
        assert_eq!(document.history_lengths(), history);
    });
}

#[gpui_kit::test]
fn parameter_indirect_button_prefers_the_primary_selected_object(cx: &mut TestAppContext) {
    use zipzi_editor_core::{ObjectKind, ObjectRef};
    let (mut document, assets, parameter) = indirect_selection_document(true);
    let secondary = document.context().selection.clone().unwrap();
    let index = document.object_index();
    let primary = index
        .parameter(&parameter)
        .indirect
        .iter()
        .find(|path| path.target.kind == ObjectKind::Mesh && path.target != secondary)
        .unwrap()
        .target
        .clone();
    document.select_objects(vec![secondary, primary]).unwrap();
    document.focus_parameter(&parameter).unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-indirect-{parameter}"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().context().selections,
            vec![ObjectRef::new(
                ObjectKind::Deformer,
                uuid::Uuid::from_u128(501).to_string()
            )]
        );
    });
}

#[gpui_kit::test]
fn parameter_indirect_button_skips_an_unbound_ancestor(cx: &mut TestAppContext) {
    use zipzi_editor_core::{ObjectKind, ObjectRef};
    let (document, assets, parameter) = indirect_selection_document(false);
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-indirect-{parameter}"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().context().selections,
            vec![ObjectRef::new(
                ObjectKind::Deformer,
                uuid::Uuid::from_u128(501).to_string()
            )]
        );
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(format!("parameter-indirect-{parameter}"))
                .is_none()
        );
    })
    .unwrap();
}

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
    use gpui_kit::{ScrollDelta, px};
    use uuid::Uuid;
    use zipzi_core::{Canvas, Parameter, Vec2};
    use zipzi_editor_core::Document;
    use zipzi_sdk::AuthoringSession;

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
        .save_project(&assets.path().join("parameters.zipzi"), None)
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
    show_keyforms(&harness, cx);
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
    show_keyforms(&harness, cx);
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
    show_keyforms(&harness, cx);
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
fn parameter_titlebar_adds_the_current_keyform(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let target = document.context().selection.clone().unwrap();
    document.set_preview_parameter(&parameter, 0.5).unwrap();
    assert!(
        !document.ordinary_binding(&target).unwrap().axes[0]
            .keys
            .contains(&0.5)
    );
    let history = document.history_lengths();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{parameter}"), cx);
        window.render_frame(cx);
        assert!(
            window
                .within("parameters-panel-toolbar")
                .try_find("parameter-add-keyform")
                .is_some()
        );
        assert!(
            window
                .within(format!("parameter-row-{parameter}"))
                .try_find("parameter-add-keyform")
                .is_none()
        );
        window.click("parameter-add-keyform", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        assert!(
            document.ordinary_binding(&target).unwrap().axes[0]
                .keys
                .contains(&0.5)
        );
        assert_eq!(document.history_lengths(), (history.0 + 1, 0));
    });
}

#[gpui_kit::test]
fn parameter_titlebar_deletes_the_current_keyform(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let target = document.context().selection.clone().unwrap();
    let history = document.history_lengths();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{parameter}"), cx);
        window.render_frame(cx);
        window.click("parameter-delete-keyform", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        assert!(
            !document.ordinary_binding(&target).unwrap().axes[0]
                .keys
                .contains(&0.)
        );
        assert_eq!(document.history_lengths(), (history.0 + 1, 0));
    });
}

#[gpui_kit::test]
fn parameter_titlebar_adds_endpoint_keyforms(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].clone();
    let target = document.context().selection.clone().unwrap();
    document
        .edit_selected_parameter_keyforms(
            &parameter.id,
            zipzi_editor_core::ParameterKeyformAction::DeleteAll,
        )
        .unwrap();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{}", parameter.id), cx);
        window.render_frame(cx);
        let standard = window.find("new-parameter").bounds();
        for button in [
            "parameter-add-keyform",
            "parameter-delete-keyform",
            "parameter-add-two-keyforms",
            "parameter-add-three-keyforms",
            "parameter-delete-all-keyforms",
        ] {
            let bounds = window.find(button).bounds();
            assert_eq!(
                bounds.size, standard.size,
                "{button} must use standard icon button sizing"
            );
            assert_eq!(
                bounds.center().y,
                standard.center().y,
                "{button} must align with the toolbar"
            );
        }
        window.click("parameter-add-two-keyforms", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .ordinary_binding(&target)
                .unwrap()
                .axes[0]
                .keys,
            vec![parameter.minimum, parameter.maximum]
        );
    });
}

#[gpui_kit::test]
fn parameter_titlebar_adds_endpoint_and_midpoint_keyforms(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].clone();
    let target = document.context().selection.clone().unwrap();
    document
        .edit_selected_parameter_keyforms(
            &parameter.id,
            zipzi_editor_core::ParameterKeyformAction::DeleteAll,
        )
        .unwrap();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{}", parameter.id), cx);
        window.render_frame(cx);
        window.click("parameter-add-three-keyforms", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let midpoint = parameter.minimum + (parameter.maximum - parameter.minimum) / 2.;
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .ordinary_binding(&target)
                .unwrap()
                .axes[0]
                .keys,
            vec![parameter.minimum, midpoint, parameter.maximum]
        );
    });
}

#[gpui_kit::test]
fn parameter_titlebar_deletes_all_keyforms(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let target = document.context().selection.clone().unwrap();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{parameter}"), cx);
        window.render_frame(cx);
        window.click("parameter-delete-all-keyforms", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .ordinary_binding(&target)
                .unwrap()
                .axes
                .is_empty()
        );
    });
}

#[gpui_kit::test]
fn parameter_titlebar_disabled_keyform_button_shows_reason_tooltip(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.hover("parameter-add-keyform", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn focusing_parameter_keeps_binding_summary_hidden(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters()[0].id.clone();
    let selected = document.context().selection.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{parameter}"), cx);
        window.render_frame(cx);
        assert!(window.try_find("parameter-binding-summary").is_none());
        assert!(window.try_find("parameter-indirect-toggle").is_none());
        assert!(
            window
                .within(format!("parameter-row-{parameter}"))
                .try_find("slider-bar-container")
                .is_some()
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        assert_eq!(document.context().selection, selected);
        assert_eq!(
            document.context().parameter_focus.as_ref().unwrap().id,
            parameter
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
    show_keyforms(&harness, cx);
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
