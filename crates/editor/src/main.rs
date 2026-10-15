rust_i18n::i18n!("locales", fallback = "en");

mod actions;
mod animation;
mod benchmark;
mod canvas;
mod canvas_overlay;
mod commands;
mod dev_session;
mod docking;
#[cfg(test)]
#[path = "../headless_tests/mod.rs"]
mod headless_tests;
mod i18n;
mod keybindings;
mod menus;
mod mesh_edges;
mod mesh_thumbnails;
mod panels;
mod preview;
mod project;
mod settings;
mod theme;
mod ui;
mod worker;
mod workspace;

use editor_core::{CloseDisposition, Editor, EditorCommand, LoadRequest};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;

struct EditorView {
    focus: FocusHandle,
    modal: bool,
    frozen_frames: std::collections::HashMap<editor_core::DocumentId, preview::Presented>,
    editor: Editor,
    mesh_thumbnails: Entity<mesh_thumbnails::MeshThumbnails>,
    dock: Option<docking::DockWorkspace>,
    ui_subscriptions: Vec<Subscription>,
    frame: Option<preview::Presented>,
    animation_frame: Option<animation::Presented>,
    animation_worker: Option<animation::AnimationWorker>,
    _animation_results: Option<Task<()>>,
    animation_anchor: Option<(std::time::Instant, f32)>,
    animation_clock_running: bool,
    render_stats: (u64, u64, u64),
    menu_state: menus::MenuState,
    edit_error: Option<String>,
    render_error: Option<String>,
    message: String,
    requests: Option<preview::RequestSender>,
    gpu_device: Option<metal::Device>,
    _frames: Option<Task<()>>,
    status_bar: Entity<workspace::status_bar::StatusBar>,
    dev_session: Option<dev_session::DevSession>,
}
impl EditorView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut editor = Editor::empty();
        editor
            .set_workspace(cx.global::<settings::Settings>().workspace)
            .unwrap();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut editor = Self {
            focus,
            modal: false,
            frozen_frames: Default::default(),
            editor,
            mesh_thumbnails: mesh_thumbnails::MeshThumbnails::new(cx),
            dock: None,
            ui_subscriptions: Vec::new(),
            frame: None,
            animation_frame: None,
            animation_worker: None,
            _animation_results: None,
            animation_anchor: None,
            animation_clock_running: false,
            render_stats: (0, 0, 0),
            menu_state: menus::MenuState::default(),
            edit_error: None,
            render_error: None,
            message: if cx.global::<settings::Settings>().notices.is_empty() {
                t!("editor.welcome").into_owned()
            } else {
                cx.global::<settings::Settings>().notices.join("; ")
            },
            requests: None,
            gpu_device: None,
            _frames: None,
            status_bar: workspace::status_bar::StatusBar::new(cx.weak_entity(), cx),
            dev_session: dev_session::DevSession::from_env(),
        };
        let entity = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            entity
                .update(cx, |this, cx| {
                    this.close(window, cx);
                    false
                })
                .unwrap_or(true)
        });
        editor.init_dock(window, cx);
        if let Some(session) = &mut editor.dev_session {
            if let Err(error) = session.restore(&mut editor.editor) {
                editor.edit_error = Some(error.to_string());
            }
        }
        editor.connect_preview(window, cx);
        benchmark::start(&mut editor, window, cx);
        editor
    }

    fn complete_edit(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        self.edit_error = result.err().map(|error| error.to_string());
        self.redraw(cx);
    }

    fn complete_preview_edit(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        self.edit_error = result.err().map(|error| error.to_string());
        self.redraw_canvas(cx);
        if let Some(dock) = &self.dock {
            for kind in [
                panels::DocumentPanelKind::Keyforms,
                panels::DocumentPanelKind::DrawOrder,
            ] {
                App::notify(cx, dock.document_panels[&kind].entity_id());
            }
        }
    }

    fn connect_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        self._frames = None;
        self.requests = None;
        self.frame = None;
        self.animation_frame = None;
        self.editor.invalidate_animation();
        self.frozen_frames.clear();
        self.editor.invalidate_preview();
        let (requests, frames, device) = match preview::start(window) {
            Ok(connection) => connection,
            Err(error) => {
                self.render_error = Some(error.to_string());
                return;
            }
        };
        if self.animation_worker.is_none() {
            self.init_animation(cx);
        }
        self.gpu_device = device;
        self.render_error = None;
        self._frames = Some(cx.spawn(async move |this, cx| {
            while let Ok(frame) = frames.recv().await {
                if this
                    .update(cx, |this, cx| {
                        frame_trace::emit(frame.trace_id, "ui_received", &[]);
                        if let Some(state) = frame.animation {
                            let mut pose_changed = true;
                            match frame.image {
                                Ok(image) => {
                                    if !this
                                        .editor
                                        .present_animation(state.identity, state.snapshot.clone())
                                    {
                                        frame_trace::emit(frame.trace_id, "rejected", &[]);
                                        return;
                                    }
                                    frame_trace::emit(frame.trace_id, "accepted", &[]);
                                    this.render_stats = frame.stats;
                                    pose_changed =
                                        this.animation_frame.as_ref().is_none_or(|frame| {
                                            !std::sync::Arc::ptr_eq(
                                                &frame.state.snapshot,
                                                &state.snapshot,
                                            )
                                        });
                                    this.animation_frame =
                                        Some(animation::Presented { image, state });
                                }
                                Err(error) => this.editor.fail_animation(
                                    state.identity,
                                    editor_core::AnimationFailure::Other(error),
                                ),
                            }
                            this.sync_animation_clock(cx);
                            if pose_changed {
                                this.notify_animation(cx);
                            } else {
                                // Panning/zooming a completed pose only changes its image.
                                this.notify_canvas(cx);
                            }
                            return;
                        }
                        match frame.image {
                            Ok(image) => {
                                let Some(canvas) = frame.canvas else {
                                    return;
                                };
                                if !this.editor.present_frame(canvas.clone()) {
                                    frame_trace::emit(frame.trace_id, "rejected", &[]);
                                    return;
                                }
                                frame_trace::emit(frame.trace_id, "accepted", &[]);
                                this.render_stats = frame.stats;
                                this.frame = Some(preview::Presented { image, canvas });
                                if this.render_error.take().is_some() {
                                    cx.notify();
                                }
                            }
                            Err(error) => {
                                if frame.revision != 0
                                    && !this.editor.accepts_preview(frame.revision)
                                {
                                    return;
                                }
                                this.render_error = Some(error);
                                cx.notify();
                            }
                        }
                        this.notify_canvas(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.requests = Some(requests);
        self.redraw(cx);
    }

    fn redraw(&mut self, cx: &mut Context<Self>) {
        self.request_preview();
        self.sync_animation_clock(cx);
        cx.notify();
    }

    fn notify_animation(&self, cx: &mut Context<Self>) {
        self.notify_canvas(cx);
        if let Some(dock) = &self.dock {
            // A paused raw candidate is already shown by the editing controls.
            // Adopting its canvas texture does not change the Timeline UI.
            let editing_candidate = self.editor.timeline_edit().is_some()
                && self.editor.animation().is_some_and(|a| {
                    !a.playing
                        && a.error.is_none()
                        && a.preview_mode == editor_core::AnimationPreviewMode::Editing
                });
            if !editing_candidate {
                App::notify(cx, dock.timeline.entity_id());
            }
            App::notify(cx, dock.animation_info.entity_id());
            App::notify(cx, dock.animation_parameters.entity_id());
            for kind in [
                editor_core::PanelKind::AnimationPhysics,
                editor_core::PanelKind::AnimationPose,
            ] {
                if dock.contains(kind, cx) {
                    App::notify(cx, dock.animation_inspectors[&kind].entity_id());
                }
            }
        }
    }
    fn notify_canvas(&self, cx: &mut Context<Self>) {
        if let Some(dock) = &self.dock {
            for canvas in dock.canvas_views() {
                App::notify(cx, canvas.entity_id());
            }
        }
    }

    fn redraw_canvas(&mut self, cx: &mut Context<Self>) {
        self.redraw_canvas_for_input(cx, frame_trace::current_input());
    }

    fn redraw_canvas_for_input(&mut self, cx: &mut Context<Self>, input: u64) {
        self.request_preview_for_input(input);
        self.notify_canvas(cx);
        App::notify(cx, self.status_bar.entity_id());
    }

    fn request_preview(&mut self) {
        self.request_preview_for_input(frame_trace::current_input());
    }

    fn request_preview_for_input(&mut self, input: u64) {
        if self.editor.workspace() == editor_core::WorkspaceKind::Animation {
            if self.editor.is_busy()
                || !self.editor.canvas_metrics().visible()
                || self.editor.active_id().is_none()
            {
                if let Some(worker) = &mut self.animation_worker {
                    worker.suspend();
                }
            } else if let Some(request) = self.editor.animation_request() {
                if let Some(worker) = &mut self.animation_worker {
                    worker.send(request);
                }
            }
            return;
        }
        if let Some(worker) = &mut self.animation_worker {
            worker.suspend();
        }
        if self.editor.is_busy() {
            return;
        }
        let trace_id = frame_trace::next_id();
        frame_trace::emit(trace_id, "scene_begin", &[("input", input)]);
        let preview = self.editor.preview_if_needed();
        frame_trace::emit(trace_id, "scene_end", &[]);
        match preview {
            Ok(Some(preview)) => {
                let Some(canvas) = preview.canvas else {
                    return;
                };
                let Some((width, height)) = canvas.transform.metrics.physical_size() else {
                    return;
                };
                frame_trace::emit(
                    trace_id,
                    "request",
                    &[
                        ("revision", preview.revision),
                        ("width", width as u64),
                        ("height", height as u64),
                    ],
                );
                let request = preview::Request {
                    trace_id,
                    revision: preview.revision,
                    transform: canvas.transform,
                    canvas: Some(canvas),
                    animation: None,
                    scene: preview.scene,
                    target: editor_render::MetalTargetConfig {
                        width,
                        height,
                        format: metal::MTLPixelFormat::RGBA8Unorm,
                    },
                };
                if self
                    .requests
                    .as_ref()
                    .is_none_or(|requests| requests.send(request).is_err())
                {
                    frame_trace::emit(trace_id, "send_error", &[]);
                    self.render_error = Some(t!("editor.preview_stopped").into_owned());
                }
            }
            Ok(None) => {}
            Err(error) => self.render_error = Some(error.to_string()),
        }
    }
}

#[cfg(not(test))]
fn main() {
    // Record ownership before window creation and session restoration, which
    // can take longer than the dev launcher's startup timeout.
    if let Some(pid_file) = std::env::var_os("KASANE_DEV_PID_FILE") {
        let _ = std::fs::write(pid_file, std::process::id().to_string());
    }
    benchmark::init().expect("initialize benchmark trace");
    i18n::activate(None);
    let background_launch = std::env::var_os("KASANE_DEV_BACKGROUND").is_some();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.on_app_quit(|_| async {
                benchmark::finish();
            })
            .detach();
            let settings = settings::Settings::load(cx);
            i18n::activate(settings.language.as_deref());
            cx.set_global(settings);
            actions::init(cx);
            theme::init(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1100.), px(740.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(800.), px(480.))),
                    ..workspace::title_bar::window_options()
                },
                cx,
                |window, cx| cx.new(|cx| EditorView::new(window, cx)),
            )
            .expect("open kit editor");
            if !background_launch {
                cx.activate(true);
            }
        });
    benchmark::finish();
}

#[cfg(test)]
mod i18n_tests {
    use super::t;

    #[test]
    fn editor_catalog_is_wired_up() {
        assert_eq!(t!("editor.layers", locale = "en"), "Layers");
        assert_eq!(t!("editor.layers", locale = "zh-CN"), "图层");
        assert_eq!(
            t!("project.saved", locale = "zh-CN", path = "test.kasane"),
            "已保存 test.kasane"
        );
    }
}
