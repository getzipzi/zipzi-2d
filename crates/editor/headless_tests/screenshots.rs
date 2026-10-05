//! Pixel sensitivity checks for the real editor, including its asynchronous Metal preview.
#![cfg(target_os = "macos")]

use crate::{EditorView, i18n, menus, settings, theme};
use editor_core::{ConfigStore, Editor};
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
            menus::init(cx);
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
        let (bounds, scale) = self.cx.update_window(self.window, |_, window, _| {
            (window.find("canvas-body").bounds(), window.scale_factor())
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
    println!("Headless editor screenshots passed (real Metal, no native window).");
    Ok(())
}
