use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{i18n, theme};
use editor_core::Editor;
use gpui_kit::{px, test::TestWindowExt};

pub(crate) fn timeline_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let (document, assets) = fixture::animation_document()?;
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.redraw(cx);
        });
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(editor_core::CommandId::FitView, window, cx);
            let projection = view.editor.timeline_projection().unwrap();
            let row = projection
                .rows
                .iter()
                .find(|row| row.track_id.is_some())
                .unwrap();
            let key = row.keys[0].clone();
            let row = row.id.clone();
            let d = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(d, |state| {
                    state.channels.insert(row);
                    state.keys.insert(key);
                })
                .unwrap();
            view.redraw(cx);
        });
    })?;
    let overview = harness.capture()?;
    save(&overview, directory, "timeline-overview.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let bounds = window.find("editor-bottom-dock").bounds();
        let scale = window.scale_factor();
        let expected =
            (gpui_kit::Rgba::from(crate::ui::compact::workspace(cx)).r * 255.).round() as u8;
        for fraction in [0.25, 0.5, 0.75] {
            let x = ((bounds.left().as_f32() + bounds.size.width.as_f32() * fraction) * scale)
                .floor() as u32;
            let y = (bounds.top().as_f32() * scale).ceil() as u32;
            let pixel = overview.get_pixel(x, y);
            anyhow::ensure!(
                pixel.0[..3]
                    .iter()
                    .all(|channel| channel.abs_diff(expected) <= 3),
                "Extra line above Timeline at ({x}, {y}): {:?}",
                pixel.0
            );
        }
        Ok::<_, anyhow::Error>(())
    })??;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-interpolation-bezier", cx);
        window.render_frame(cx);
        window.click("timeline-view", cx);
        window.render_frame(cx);
    })?;
    let curves = harness.capture()?;
    save(&curves, directory, "timeline-curves.png")?;
    anyhow::ensure!(
        overview != curves,
        "Timeline mode did not alter the rendered scene"
    );
    let canvas_before = harness.canvas_image(&curves)?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let key = harness
            .view
            .read(cx)
            .editor
            .timeline_view()
            .unwrap()
            .keys
            .first()
            .unwrap()
            .clone();
        window.click(format!("timeline-number-KeyValue({key:?})"), cx);
        window.press("cmd-a", cx);
        window.input("0.6", cx);
    })?;
    let candidate = harness.capture()?;
    save(&candidate, directory, "timeline-candidate.png")?;
    anyhow::ensure!(
        harness.canvas_image(&candidate)? != canvas_before,
        "Candidate key edit did not alter the rendered image"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        let view = harness.view.read(cx);
        anyhow::ensure!(
            view.editor.timeline_edit().is_some(),
            "Candidate was committed during preview"
        );
        anyhow::ensure!(
            view.editor.document().unwrap().history_lengths() == (1, 0),
            "Preview added history"
        );
        window.press("escape", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    let restored = harness.capture()?;
    save(&restored, directory, "timeline-cancel-restored.png")?;
    anyhow::ensure!(
        harness.canvas_image(&restored)? == canvas_before,
        "Cancel did not restore the canvas image"
    );
    harness.cx.update_window(harness.window, |_, _window, cx| {
        i18n::activate(Some("zh-CN"));
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
        harness.view.update(cx, |view, cx| view.redraw(cx));
    })?;
    save(
        &harness.capture()?,
        directory,
        "timeline-curves-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let panel = PanelId::from(dock.timeline.entity_id());
        let info = PanelId::from(dock.animation_info.entity_id());
        let area = dock.area.clone();
        let node = area
            .read(cx)
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(info)
            .unwrap();
        area.update(cx, |area, cx| {
            area.move_panel(
                panel,
                InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                },
                window,
                cx,
            )
        });
    })?;
    save(
        &harness.capture()?,
        directory,
        "timeline-narrow-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        anyhow::ensure!(
            window.find("timeline-graph").bounds().size.width >= px(150.),
            "Narrow dock lost its editable graph"
        );
        anyhow::ensure!(
            window.find("timeline-graph").bounds().size.height >= px(50.),
            "Narrow dock chrome consumed the graph height"
        );
        window.click("timeline-more", cx);
        window.render_frame(cx);
        window.click("timeline-menu-properties", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    save(
        &harness.capture()?,
        directory,
        "timeline-narrow-properties-zh-light.png",
    )?;
    i18n::activate(Some("en"));
    Ok(())
}
