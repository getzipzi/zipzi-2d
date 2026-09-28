//! Real editor widgets exercised in GPUI's headless window.
use crate::{EditorView, i18n, menus, settings};
use editor_core::{ConfigStore, Editor};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, Bounds, TestAppContext, WindowBounds, WindowOptions, point, px, size,
};

#[path = "../../editor-core/tests/support/mod.rs"]
#[allow(dead_code)]
mod support;

#[gpui_kit::test]
fn layer_and_property_events_survive_document_switch(cx: &mut TestAppContext) {
    i18n::activate(Some("en"));
    let config_dir = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(settings::Settings::from_store(
            ConfigStore::new(config_dir.path()),
            cx.keyboard_mapper().as_ref(),
        ));
        menus::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
    });

    let (document, _assets) = support::sample_document_with_two_layers().unwrap();
    let (window_handle, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1100.), px(740.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| EditorView::new(window, cx)),
        )
        .unwrap()
    });

    let (original_id, selected_id, second_id) = cx
        .update_window(window_handle.into(), |_, window, cx| {
            view.update(cx, |view, cx| {
                view.editor = Editor::from_document(document);
                cx.notify();
            });
            window.render_frame(cx);
            view.read_with(cx, |view, _| {
                let layers = view.editor.layers();
                (
                    view.editor.active_id().unwrap(),
                    view.editor.selected_layer().unwrap().id,
                    layers
                        .into_iter()
                        .find(|layer| layer.name == "Second")
                        .unwrap()
                        .id,
                )
            })
        })
        .unwrap();

    cx.update_window(window_handle.into(), |_, window, cx| {
        window.click(format!("layer-vis-{second_id}"), cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer().unwrap().id, selected_id);
            assert!(
                !view
                    .editor
                    .layers()
                    .into_iter()
                    .find(|layer| layer.id == second_id)
                    .unwrap()
                    .visible
            );
        });

        let opacity_track = window.find("slider-bar-container").bounds();
        window.click_at(
            "slider-bar-container",
            point(
                opacity_track.size.width * 0.55,
                opacity_track.size.height / 2.,
            ),
            cx,
        );

        window.click("property-name", cx);
        view.read_with(cx, |view, cx| {
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

    cx.update_window(window_handle.into(), |_, window, cx| {
        window.render_frame(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer().unwrap().name, "Renamed");
            assert!((0.4..0.7).contains(&view.editor.selected_layer().unwrap().opacity));
        });
        window.click("tab-new-button", cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor.documents().len(), 2);
            assert_ne!(view.editor.active_id(), Some(original_id));
        });
        window.click(format!("tab-item-{original_id:?}"), cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer().unwrap().name, "Renamed");
            assert_eq!(view.editor.selected_layer().unwrap().id, selected_id);
        });
        window.click("tool_hand", cx);
        let canvas = window.find("canvas-body").bounds();
        let from = canvas.center();
        let to = point(from.x + px(40.), from.y + px(20.));
        let before = view.read_with(cx, |view, _| view.editor.view().pan);
        window.drag(from, to, cx);
        let after = view.read_with(cx, |view, _| view.editor.view().pan);
        assert_eq!(after.x - before.x, 40.);
        assert_eq!(after.y - before.y, 20.);
        window.click("tool_select", cx);
        view.read_with(cx, |view, cx| {
            assert!(!view.dock.as_ref().unwrap().canvas.read(cx).hand_tool());
        });
    })
    .unwrap();
}
