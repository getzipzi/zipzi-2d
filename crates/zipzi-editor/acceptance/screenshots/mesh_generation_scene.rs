use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{i18n, theme};
use gpui_kit::{point, px, test::TestWindowExt};
use std::time::Duration;
use zipzi_editor_core::Editor;

pub(crate) fn mesh_generation_screenshots(
    harness: &mut Harness,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let (document, assets) = fixture::rectangle_document(false)?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.click("tool_generate_mesh", cx);
    })?;
    harness._assets = assets;
    harness.cx.run_until_parked();
    harness.cx.advance_clock(Duration::from_millis(150));
    harness.cx.run_until_parked();
    let mesh_preview = harness.capture()?;
    save(
        &harness.dialog_image(&mesh_preview)?,
        directory,
        "mesh-generation-zh-light.png",
    )?;
    let wire = harness.element_image(&mesh_preview, "mesh-generation-preview")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-wireframe", cx);
    })?;
    let artwork = harness.capture()?;
    anyhow::ensure!(
        wire != harness.element_image(&artwork, "mesh-generation-preview")?,
        "Wireframe toggle must change preview pixels"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
        i18n::activate(Some("en"));
        theme::change(gpui_kit::component::ThemeMode::Dark, cx);
        window.click("tool_generate_mesh", cx);
    })?;
    harness.cx.run_until_parked();
    harness.cx.advance_clock(Duration::from_millis(150));
    harness.cx.run_until_parked();
    let mesh_preview = harness.capture()?;
    save(
        &harness.dialog_image(&mesh_preview)?,
        directory,
        "mesh-generation-dark.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-fit", cx);
    })?;
    let fit_preview = harness.capture()?;
    let fitted = harness.element_image(&fit_preview, "preview-viewport")?;
    anyhow::ensure!(fitted.width() == fitted.height(), "Preview must be square");
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-zoom-in", cx);
    })?;
    let zoomed = harness.capture()?;
    let zoomed_view = harness.element_image(&zoomed, "preview-viewport")?;
    anyhow::ensure!(fitted != zoomed_view, "Zoom must change preview pixels");
    harness.cx.update_window(harness.window, |_, window, cx| {
        let center = window.find("preview-viewport").bounds().center();
        window.drag(center, center + point(px(50.), px(30.)), cx);
    })?;
    let panned = harness.capture()?;
    anyhow::ensure!(
        zoomed_view != harness.element_image(&panned, "preview-viewport")?,
        "Dragging must pan the image"
    );
    save(
        &harness.dialog_image(&panned)?,
        directory,
        "mesh-generation-panned.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-fit", cx);
    })?;
    let reset = harness.capture()?;
    anyhow::ensure!(
        fitted == harness.element_image(&reset, "preview-viewport")?,
        "Fit must restore both zoom and pan"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })?;
    let (document, assets) = fixture::alpha_document()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        i18n::activate(Some("zh-CN"));
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.click("tool_generate_mesh", cx);
    })?;
    harness._assets = assets;
    harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-alpha-heavy", cx);
    })?;
    harness.cx.run_until_parked();
    harness.cx.advance_clock(Duration::from_millis(150));
    harness.cx.run_until_parked();
    let alpha = harness.capture()?;
    save(
        &harness.dialog_image(&alpha)?,
        directory,
        "mesh-generation-alpha.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        assert!(window.find("mesh-generation-counts").visible());
        window.click("preview-wireframe", cx);
    })?;
    let artwork = harness.capture()?;
    anyhow::ensure!(
        harness.element_image(&alpha, "mesh-generation-preview")?
            != harness.element_image(&artwork, "mesh-generation-preview")?,
        "Alpha mesh wireframe must change rendered pixels"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })?;
    Ok(())
}
