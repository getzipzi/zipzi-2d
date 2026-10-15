//! Editor UI integration tests using GPUI's headless window.

mod animation;
mod animation_panels;
mod canvas;
mod compact_popups;
mod compact_ui;
mod docking_tabs;
mod hierarchy;
mod mesh_generation;
mod panel_toolbar;
mod parameter_creation;
mod parameter_groups;
mod parameter_management;
mod parameters;
mod preview_viewport;
mod project_import;
mod properties;
mod recent_files;
mod slide_number_input;
mod sorting;
mod timeline;
mod timeline_navigation;
mod timeline_regressions;
mod titlebar;
mod ui_performance;
mod workspaces;

use crate::{EditorView, actions, i18n, settings};
use editor_core::{ConfigStore, Document, Editor};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions,
    point, px, size,
};

#[path = "../../editor-core/tests/support/mod.rs"]
#[allow(dead_code)]
pub(crate) mod document_fixture;

struct Harness {
    window: AnyWindowHandle,
    view: Entity<EditorView>,
    _config_dir: tempfile::TempDir,
    _assets: tempfile::TempDir,
}

fn setup(cx: &mut TestAppContext) -> Harness {
    let (document, assets) = document_fixture::sample_document_with_two_layers().unwrap();
    setup_with_document(cx, document, assets)
}

fn setup_with_document(
    cx: &mut TestAppContext,
    document: Document,
    assets: tempfile::TempDir,
) -> Harness {
    i18n::activate(Some("en"));
    let config_dir = tempfile::tempdir().unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(settings::Settings::from_store(
            ConfigStore::new(config_dir.path()),
            cx.keyboard_mapper().as_ref(),
        ));
        actions::init(cx);
        crate::theme::init(cx);
    });

    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1100.), px(740.)),
                ))),
                ..crate::workspace::title_bar::window_options()
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
