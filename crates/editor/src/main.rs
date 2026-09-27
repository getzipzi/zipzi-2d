mod menus;
mod preview;
mod project;
mod ui;

use editor_scene::{Document, ViewState};
use gpui_kit::component::{
    Theme, ThemeMode,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState, SliderValue},
};
use gpui_kit::{prelude::*, *};
use std::sync::mpsc;

struct Editor {
    focus: FocusHandle,
    document: Document,
    view: ViewState,
    name: Entity<InputState>,
    opacity: Entity<SliderState>,
    frame: Option<preview::Image>,
    edit_error: Option<String>,
    render_error: Option<String>,
    message: String,
    busy: bool,
    revision: u64,
    preview_target: editor_render::MetalTargetConfig,
    requests: Option<mpsc::Sender<preview::Request>>,
    gpu_device: Option<metal::Device>,
    _frames: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}
impl Editor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let document = Document::new_blank().expect("create blank document");
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Layer name"));
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(0.)
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &name,
                window,
                |this: &mut Self, input, event: &InputEvent, window, cx| {
                    if this.busy {
                        return;
                    }
                    let result = match event {
                        InputEvent::Change => {
                            this.document.preview_name(input.read(cx).value().as_ref())
                        }
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            this.document.finish_edit()
                        }
                        _ => return,
                    };
                    this.complete_edit(result, window, cx, false);
                },
            ),
            cx.subscribe_in(
                &opacity,
                window,
                |this: &mut Self, _, event: &SliderEvent, window, cx| {
                    if this.busy {
                        return;
                    }
                    let result = match event {
                        SliderEvent::Change(SliderValue::Single(value)) => {
                            this.document.preview_opacity(value / 100.)
                        }
                        SliderEvent::Release(_) => this.document.finish_edit(),
                        _ => return,
                    };
                    this.complete_edit(result, window, cx, false);
                },
            ),
        ];
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut editor = Self {
            focus,
            document,
            view: ViewState::default(),
            name,
            opacity,
            frame: None,
            edit_error: None,
            render_error: None,
            message: "Open or import a project to get started.".into(),
            busy: false,
            revision: 0,
            preview_target: preview::target_config(window),
            requests: None,
            gpu_device: None,
            _frames: None,
            _subscriptions: subscriptions,
        };
        let entity = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            entity
                .update(cx, |this, cx| {
                    if this.busy {
                        return false;
                    }
                    if !this.document.modified() {
                        return true;
                    }
                    this.confirm_close(window, cx);
                    false
                })
                .unwrap_or(true)
        });
        editor.connect_preview(window, cx);
        editor
    }

    fn sync_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let layer = self.document.selected_layer();
        let name = layer.as_ref().map_or("", |layer| layer.name.as_str());
        if self.name.read(cx).value().as_ref() != name {
            self.name
                .update(cx, |state, cx| state.set_value(name.to_owned(), window, cx));
        }
        let opacity = layer.as_ref().map_or(0., |layer| layer.opacity * 100.);
        self.opacity
            .update(cx, |state, cx| state.set_value(opacity, window, cx));
    }

    fn complete_edit(
        &mut self,
        result: anyhow::Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
        sync: bool,
    ) {
        let failed = result.is_err();
        self.edit_error = result.err().map(|error| error.to_string());
        if sync || failed {
            self.sync_controls(window, cx);
        }
        self.redraw(cx);
    }

    fn connect_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        self._frames = None;
        self.requests = None;
        self.frame = None;
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
                        // Ignore old frames, including frames from a document replaced by Open.
                        if frame.revision != 0 && frame.revision != this.revision {
                            return;
                        }
                        match frame.image {
                            Ok(image) => {
                                this.frame = Some(image);
                                this.render_error = None;
                            }
                            Err(error) => {
                                this.render_error = Some(error);
                            }
                        }
                        cx.notify();
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
        self.revision += 1;
        match self.document.snapshot() {
            Ok(scene) => {
                let request = preview::Request {
                    revision: self.revision,
                    scene,
                    view: self.view,
                    target: self.preview_target,
                };
                if self
                    .requests
                    .as_ref()
                    .is_none_or(|requests| requests.send(request).is_err())
                {
                    self.render_error = Some("Preview renderer stopped. Reopen the window.".into());
                }
            }
            Err(error) => self.render_error = Some(error.to_string()),
        }
        cx.notify();
    }
}

fn main() {
    let background_launch = std::env::var_os("KASANE_DEV_BACKGROUND").is_some();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            menus::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            gpui_kit::open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some("Kasane — GPUI Kit".into()),
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
                |window, cx| cx.new(|cx| Editor::new(window, cx)),
            )
            .expect("open kit editor");
            if let Some(pid_file) = std::env::var_os("KASANE_DEV_PID_FILE") {
                let _ = std::fs::write(pid_file, std::process::id().to_string());
            }
            if !background_launch {
                cx.activate(true);
            }
        });
}
