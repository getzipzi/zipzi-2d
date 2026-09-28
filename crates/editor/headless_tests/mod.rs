//! Editor UI integration tests using GPUI's headless window.

mod canvas;
mod layers;
mod properties;
mod recent_files;

use crate::{EditorView, i18n, menus, settings};
use editor_core::{ConfigStore, Editor};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions,
    point, px, size,
};

#[path = "../../editor-core/tests/support/mod.rs"]
#[allow(dead_code)]
mod document_fixture;

struct Harness {
    window: AnyWindowHandle,
    view: Entity<EditorView>,
    _config_dir: tempfile::TempDir,
    _assets: tempfile::TempDir,
}

fn setup(cx: &mut TestAppContext) -> Harness {
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

    let (document, assets) = document_fixture::sample_document_with_two_layers().unwrap();
    let (window, view) = cx.update(|cx| {
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
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            cx.notify();
        });
        gpui_kit::test::TestWindowExt::render_frame(window, cx);
    })
    .unwrap();
    Harness {
        window,
        view,
        _config_dir: config_dir,
        _assets: assets,
    }
}
