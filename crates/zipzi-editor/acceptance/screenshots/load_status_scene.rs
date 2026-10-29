//! Real status bar pixels with concurrent load progress and a narrow translated view.
use super::{Harness, common::save};
use crate::{
    i18n,
    theme::{self, ColorTheme},
};
use gpui_kit::test::TestWindowExt;
use std::{path::PathBuf, time::Duration};
use zipzi_editor_core::LoadRequest;

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for (locale, width, color) in [
        ("en", 1100., ColorTheme::Light),
        ("zh-CN", 760., ColorTheme::Dark),
    ] {
        let mut h = Harness::with_size(width, 740.)?;
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../zipzi-psd/tests/fixtures/layered.psd");
        h.capture()?;
        let (first, second, task, queued) = h.view.update(&mut h.cx, |view, cx| {
            i18n::activate(Some(locale));
            theme::set_preference(color, cx)?;
            let task = view
                .editor
                .prepare_load(LoadRequest::Psd(source.clone()))?
                .unwrap();
            let other_path = source.with_file_name("Character-with-a-long-project-name.zipzi");
            let queued = view
                .editor
                .prepare_load(LoadRequest::Open(other_path.clone()))?
                .unwrap();
            let first = view.begin_load_status(source, true, task.progress(), cx);
            let second = view.begin_load_status(other_path, false, queued.progress(), cx);
            view.redraw(cx);
            Ok::<_, anyhow::Error>((first, second, task, queued))
        })?;
        let completion = task.run();
        h.cx.run_until_parked();
        h.cx.advance_clock(Duration::from_millis(250));
        h.cx.run_until_parked();
        let frame = h.capture()?;
        save(&frame, directory, &format!("load-status-{locale}.png"))?;
        let crop = h.element_image(&frame, "app-status-bar")?;
        save(
            &crop,
            directory,
            &format!("load-status-detail-{locale}.png"),
        )?;
        h.view.update(&mut h.cx, |view, cx| {
            view.set_status_message("A background action completed", cx);
        });
        let frame = h.capture()?;
        save(
            &frame,
            directory,
            &format!("load-status-message-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, _| {
            anyhow::ensure!(window.try_find("app-status-tasks").is_some());
            anyhow::ensure!(window.try_find("app-status-message").is_none());
            Ok::<_, anyhow::Error>(())
        })??;
        h.cx.advance_clock(crate::EditorView::STATUS_MESSAGE_TIMEOUT);
        h.cx.run_until_parked();
        let frame = h.capture()?;
        save(
            &frame,
            directory,
            &format!("load-status-restored-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            let bar = window.find("app-status-bar").bounds();
            let load = window.find("app-status-tasks").bounds();
            let center = window.find("app-status-center").bounds();
            anyhow::ensure!((center.center().x - bar.center().x).abs() <= gpui_kit::px(0.5));
            anyhow::ensure!(load.left() >= bar.left() && load.right() <= bar.right());
            anyhow::ensure!(load.top() >= bar.top() && load.bottom() <= bar.bottom());
            h.view.update(cx, |view, cx| {
                view.set_edit_error("Another operation failed", cx)
            });
            window.render_frame(cx);
            anyhow::ensure!(window.try_find("app-status-tasks").is_some());
            anyhow::ensure!(window.try_find("app-status-message").is_none());
            Ok::<_, anyhow::Error>(())
        })??;
        // The ordinary capture helper rejects editor errors. This scene
        // checks error reporting while tasks retain the center slot.
        let frame = h.cx.capture_screenshot(h.window)?;
        save(
            &frame,
            directory,
            &format!("load-status-error-{locale}.png"),
        )?;
        h.view.update(&mut h.cx, |view, cx| {
            view.clear_edit_error(cx);
            view.record_load_warnings(&PathBuf::from("/Artwork/Character/layered.psd"), true,
                vec!["A layer blend mode was approximated during import.".into(),
                    "Physics data was not found. The model remains editable.\nYou can add physics in the editor.".into()], cx);
        });
        h.cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            window.click("app-status-problems", cx);
        })?;
        h.cx.advance_clock(Duration::from_millis(160));
        h.cx.run_until_parked();
        let frame = h.capture()?;
        save(
            &frame,
            directory,
            &format!("load-status-problems-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, _| {
            let popup = window.find("status-problems-content").bounds();
            let viewport = window.viewport_size();
            anyhow::ensure!(popup.left() >= gpui_kit::px(0.) && popup.right() <= viewport.width);
            anyhow::ensure!(popup.top() >= gpui_kit::px(0.) && popup.bottom() <= viewport.height);
            Ok::<_, anyhow::Error>(())
        })??;
        h.cx.update_window(h.window, |_, window, cx| {
            window.press("escape", cx);
        })?;
        h.view.update(&mut h.cx, |view, cx| {
            view.set_status_message(crate::t!("project.opened").into_owned(), cx);
        });
        h.cx.advance_clock(crate::EditorView::STATUS_MESSAGE_TIMEOUT * 2);
        h.cx.run_until_parked();
        let failed = queued.run();
        h.view.update(&mut h.cx, |view, cx| {
            view.clear_edit_error(cx);
            view.editor.finish_load(completion)?;
            anyhow::ensure!(view.editor.finish_load(failed).is_err());
            view.finish_load_status(first, cx);
            view.finish_load_status(second, cx);
            view.redraw(cx);
            Ok::<_, anyhow::Error>(())
        })?;
        let frame = h.capture()?;
        save(
            &frame,
            directory,
            &format!("load-status-feedback-only-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, _| {
            let message = window.find("app-status-message").bounds();
            let bar = window.find("app-status-bar").bounds();
            anyhow::ensure!((message.center().x - bar.center().x).abs() <= gpui_kit::px(0.5));
            anyhow::ensure!(window.try_find("app-status-tasks").is_none());
            anyhow::ensure!(window.try_find("app-status-problems").is_some());
            Ok::<_, anyhow::Error>(())
        })??;
    }
    Ok(())
}
