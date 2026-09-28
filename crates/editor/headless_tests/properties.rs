use super::setup;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point};

#[gpui_kit::test]
fn opacity_and_name_edits_survive_document_switch(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let original_id = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());

    cx.update_window(harness.window, |_, window, cx| {
        let mut properties = window.within("properties-panel");
        let opacity_track = properties.find("slider-bar-container").bounds();
        properties.click_at(
            "slider-bar-container",
            point(
                opacity_track.size.width * 0.55,
                opacity_track.size.height / 2.,
            ),
            cx,
        );
        window.click("property-name", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(
                view.dock
                    .as_ref()
                    .unwrap()
                    .properties
                    .read(cx)
                    .is_focused(window, cx)
            );
        });
        window.press("cmd-a", cx);
        window.input("Renamed", cx);
    })
    .unwrap();

    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            let layer = view.editor.selected_layer().unwrap();
            assert_eq!(layer.name, "Renamed");
            assert!((0.4..0.7).contains(&layer.opacity));
        });
        window.press("cmd-n", cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.documents().len(), 2);
            assert_ne!(view.editor.active_id(), Some(original_id));
        });
        window.click(format!("tab-item-{original_id:?}"), cx);
        harness.view.read_with(cx, |view, _| {
            let layer = view.editor.selected_layer().unwrap();
            assert_eq!(layer.name, "Renamed");
            assert!((0.4..0.7).contains(&layer.opacity));
        });
    })
    .unwrap();
}
