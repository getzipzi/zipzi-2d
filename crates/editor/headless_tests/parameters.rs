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
