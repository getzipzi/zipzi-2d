rust_i18n::i18n!("locales", fallback = "en");

mod canvas;
mod docking;
mod i18n;
mod menus;
mod preview;
mod project;
mod settings;
mod ui;

use editor_core::{CloseDisposition, Editor, EditorCommand, LoadRequest};
use gpui_kit::component::{
    Theme, ThemeMode,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState, SliderValue},
};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;

struct EditorView {
    focus: FocusHandle,
    modal: bool,
    control_target: Option<(editor_core::DocumentId, Option<String>)>,
    frozen_frames: std::collections::HashMap<editor_core::DocumentId, preview::Presented>,
    editor: Editor,
    dock: Option<docking::DockWorkspace>,
    ui_subscriptions: Vec<Subscription>,
    name: Entity<InputState>,
    opacity: Entity<SliderState>,
    frame: Option<preview::Presented>,
    render_stats: (u64, u64, u64),
    menu_state: Vec<bool>,
    edit_error: Option<String>,
    render_error: Option<String>,
    message: String,
    requests: Option<preview::RequestSender>,
    gpu_device: Option<metal::Device>,
    _frames: Option<Task<()>>,
    #[cfg(debug_assertions)]
    _metal_hud_refresh: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}
impl EditorView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = Editor::new().expect("create blank document");
        let target = Self::property_target(&editor);
        let (name, opacity, subscriptions) = Self::create_controls(window, cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut editor = Self {
            focus,
            modal: false,
            control_target: target,
            frozen_frames: Default::default(),
            editor,
            dock: None,
            ui_subscriptions: Vec::new(),
            name,
            opacity,
            frame: None,
            render_stats: (0, 0, 0),
            menu_state: Vec::new(),
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
            #[cfg(debug_assertions)]
            _metal_hud_refresh: None,
            _subscriptions: subscriptions,
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
        editor.connect_preview(window, cx);
        #[cfg(debug_assertions)]
        if std::env::var("MTL_HUD_ENABLED").is_ok_and(|enabled| enabled == "1") {
            editor._metal_hud_refresh = Some(cx.spawn_in(window, async move |_, cx| {
                let executor = cx.background_executor().clone();
                loop {
                    executor.timer(std::time::Duration::from_secs(1)).await;
                    if cx.update(|window, _| window.refresh()).is_err() {
                        break;
                    }
                }
            }));
        }
        editor
    }

    fn create_controls(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<InputState>, Entity<SliderState>, Vec<Subscription>) {
        let name = cx
            .new(|cx| InputState::new(window, cx).placeholder(t!("editor.layer_name_placeholder")));
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
                move |this: &mut Self, input, event: &InputEvent, window, cx| {
                    if this.modal || this.editor.is_busy() || input != &this.name {
                        return;
                    }
                    let Some((id, _)) = this.control_target.as_ref() else {
                        return;
                    };
                    if this.control_target != Self::property_target(&this.editor) {
                        return;
                    }
                    let id = *id;
                    let result = match event {
                        InputEvent::Change => this.editor.execute_for(
                            id,
                            EditorCommand::PreviewName(input.read(cx).value().to_string()),
                        ),
                        InputEvent::PressEnter { .. } => return, // Commit is owned by the configured command.
                        InputEvent::Blur => this.editor.execute_for(id, EditorCommand::FinishEdit),
                        InputEvent::Focus => {
                            cx.notify();
                            return;
                        }
                    };
                    this.complete_edit(result, window, cx, false);
                },
            ),
            cx.subscribe_in(
                &opacity,
                window,
                move |this: &mut Self, slider, event: &SliderEvent, window, cx| {
                    if this.modal || this.editor.is_busy() || slider != &this.opacity {
                        return;
                    }
                    let Some((id, _)) = this.control_target.as_ref() else {
                        return;
                    };
                    if this.control_target != Self::property_target(&this.editor) {
                        return;
                    }
                    let id = *id;
                    let result = match event {
                        SliderEvent::Change(SliderValue::Single(value)) => this
                            .editor
                            .execute_for(id, EditorCommand::PreviewOpacity(value / 100.)),
                        SliderEvent::Release(_) => {
                            this.editor.execute_for(id, EditorCommand::FinishEdit)
                        }
                        _ => return,
                    };
                    this.complete_edit(result, window, cx, false);
                },
            ),
        ];
        (name, opacity, subscriptions)
    }

    fn property_target(editor: &Editor) -> Option<(editor_core::DocumentId, Option<String>)> {
        editor
            .active_id()
            .map(|id| (id, editor.selected_layer().map(|layer| layer.id)))
    }

    fn sync_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = Self::property_target(&self.editor);
        if self.control_target != target {
            // InputState owns text undo history, so even identically named layers
            // need separate controls. Entity checks also discard queued old events.
            let (name, opacity, subscriptions) = Self::create_controls(window, cx);
            self.name = name;
            self.opacity = opacity;
            self._subscriptions = subscriptions;
        }
        let layer = self.editor.selected_layer();
        let name = layer.as_ref().map_or("", |layer| layer.name.as_str());
        if self.name.read(cx).value().as_ref() != name {
            self.name
                .update(cx, |state, cx| state.set_value(name.to_owned(), window, cx));
        }
        let opacity = layer.as_ref().map_or(0., |layer| layer.opacity * 100.);
        self.opacity
            .update(cx, |state, cx| state.set_value(opacity, window, cx));
        self.control_target = target;
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
                        // The core pairs each request with the document active at submission.
                        if frame.revision != 0 && !this.editor.accepts_preview(frame.revision) {
                            return;
                        }
                        match frame.image {
                            Ok(image) => {
                                let Some(canvas) = frame.canvas else {
                                    return;
                                };
                                if !this.editor.present_frame(canvas.clone()) {
                                    return;
                                }
                                this.render_stats = frame.stats;
                                this.frame = Some(preview::Presented { image, canvas });
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
        if self.editor.is_busy() {
            cx.notify();
            return;
        }
        match self.editor.preview_if_needed() {
            Ok(Some(preview)) => {
                let Some(canvas) = preview.canvas else {
                    return;
                };
                let Some((width, height)) = canvas.transform.metrics.physical_size() else {
                    return;
                };
                let request = preview::Request {
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
                    self.render_error = Some(t!("editor.preview_stopped").into_owned());
                }
            }
            Ok(None) => {}
            Err(error) => self.render_error = Some(error.to_string()),
        }
        cx.notify();
    }
}

fn main() {
    i18n::activate(None);
    let background_launch = std::env::var_os("KASANE_DEV_BACKGROUND").is_some();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            let settings = settings::Settings::load(cx);
            cx.set_global(settings);
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
            if let Some(pid_file) = std::env::var_os("KASANE_DEV_PID_FILE") {
                let _ = std::fs::write(pid_file, std::process::id().to_string());
            }
            if !background_launch {
                cx.activate(true);
            }
        });
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
