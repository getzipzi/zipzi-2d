rust_i18n::i18n!("locales", fallback = "en");

mod actions;
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
        self.frozen_frames.clear();
        self.editor.invalidate_preview();
        let (requests, frames, device) = match preview::start(window) {
            Ok(connection) => connection,
            Err(error) => {
                self.render_error = Some(error.to_string());
                return;
            }
        };
        self.gpu_device = device;
        self.render_error = None;
        self._frames = Some(cx.spawn(async move |this, cx| {
            while let Ok(frame) = frames.recv().await {
                if this
                    .update(cx, |this, cx| {
                        frame_trace::emit(frame.trace_id, "ui_received", &[]);
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
        cx.notify();
    }

    fn notify_canvas(&self, cx: &mut Context<Self>) {
        if let Some(dock) = &self.dock {
            App::notify(cx, dock.canvas.entity_id());
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
                    canvas,
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
                    titlebar: Some(TitlebarOptions {
                        title: Some(t!("app.title").into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1100.), px(740.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(800.), px(480.))),
                    ..Default::default()
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
