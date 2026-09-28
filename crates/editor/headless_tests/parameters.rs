use super::{document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point};

#[gpui_kit::test]
fn parameter_slider_and_reset_update_the_live_preview(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = document.parameters().remove(0);
    let id = parameter.id.clone();
    let harness = setup_with_document(cx, document, assets);

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
        let value = view.editor.document().unwrap().parameters()[0].value;
        assert!((0.4..0.7).contains(&value), "parameter value = {value}");
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
