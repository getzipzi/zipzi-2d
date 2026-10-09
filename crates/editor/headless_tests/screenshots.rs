//! Pixel sensitivity checks for the real editor, including its asynchronous Metal preview.
#![cfg(target_os = "macos")]

use crate::{EditorView, actions, i18n, settings, theme};
use editor_core::{ConfigStore, Editor};
use gpui_kit::component::WindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, HeadlessAppContext, WindowBounds,
    WindowOptions, assets::AllAssets, point, px, size, test::TestWindowExt,
};
use image::RgbaImage;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "../../editor-core/tests/support/mod.rs"]
#[allow(dead_code)]
mod fixture;

struct Harness {
    window: AnyWindowHandle,
    view: Entity<EditorView>,
    _config: tempfile::TempDir,
    _assets: tempfile::TempDir,
    // Drop our entity handle before the context performs its leak check.
    cx: HeadlessAppContext,
}

impl Harness {
    fn new() -> anyhow::Result<Self> {
        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(AllAssets),
            gpui_kit::platform::current_headless_renderer,
        );
        let config = tempfile::tempdir()?;
        let (document, assets) = fixture::sample_document()?;
        i18n::activate(Some("en"));
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_global(settings::Settings::from_store(
                ConfigStore::new(config.path()),
                cx.keyboard_mapper().as_ref(),
            ));
            actions::init(cx);
            theme::init(cx);
        });
        let (window, view) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(1100.), px(740.)),
                    ))),
                    // A TestWindow has no OS window. Logical visibility allows canvas measurement.
                    show: true,
                    focus: false,
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| EditorView::new(window, cx)),
            )
        })?;
        cx.update_window(window, |_, window, cx| {
            anyhow::ensure!(
                window.metal_context().is_some(),
                "Headless Metal context unavailable"
            );
            view.update(cx, |view, cx| {
                view.dev_session = None;
                view.editor = Editor::from_document(document);
                view.editor
                    .execute(editor_core::EditorCommand::SelectObjects(Vec::new()))
                    .unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
            Ok::<_, anyhow::Error>(())
        })??;
        Ok(Self {
            cx,
            window,
            view,
            _config: config,
            _assets: assets,
        })
    }

    fn capture(&mut self) -> anyhow::Result<RgbaImage> {
        // The preview worker uses real I/O rather than the simulated test clock.
        // Wait for the current accepted revision and thumbnails, with a bounded deadline.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.cx.run_until_parked();
            let ready = self.cx.update_window(self.window, |_, window, cx| {
                window.render_frame(cx);
                self.view.read_with(cx, |view, cx| {
                    anyhow::ensure!(
                        view.render_error.is_none(),
                        "Preview failed: {:?}",
                        view.render_error
                    );
                    anyhow::ensure!(
                        view.edit_error.is_none(),
                        "Editor failed: {:?}",
                        view.edit_error
                    );
                    let current = view.frame.as_ref().is_some_and(|frame| {
                        view.editor.accepts_preview(frame.canvas.version.request)
                    });
                    let (_, pending) = view.mesh_thumbnails.read(cx).cache_state();
                    Ok::<_, anyhow::Error>(current && !pending)
                })
            })??;
            if ready {
                return self.cx.capture_screenshot(self.window);
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "Timed out waiting for editor preview"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn canvas_image(&mut self, image: &RgbaImage) -> anyhow::Result<RgbaImage> {
        self.element_image(image, "canvas-body")
    }

    fn element_image(&mut self, image: &RgbaImage, id: &str) -> anyhow::Result<RgbaImage> {
        let (bounds, scale) = self.cx.update_window(self.window, |_, window, _| {
            (
                window
                    .find(gpui_kit::SharedString::from(id.to_owned()))
                    .bounds(),
                window.scale_factor(),
            )
        })?;
        Ok(image::imageops::crop_imm(
            image,
            (bounds.origin.x.as_f32() * scale).ceil() as u32,
            (bounds.origin.y.as_f32() * scale).ceil() as u32,
            (bounds.size.width.as_f32() * scale).floor() as u32,
            (bounds.size.height.as_f32() * scale).floor() as u32,
        )
        .to_image())
    }

    fn dialog_image(&mut self, image: &RgbaImage) -> anyhow::Result<RgbaImage> {
        let (bounds, scale) = self.cx.update_window(self.window, |_, window, _| {
            (
                window.within("dialog").find(0_usize).bounds(),
                window.scale_factor(),
            )
        })?;
        Ok(image::imageops::crop_imm(
            image,
            (bounds.origin.x.as_f32() * scale).ceil() as u32,
            (bounds.origin.y.as_f32() * scale).ceil() as u32,
            (bounds.size.width.as_f32() * scale).floor() as u32,
            (bounds.size.height.as_f32() * scale).floor() as u32,
        )
        .to_image())
    }
}

fn save(image: &RgbaImage, directory: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let path = directory.join(name);
    image.save_with_format(&path, image::ImageFormat::Png)?;
    // Verify the actual exported artifact, not just the in-memory buffer.
    anyhow::ensure!(
        image::open(&path)?.into_rgba8() == *image,
        "PNG round trip changed pixels"
    );
    println!("Screenshot: {}", path.display());
    Ok(())
}

pub(super) fn run() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("KASANE_DEV_SESSION_DIR").is_none(),
        "Unset KASANE_DEV_SESSION_DIR to keep screenshots isolated from development sessions"
    );
    let directory = std::env::var_os("KASANE_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/headless-screenshots")
        });
    std::fs::create_dir_all(&directory)?;
    let mut harness = Harness::new()?;
    let baseline = harness.capture()?;
    anyhow::ensure!(
        baseline.dimensions() == (2200, 1480),
        "Unexpected screenshot size"
    );
    anyhow::ensure!(
        baseline == harness.capture()?,
        "Unchanged editor must render identical pixels"
    );
    save(&baseline, &directory, "editor.png")?;
    let baseline_canvas = harness.canvas_image(&baseline)?;

    let mesh = harness
        .view
        .read_with(&harness.cx, |view, _| view.editor.layers()[0].id.clone());
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("canvas-body", cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer_id(), Some(mesh.as_str()));
        });
    })?;
    let selected = harness.capture()?;
    let selected_canvas = harness.canvas_image(&selected)?;
    let marker = harness.view.read_with(&harness.cx, |view, _| {
        let position = view.editor.overlay().vertices[0].position;
        let scale = view
            .frame
            .as_ref()
            .unwrap()
            .canvas
            .transform
            .metrics
            .scale_factor;
        (
            (position.x * scale).round() as u32,
            (position.y * scale).round() as u32,
        )
    });
    anyhow::ensure!(
        image::imageops::crop_imm(&selected_canvas, marker.0 - 8, marker.1 - 8, 16, 16).to_image()
            != image::imageops::crop_imm(&baseline_canvas, marker.0 - 8, marker.1 - 8, 16, 16)
                .to_image(),
        "Selecting the mesh must paint a vertex marker at its canvas position"
    );
    save(&selected, &directory, "editor-selected.png")?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window.hover(format!("mesh-item-Mesh:{mesh}"), cx);
        window
            .within("mesh-tree-panel")
            .click(format!("mesh-lock-{mesh}"), cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.overlay().vertices.is_empty());
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .object_state_index()
                    .is_locked(&editor_core::ObjectRef::new(
                        editor_core::ObjectKind::Mesh,
                        &mesh
                    ))
            );
        });
    })?;
    let locked = harness.capture()?;
    let locked_canvas = harness.canvas_image(&locked)?;
    anyhow::ensure!(
        image::imageops::crop_imm(&locked_canvas, marker.0 - 8, marker.1 - 8, 16, 16).to_image()
            != image::imageops::crop_imm(&selected_canvas, marker.0 - 8, marker.1 - 8, 16, 16)
                .to_image(),
        "Locking must remove the editable vertex marker"
    );
    let (w, h) = locked_canvas.dimensions();
    anyhow::ensure!(
        image::imageops::crop_imm(&locked_canvas, w / 2 - 10, h / 2 - 10, 20, 20).to_image()
            == image::imageops::crop_imm(&selected_canvas, w / 2 - 10, h / 2 - 10, 20, 20)
                .to_image(),
        "Locking must preserve the rendered artwork"
    );
    save(&locked, &directory, "editor-locked.png")?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window
            .within("mesh-tree-panel")
            .click(format!("mesh-vis-{mesh}"), cx);
    })?;
    let hidden = harness.capture()?;
    let hidden_canvas = harness.canvas_image(&hidden)?;
    let changed = hidden_canvas
        .pixels()
        .zip(selected_canvas.pixels())
        .filter(|(a, b)| a != b)
        .count();
    anyhow::ensure!(
        changed > (hidden_canvas.width() * hidden_canvas.height()) as usize / 20,
        "Hiding the model must remove its filled pixels, not just the overlay"
    );
    save(&hidden, &directory, "editor-hidden.png")?;

    let (document, assets) = fixture::sample_document_with_parameter_groups()?;
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            let dock = view.dock.as_ref().unwrap();
            // Give the grouped list a full-height, standard-width panel for visual QA.
            use gpui_kit::component::dock::{DockLayout, panel_handle};
            dock.area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::h_split()
                        .child(
                            DockLayout::tabs().panel_view(panel_handle(dock.canvas.clone()), cx),
                            None,
                        )
                        .child(
                            DockLayout::tabs()
                                .panel_view(panel_handle(dock.parameters.clone()), cx),
                            Some(px(360.)),
                        ),
                    window,
                    cx,
                );
            });
            view.redraw(cx);
        });
    })?;
    let grouped = harness.capture()?;
    save(
        &harness.element_image(&grouped, "parameters-panel")?,
        &directory,
        "parameters-grouped.png",
    )?;
    let face = uuid::Uuid::from_u128(101).to_string();
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-group-toggle-{face}"), cx);
        assert!(
            !window
                .find(format!("parameter-group-{face}"))
                .expanded()
                .unwrap()
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", uuid::Uuid::from_u128(3)))
                .is_none()
        );
    })?;
    let collapsed = harness.capture()?;
    let collapsed_panel = harness.element_image(&collapsed, "parameters-panel")?;
    anyhow::ensure!(
        collapsed != grouped,
        "Collapsing a parameter group must change its rendered rows"
    );
    save(&collapsed_panel, &directory, "parameters-collapsed.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-group-toggle-{face}"), cx);
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
    })?;
    let light = harness.capture()?;
    save(
        &harness.element_image(&light, "parameters-panel")?,
        &directory,
        "parameters-grouped-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        cx.set_reduce_motion(true);
        theme::change(gpui_kit::component::ThemeMode::Dark, cx);
        window.click("new-parameter", cx);
    })?;
    let popup = harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, _| {
        assert!(window.try_find("new-parameter-advanced").is_none());
        for field in [
            "new-parameter-precision",
            "new-parameter-kind",
            "new-parameter-group",
            "new-parameter-repeat",
        ] {
            assert!(
                window.find(field).visible(),
                "{field} must be visible on open"
            );
        }
    })?;
    save(
        &harness.dialog_image(&popup)?,
        &directory,
        "new-parameter.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-cancel", cx);
        i18n::activate(Some("zh-CN"));
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
        window.click("new-parameter", cx);
    })?;
    let popup_zh = harness.capture()?;
    save(
        &harness.dialog_image(&popup_zh)?,
        &directory,
        "new-parameter-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-cancel", cx);
    })?;
    for (action, filename) in [
        (
            crate::panels::parameters::ParameterAction::Edit(uuid::Uuid::from_u128(3).to_string()),
            "edit-parameter-zh-light.png",
        ),
        (
            crate::panels::parameters::ParameterAction::CreateGroup,
            "new-parameter-group-zh-light.png",
        ),
        (
            crate::panels::parameters::ParameterAction::DeleteGroup(
                uuid::Uuid::from_u128(101).to_string(),
            ),
            "delete-parameter-group-zh-light.png",
        ),
    ] {
        harness.cx.update_window(harness.window, |_, window, cx| {
            let panel = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .parameters
                .clone();
            panel.update(cx, |panel, cx| panel.open_management(action, window, cx));
        })?;
        let popup = harness.capture()?;
        save(&harness.dialog_image(&popup)?, &directory, filename)?;
        harness
            .cx
            .update_window(harness.window, |_, window, cx| window.close_dialog(cx))?;
    }
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
    let mesh_preview = harness.capture()?;
    save(
        &harness.dialog_image(&mesh_preview)?,
        &directory,
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
    let mesh_preview = harness.capture()?;
    save(
        &harness.dialog_image(&mesh_preview)?,
        &directory,
        "mesh-generation-dark.png",
    )?;
    let fitted = harness.element_image(&mesh_preview, "preview-viewport")?;
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
        &directory,
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
    println!("Headless editor screenshots passed (real Metal, no native window).");
    Ok(())
}
