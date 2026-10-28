use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{i18n, theme};
use gpui_kit::{component::WindowExt, test::TestWindowExt};
use std::time::Duration;
use zipzi_editor_core::{CommandId, Editor};

pub(crate) fn mesh_generation_screenshots(
    harness: &mut Harness,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let (document, assets) = fixture::alpha_document()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.replace_editor(Editor::from_document(document), cx);
            view.redraw(cx);
        });
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::GenerateMesh, window, cx)
        });
    })?;
    harness._assets = assets;
    for _ in 0..4 {
        harness.cx.run_until_parked();
        harness.cx.advance_clock(Duration::from_millis(150));
    }
    let edges = harness.capture()?;
    let edge_vertices = harness
        .view
        .read_with(&harness.cx, |view, _| view.editor.layers()[0].vertices);
    harness.cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert!(window.find("mesh-generation-panel").visible());
        assert!(window.find("mesh-generation-counts").visible());
        window.click("mesh-generation-stage-interior", cx);
    })?;
    save(&edges, directory, "mesh-generation-edges.png")?;
    for _ in 0..4 {
        harness.cx.run_until_parked();
        harness.cx.advance_clock(Duration::from_millis(150));
    }
    harness.cx.update_window(harness.window, |_, window, cx| {
        for (field, value) in [
            ("mesh-generation-interior-spacing", "12"),
            ("mesh-generation-edge-ratio", "1"),
        ] {
            window.click(field, cx);
            window.press("cmd-a", cx);
            window.input(value, cx);
            window.blur(cx);
        }
    })?;
    for _ in 0..4 {
        harness.cx.run_until_parked();
        harness.cx.advance_clock(Duration::from_millis(150));
    }
    let interior = harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        assert!(window.find("mesh-generation-counts").visible());
        assert!(harness.view.read(cx).editor.layers()[0].vertices > edge_vertices);
    })?;
    save(&interior, directory, "mesh-generation-interior.png")?;
    anyhow::ensure!(
        edges != interior,
        "Interior generation must update viewport presentation"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        i18n::activate(Some("en"));
        theme::change(gpui_kit::component::ThemeMode::Dark, cx);
        window.refresh();
    })?;
    save(&harness.capture()?, directory, "mesh-generation-dark.png")?;
    harness.cx.update_window(harness.window, |_, window, _| {
        i18n::activate(Some("zh-CN"));
        window.refresh();
    })?;
    save(&harness.capture()?, directory, "mesh-generation-zh.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
        i18n::activate(Some("en"));
        window.refresh();
    })?;
    harness.cx.run_until_parked();
    harness.view.read_with(&harness.cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert_eq!(view.editor.layers()[0].vertices, 4);
    });
    Ok(())
}
