//! Pixel sensitivity checks for the real editor, including its asynchronous Metal preview.
#![cfg(target_os = "macos")]

use crate::{EditorView, actions, i18n, settings, theme};
use editor_core::{ConfigStore, Editor};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, HeadlessAppContext, WindowBounds,
    WindowOptions, assets::AllAssets, point, px, size, test::TestWindowExt,
};
use image::RgbaImage;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use super::fixture;

pub(super) struct Harness {
    pub(super) window: AnyWindowHandle,
    pub(super) view: Entity<EditorView>,
    // Drop our entity handle before the context performs its leak check.
    pub(super) cx: HeadlessAppContext,
    // Keep files alive until the context releases workers and flushes the layout.
    _config: tempfile::TempDir,
    pub(super) _assets: tempfile::TempDir,
}

impl Harness {
    pub(super) fn new() -> anyhow::Result<Self> {
        Self::with_size(1100., 740.)
    }
    pub(super) fn with_size(width: f32, height: f32) -> anyhow::Result<Self> {
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
            // Pixel baselines capture settled controls rather than a spring's
            // intermediate frame after changing workspace or UI scale.
            cx.set_reduce_motion(true);
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
                        size(px(width), px(height)),
                    ))),
                    // A TestWindow has no OS window. Logical visibility allows canvas measurement.
                    show: true,
                    focus: false,
                    ..crate::workspace::title_bar::window_options()
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

    pub(super) fn capture(&mut self) -> anyhow::Result<RgbaImage> {
        // The preview worker uses real I/O rather than the simulated test clock.
        // Wait for the current accepted revision and thumbnails, with a bounded deadline.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.cx.run_until_parked();
            let ready = self.cx.update_window(self.window, |_, window, cx| {
                window.render_frame(cx);
                self.view.read_with(cx, |view, cx| {
                    anyhow::ensure!(
                        view.render_error().is_none(),
                        "Preview failed: {:?}",
                        view.render_error()
                    );
                    anyhow::ensure!(
                        view.edit_error.is_none(),
                        "Editor failed: {:?}",
                        view.edit_error
                    );
                    let current = if view.editor.active_id().is_none() {
                        true
                    } else if view.editor.workspace() == editor_core::WorkspaceKind::Animation {
                        view.animation_frame().is_some_and(|frame| {
                            view.editor
                                .can_display_animation_viewport(frame.state.viewport)
                                && view
                                    .editor
                                    .viewport_state(
                                        frame.state.viewport.binding.document,
                                        frame.state.viewport.binding.viewport,
                                    )
                                    .is_some_and(|state| {
                                        state.revision == frame.state.viewport.viewport_revision
                                    })
                                && view.editor.animation().is_some_and(|a| {
                                    a.time == frame.state.snapshot.time
                                        && a.candidate == frame.state.identity.candidate
                                        && a.model_generation
                                            == frame.state.identity.model_generation
                                        && a.preview_generation
                                            == frame.state.identity.preview_generation
                                        && a.preview_mode == frame.state.identity.preview_mode
                                        && a.presented.as_ref().is_some_and(|snapshot| {
                                            std::sync::Arc::ptr_eq(snapshot, &frame.state.snapshot)
                                        })
                                })
                        })
                    } else {
                        view.modeling_frame().is_some_and(|frame| {
                            view.editor.accepts_preview(frame.canvas.version.request)
                        })
                    };
                    let (_, pending) = view.mesh_thumbnails.read(cx).cache_state();
                    let mesh_ready = window.try_find("mesh-generation-form").is_none()
                        || window.try_find("mesh-generation-counts").is_some();
                    anyhow::ensure!(
                        window.try_find("mesh-generation-error").is_none(),
                        "Mesh preview failed"
                    );
                    Ok::<_, anyhow::Error>(current && !pending && mesh_ready)
                })
            })??;
            if ready {
                return self.cx.capture_screenshot(self.window);
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "Timed out waiting for editor preview: {:?}",
                self.view.read_with(&self.cx, |view, _| (
                    view.editor.workspace(),
                    view.editor.animation().map(|a| (
                        a.time,
                        a.playing,
                        a.presented.as_ref().map(|s| s.time),
                        a.error.clone()
                    )),
                    view.animation_frame().map(|f| (
                        f.state.snapshot.time,
                        view.editor.can_display_animation_viewport(f.state.viewport)
                    ))
                ))
            );
            self.cx.advance_clock(Duration::from_millis(10));
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn canvas_image(&mut self, image: &RgbaImage) -> anyhow::Result<RgbaImage> {
        self.element_image(image, "canvas-body")
    }

    pub(super) fn element_image(
        &mut self,
        image: &RgbaImage,
        id: &str,
    ) -> anyhow::Result<RgbaImage> {
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

    pub(super) fn dialog_image(&mut self, image: &RgbaImage) -> anyhow::Result<RgbaImage> {
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

pub(super) fn update<T>(
    h: &mut Harness,
    f: impl FnOnce(&mut EditorView, &mut gpui_kit::Window, &mut gpui_kit::Context<EditorView>) -> T,
) -> anyhow::Result<T> {
    Ok(h.cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| f(view, window, cx))
    })?)
}

pub(super) fn command(h: &mut Harness, command: editor_core::CommandId) -> anyhow::Result<()> {
    update(h, |view, window, cx| view.run_command(command, window, cx))
}
