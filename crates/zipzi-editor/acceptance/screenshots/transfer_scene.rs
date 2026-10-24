use super::{super::harness::Harness, common::save};
use gpui_kit::test::TestWindowExt;
use std::time::Duration;
pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN"] {
        let mut h = Harness::new()?;
        crate::i18n::activate(Some(locale));
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.run_command(zipzi_editor_core::CommandId::ExportCubism, window, cx)
            });
        })?;
        h.cx.run_until_parked();
        h.cx.advance_clock(Duration::from_millis(150));
        h.cx.run_until_parked();
        h.cx.update_window(h.window, |_, window, cx| window.render_frame(cx))?;
        let image = h.cx.capture_screenshot(h.window)?;
        save(
            &h.dialog_image(&image)?,
            directory,
            &format!("export-auto-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("export-manual", cx);
        })?;
        h.cx.update_window(h.window, |_, window, cx| window.render_frame(cx))?;
        let image = h.cx.capture_screenshot(h.window)?;
        save(
            &h.dialog_image(&image)?,
            directory,
            &format!("export-manual-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("export-cancel", cx);
        })?;
    }
    Ok(())
}
