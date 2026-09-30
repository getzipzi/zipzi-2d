use super::{document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point};

#[gpui_kit::test]
fn parameter_slider_and_reset_update_the_live_preview(cx: &mut TestAppContext) {
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
            Some(revision + 1),
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
        assert_eq!(view.editor.tree_revision(), Some(revision + 1));
    });

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-reset-{id}"), cx);
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
        assert!(window.try_find(format!("parameter-reset-{last}")).is_none());
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
        window.click(format!("parameter-reset-{last}"), cx);
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
fn linked_parameters_move_together_and_can_be_unlinked(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(2).unwrap();
    let first_id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-link-{first_id}"), cx);
        let pad = window.find("linked-parameter-pad").bounds();
        window.click_at(
            "linked-parameter-pad",
            point(pad.size.width * 0.8, pad.size.height * 0.2),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let parameters = view.editor.document().unwrap().parameters();
        assert!(parameters[0].value > 0.6, "x = {}", parameters[0].value);
        assert!(parameters[1].value > 0.6, "y = {}", parameters[1].value);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-link-{first_id}"), cx);
        assert!(window.try_find("linked-parameter-pad").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn parameter_value_input_commits_exact_values_and_rejects_out_of_range(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{id}"), cx);
        window.press("cmd-a", cx);
        window.input("0.375", cx);
        window.click(format!("parameter-apply-{id}"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.375);
    });

    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{id}"), cx);
        window.press("cmd-a", cx);
        window.input("1.25", cx);
        window.click(format!("parameter-apply-{id}"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.375);
    });
}

#[gpui_kit::test]
fn key_click_updates_model_and_reset_keeps_input_synchronized(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, input::AnyInputState};
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-key-{id}-1"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 1.);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-reset-{id}"), cx);
        window.click(format!("parameter-value-{id}"), cx);
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("numeric input not focused")
        };
        assert_eq!(input.read(cx).value().as_ref(), "0");
        window.click(format!("parameter-apply-{id}"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.);
    });
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
        window.click(format!("parameter-apply-{id}"), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.);
    });
}

#[gpui_kit::test]
fn parameter_link_survives_document_switch_and_does_not_change_other_document(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::sample_document_with_parameters(2).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-link-{id}"), cx);
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("linked-parameter-pad").is_none());
        harness.view.update(cx, |view, cx| {
            view.editor.activate(original).unwrap();
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("linked-parameter-pad").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn linked_key_click_preserves_authored_precision_and_evaluates_once(cx: &mut TestAppContext) {
    let (document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    let revision = harness
        .view
        .read_with(cx, |view, _| view.editor.tree_revision().unwrap());
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-link-{id}"), cx);
        window.click("linked-key-0.375-0.375", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .parameters()
                .iter()
                .all(|p| p.value == 0.375)
        );
        assert_eq!(view.editor.tree_revision(), Some(revision + 1));
        assert!(!view.editor.document().unwrap().modified());
    });
    cx.update_window(harness.window, |_, window, cx| {
        // Displayed precision is 0.38; focusing and leaving must not rewrite the key value.
        window.click(format!("parameter-value-{id}"), cx);
        window.press("enter", cx);
        window.click("linked-key-0.375-0.375", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .parameters()
                .iter()
                .all(|p| p.value == 0.375)
        );
        assert_eq!(view.editor.tree_revision(), Some(revision + 1));
    });
}

#[gpui_kit::test]
fn parameter_enter_commits_without_apply_button(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-value-{id}"), cx);
        window.press("cmd-a", cx);
        window.input("0.625", cx);
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].value, 0.625)
    });
}

#[gpui_kit::test]
fn modal_blocks_linked_parameter_changes(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(2).unwrap();
    let id = document.parameters()[0].id.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-link-{id}"), cx);
        harness.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let pad = window.find("linked-parameter-pad").bounds();
        window.click_at(
            "linked-parameter-pad",
            point(pad.size.width * 0.75, pad.size.height * 0.25),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .parameters()
                .iter()
                .all(|p| p.value == 0.)
        )
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
