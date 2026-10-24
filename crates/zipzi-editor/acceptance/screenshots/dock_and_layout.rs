use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{i18n, theme};
use gpui_kit::{BorrowAppContext as _, point, px, test::TestWindowExt};
use zipzi_editor_core::Editor;

pub(crate) fn compact_popup_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use crate::settings::{Settings, UiScale};
    use crate::theme::ColorTheme;
    for (mode, locale, appearance) in [
        (ColorTheme::Dark, "en", "dark"),
        (ColorTheme::Light, "zh-CN", "light-zh"),
        (ColorTheme::VsCodeDarkModern, "en", "vscode-dark-modern"),
        (
            ColorTheme::VsCodeLightModern,
            "zh-CN",
            "vscode-light-modern",
        ),
    ] {
        let mut harness = Harness::new()?;
        for scale in UiScale::ALL {
            harness
                .cx
                .update_window(harness.window, |_, window, _| window.activate_window())?;
            i18n::activate(Some(locale));
            harness.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                theme::change(mode, cx);
            });
            harness.view.update(&mut harness.cx, |_, cx| cx.notify());
            harness.capture()?;
            let name = format!(
                "compact-popup-{appearance}-{}",
                scale.label().trim_end_matches('%')
            );
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("project-select", cx);
            })?;
            save(
                &harness.capture()?,
                directory,
                &format!("{name}-projects.png"),
            )?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.press("escape", cx);
            })?;
            harness.capture()?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("workspace-layout-menu", cx);
                // The first entry opens an Add Panel submenu.
                window.within("popup-menu").hover(0usize, cx);
                window.render_frame(cx);
                let row = window
                    .within("submenu")
                    .within("popup-menu")
                    .find(0usize)
                    .bounds();
                anyhow::ensure!(
                    (row.size.height - px(20. * scale.factor())).abs() <= px(0.5),
                    "Submenu density at {}: {row:?}",
                    scale.label()
                );
                Ok::<_, anyhow::Error>(())
            })??;
            save(
                &harness.capture()?,
                directory,
                &format!("{name}-submenu.png"),
            )?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.press("escape", cx);
            })?;
            harness.capture()?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("new-parameter", cx);
            })?;
            harness.capture()?;
            for (id, suffix) in [
                ("property-select-kind", "select"),
                ("property-select-group_id", "search-select"),
            ] {
                harness.cx.update_window(harness.window, |_, window, cx| {
                    let bounds = window.find(id).bounds();
                    let input_height = window.find("property-input-name").bounds().size.height;
                    anyhow::ensure!(
                        (bounds.size.height - input_height).abs() <= px(0.5),
                        "Select density at {}: {bounds:?}",
                        scale.label()
                    );
                    window.click(id, cx);
                    Ok::<_, anyhow::Error>(())
                })??;
                save(
                    &harness.capture()?,
                    directory,
                    &format!("{name}-{suffix}.png"),
                )?;
                harness
                    .cx
                    .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
            }
            harness
                .cx
                .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
            harness.capture()?;
        }
    }
    i18n::activate(Some("en"));
    Ok(())
}

pub(crate) fn titlebar_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use gpui_kit::component::ThemeMode;
    for (mode, locale, appearance) in [
        (ThemeMode::Dark, "en", "dark"),
        (ThemeMode::Light, "zh-CN", "light-zh"),
    ] {
        let mut harness = Harness::new()?;
        i18n::activate(Some(locale));
        harness.cx.update(|cx| theme::change(mode, cx));
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                let first = view.editor.active_id().unwrap();
                view.editor.new_document().unwrap();
                view.editor.new_document().unwrap();
                view.editor.activate(first).unwrap();
                view.editor
                    .execute(zipzi_editor_core::EditorCommand::CreateParameter(
                        zipzi_editor_core::NewParameter {
                            name: "Test".into(),
                            runtime_id: "ParamTest".into(),
                            minimum: -1.,
                            maximum: 1.,
                            default_value: 0.,
                            ..Default::default()
                        },
                    ))
                    .unwrap();
                view.refresh_document(window, cx);
            });
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.click("project-select", cx)
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}-projects.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.press("escape", cx);
            harness.view.update(cx, |view, cx| {
                view.editor = Editor::empty();
                view.refresh_document(window, cx);
            });
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}-empty.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.click("project-select", cx)
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}-empty-menu.png"),
        )?;
    }
    i18n::activate(Some("en"));
    Ok(())
}

pub(crate) fn compact_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use crate::settings::{Settings, UiScale};
    use crate::theme::ColorTheme;
    use gpui_kit::component::dock::{DockPlacement, PaneNode, PaneRef};
    fn groups(node: &PaneNode, result: &mut Vec<u64>) {
        match node.kind() {
            PaneRef::Tabs { .. } => result.push(node.id().as_u64()),
            PaneRef::Split { children, .. } => {
                for child in children {
                    groups(child, result);
                }
            }
        }
    }
    let mut harness = Harness::new()?;
    let (mut document, assets) = fixture::sample_document()?;
    for index in 0..3 {
        document.create_parameter(zipzi_editor_core::NewParameter {
            name: format!("Parameter {index}"),
            runtime_id: format!("Param{index}"),
            minimum: -1.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
    }
    harness._assets = assets;
    harness.view.update(&mut harness.cx, |view, cx| {
        view.editor = Editor::from_document(document);
        view.redraw(cx);
    });
    let mut metrics = Vec::new();
    for scale in UiScale::ALL {
        for (mode, locale, appearance) in [
            (ColorTheme::Dark, "en", "dark"),
            (ColorTheme::Light, "zh-CN", "light-zh"),
            (ColorTheme::VsCodeDarkModern, "en", "vscode-dark-modern"),
            (
                ColorTheme::VsCodeLightModern,
                "zh-CN",
                "vscode-light-modern",
            ),
        ] {
            i18n::activate(Some(locale));
            harness.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                theme::change(mode, cx);
            });
            harness.view.update(&mut harness.cx, |_, cx| cx.notify());
            let screenshot = harness.capture()?;
            let name = format!(
                "compact-{appearance}-{}",
                scale.label().trim_end_matches('%')
            );
            save(&screenshot, directory, &format!("{name}.png"))?;
            let data = harness.cx.update_window(harness.window, |_, window, cx| {
                let display_scale = window.scale_factor();
                let canvas = window.find("canvas-body").bounds();
                let viewport = window.viewport_size();
                let mut nodes = Vec::new();
                let dock = harness.view.read(cx).dock.as_ref().unwrap();
                for placement in [DockPlacement::Center, DockPlacement::Left, DockPlacement::Right, DockPlacement::Bottom] {
                    if let Some(tree) = dock.area.read(cx).layout(placement) { groups(tree.root(), &mut nodes); }
                }
                let mut overflow = 0;
                let headers: Vec<_> = nodes.iter().filter_map(|node| {
                    let id = format!("dock-header-{node}");
                    let header = window.try_find(id.clone())?;
                    if window.within(id.clone()).try_find("panel-header-tools").is_some() { overflow += 1; }
                    let bounds = header.bounds();
                    Some(serde_json::json!({"node": node, "x": bounds.left().as_f32(), "y": bounds.top().as_f32(), "width": bounds.size.width.as_f32(), "height": bounds.size.height.as_f32()}))
                }).collect();
                // A local corner mask must cover real Metal content at every scale.
                let expected = (gpui_kit::Rgba::from(crate::ui::compact::workspace(cx)).r * 255.).round() as u8;
                for x in [(canvas.left().as_f32() * display_scale).ceil() as u32, (canvas.right().as_f32() * display_scale).floor() as u32 - 1] {
                    let pixel = screenshot.get_pixel(x, (canvas.bottom().as_f32() * display_scale).floor() as u32 - 1);
                    anyhow::ensure!(pixel.0[..3].iter().all(|channel| channel.abs_diff(expected) <= 3), "Metal corner leaked at {}: {:?}, canvas={:?}, x={:?}", scale.label(), pixel.0, canvas, x);
                }
                let tree_height = (window.find("parts-tree-panel").bounds().size.height.as_f32() - crate::ui::compact::ROW * scale.factor()).max(0.);
                Ok::<_, anyhow::Error>(serde_json::json!({"scene": name, "logical_window": [viewport.width.as_f32(), viewport.height.as_f32()], "display_scale": display_scale, "ui_scale": scale.factor(), "canvas": {"width": canvas.size.width.as_f32(), "height": canvas.size.height.as_f32(), "area": canvas.size.width.as_f32() * canvas.size.height.as_f32()}, "tree_row_capacity": (tree_height / (crate::ui::compact::ROW * scale.factor())).floor(), "header_overflow_count": overflow, "headers": headers}))
            })??;
            metrics.push(data);
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.hover("tool_glue", cx);
            })?;
            let hovered = harness.capture()?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                let bounds = window.find("tool_glue").bounds();
                let display_scale = window.scale_factor();
                for point in [
                    point(bounds.left(), bounds.center().y),
                    point(bounds.right() - px(0.5), bounds.center().y),
                    // The preceding button owns the joined top border.
                    point(bounds.center().x, bounds.top() - px(0.5)),
                    point(bounds.center().x, bounds.bottom() - px(0.5)),
                ] {
                    let x = (point.x.as_f32() * display_scale).floor() as u32;
                    let y = (point.y.as_f32() * display_scale).floor() as u32;
                    anyhow::ensure!(
                        screenshot.get_pixel(x, y) == hovered.get_pixel(x, y),
                        "Toolbar hover changed border at {name} ({x}, {y})"
                    );
                }
                let x = ((bounds.left() + px(4.)).as_f32() * display_scale).floor() as u32;
                let y = ((bounds.top() + px(4.)).as_f32() * display_scale).floor() as u32;
                anyhow::ensure!(
                    screenshot.get_pixel(x, y) != hovered.get_pixel(x, y),
                    "Toolbar hover did not change its fill at {name}"
                );
                window.hover("workspace-titlebar", cx);
                Ok::<_, anyhow::Error>(())
            })??;
            save(&hovered, directory, &format!("{name}-toolbar-hover.png"))?;
            let previous_zoom = harness
                .view
                .read_with(&harness.cx, |view, _| view.editor.view().zoom);
            harness.view.update(&mut harness.cx, |view, cx| {
                view.editor.set_zoom_at(50., None);
                view.redraw(cx);
            });
            let covered = harness.capture()?;
            save(&covered, directory, &format!("{name}-metal-corners.png"))?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                let canvas = window.find("canvas-body").bounds();
                let display_scale = window.scale_factor();
                let expected = (gpui_kit::Rgba::from(crate::ui::compact::workspace(cx)).r * 255.)
                    .round() as u8;
                for x in [
                    (canvas.left().as_f32() * display_scale).ceil() as u32,
                    (canvas.right().as_f32() * display_scale).floor() as u32 - 1,
                ] {
                    let pixel = covered.get_pixel(
                        x,
                        (canvas.bottom().as_f32() * display_scale).floor() as u32 - 1,
                    );
                    anyhow::ensure!(
                        pixel.0[..3]
                            .iter()
                            .all(|channel| channel.abs_diff(expected) <= 3),
                        "Opaque Metal leaked outside rounded canvas at {}: {:?}",
                        scale.label(),
                        pixel.0
                    );
                }
                let inside = covered.get_pixel(
                    ((canvas.left().as_f32() + 8.) * display_scale).floor() as u32,
                    ((canvas.bottom().as_f32() - 8.) * display_scale).floor() as u32,
                );
                anyhow::ensure!(
                    inside.0[1] > inside.0[0].saturating_add(20),
                    "Metal corner fixture did not cover the canvas: {:?}",
                    inside.0
                );
                Ok::<_, anyhow::Error>(())
            })??;
            harness.view.update(&mut harness.cx, |view, cx| {
                view.editor.set_zoom_at(previous_zoom, None);
                view.redraw(cx);
            });
        }
    }
    std::fs::write(
        directory.join("compact-metrics.json"),
        serde_json::to_vec_pretty(&metrics)?,
    )?;
    i18n::activate(Some("en"));
    Ok(())
}

pub(crate) fn dock_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use gpui_kit::component::dock::{DockLayout, DockPlacement, PanelId, panel_handle};
    let mut harness = Harness::new()?;
    save(&harness.capture()?, directory, "dock-workspace.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let node = dock
            .area
            .read(cx)
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(PanelId::from(dock.part_tree.entity_id()))
            .unwrap();
        window.click(format!("dock-selector-{}", node.as_u64()), cx);
    })?;
    save(&harness.capture()?, directory, "dock-panel-selector.png")?;
    harness
        .cx
        .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let area = dock.area.clone();
        let layout = DockLayout::h_split()
            .child(
                DockLayout::tabs().panel_view(panel_handle(dock.part_tree.clone()), cx),
                Some(px(100.)),
            )
            .child(
                DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                None,
            );
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
    })?;
    save(&harness.capture()?, directory, "dock-narrow.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("panel-header-tools", cx)
    })?;
    save(&harness.capture()?, directory, "dock-narrow-tools.png")?;
    harness
        .cx
        .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let area = dock.area.clone();
        let layout = DockLayout::tabs()
            .panel_view(dock.viewport_panel(), cx)
            .panel_view(panel_handle(dock.part_tree.clone()), cx);
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
    })?;
    save(&harness.capture()?, directory, "dock-canvas-group.png")?;
    i18n::activate(Some("zh-CN"));
    harness
        .cx
        .update(|cx| theme::change(gpui_kit::component::ThemeMode::Light, cx));
    harness.view.update(&mut harness.cx, |_, cx| cx.notify());
    save(
        &harness.capture()?,
        directory,
        "dock-canvas-group-zh-light.png",
    )?;
    i18n::activate(Some("en"));
    Ok(())
}
