rust_i18n::i18n!("assets/locales", fallback = "en");

#[cfg(all(feature = "test-support", target_os = "macos"))]
#[path = "../acceptance/mod.rs"]
pub mod acceptance;
mod actions;
mod animation;
mod authoring;
mod benchmark;
mod canvas_overlay;
mod commands;
mod dev_session;
mod docking;
mod editing;
#[cfg(test)]
#[path = "../headless_tests/mod.rs"]
mod headless_tests;
mod i18n;
mod input_routing;
mod keybindings;
mod menus;
mod mesh_edges;
mod mesh_thumbnails;
mod panel_lifecycle;
mod panels;
mod presentation;
mod preview;
mod project;
mod properties;
mod settings;
#[cfg(any(test, feature = "test-support"))]
#[path = "../../editor-core/tests/support/mod.rs"]
pub(crate) mod test_fixture;
mod theme;
mod transfer;
mod ui;
mod viewports;
mod worker;
mod worker_queue;
mod workspace;

use editor_core::{CloseDisposition, Editor, EditorCommand, LoadRequest};
use gpui_kit::{prelude::*, *};
use rust_i18n::t;

struct EditorView {
    input_router: input_routing::InputRouter,
    focus: FocusHandle,
    modal: bool,
    focused_viewport: Option<editor_core::ViewportId>,
    frozen_frames: std::collections::HashMap<
        editor_core::DocumentId,
        std::collections::HashMap<editor_core::ViewportId, presentation::ViewportFrames>,
    >,
    editor: Editor,
    property_forms: properties::form::PropertyForms,
    playback_dialog: Option<authoring::PlaybackDialogGuard>,
    texture_loads: std::collections::HashMap<String, authoring::texture::TextureLoad>,
    viewport_slots: [editor_core::ViewportId; 2],
    #[cfg(any(test, feature = "test-support"))]
    viewport_scene: Option<viewports::ViewportScene>,
    mesh_thumbnails: Entity<mesh_thumbnails::MeshThumbnails>,
    dock: Option<docking::DockWorkspace>,
    ui_subscriptions: Vec<Subscription>,
    viewport_frames:
        std::collections::HashMap<editor_core::ViewportId, presentation::ViewportFrames>,
    animation_content: Option<editor_core::AnimationEvaluated>,
    animation_worker: Option<animation::AnimationWorker>,
    _animation_results: Option<Task<()>>,
    animation_anchor: Option<(std::time::Instant, f32)>,
    animation_clock_running: bool,
    menu_state: menus::MenuState,
    edit_error: Option<String>,
    message: String,
    status_message_task: Option<Task<()>>,
    requests: Option<preview::RequestSender>,
    gpu_device: Option<metal::Device>,
    _frames: Option<Task<()>>,
    status_bar: Entity<workspace::status_bar::StatusBar>,
    dev_session: Option<dev_session::DevSession>,
}
impl EditorView {
    pub(crate) const STATUS_MESSAGE_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(4);

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut editor = Editor::empty();
        editor
            .set_workspace(cx.global::<settings::Settings>().workspace)
            .unwrap();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut editor = Self {
            input_router: Default::default(),
            focus,
            modal: false,
            focused_viewport: None,
            frozen_frames: Default::default(),
            editor,
            property_forms: Default::default(),
            playback_dialog: None,
            texture_loads: Default::default(),
            viewport_slots: std::array::from_fn(|_| editor_core::ViewportId::new()),
            #[cfg(any(test, feature = "test-support"))]
            viewport_scene: None,
            mesh_thumbnails: mesh_thumbnails::MeshThumbnails::new(cx),
            dock: None,
            ui_subscriptions: Vec::new(),
            viewport_frames: Default::default(),
            animation_content: None,
            animation_worker: None,
            _animation_results: None,
            animation_anchor: None,
            animation_clock_running: false,
            menu_state: menus::MenuState::default(),
            edit_error: None,
            message: if cx.global::<settings::Settings>().notices.is_empty() {
                t!("editor.welcome").into_owned()
            } else {
                cx.global::<settings::Settings>().notices.join("; ")
            },
            status_message_task: None,
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
        editor.connect_panel_lifecycle(window, cx);
        editor.sync_viewports();
        editor.init_dock(window, cx);
        if let Some(session) = &mut editor.dev_session {
            if let Err(error) = session.restore(&mut editor.editor) {
                editor.edit_error = Some(error.to_string());
            }
        }
        editor.schedule_status_clear(cx);
        editor.connect_preview(window, cx);
        benchmark::start(&mut editor, window, cx);
        editor
    }

    pub(crate) fn set_edit_error(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
        self.edit_error = Some(error.into());
        self.schedule_status_clear(cx);
        workspace::status_bar::StatusBar::sync(&self.status_bar, self, cx);
        cx.notify();
    }

    pub(crate) fn clear_edit_error(&mut self, cx: &mut Context<Self>) {
        if self.edit_error.take().is_some() {
            if self.message.is_empty() {
                self.status_message_task = None;
            }
            workspace::status_bar::StatusBar::sync(&self.status_bar, self, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_status_message(
        &mut self,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.edit_error = None;
        self.message = message.into();
        self.schedule_status_clear(cx);
        workspace::status_bar::StatusBar::sync(&self.status_bar, self, cx);
        cx.notify();
    }

    fn schedule_status_clear(&mut self, cx: &mut Context<Self>) {
        if self.edit_error.is_none() && self.message.is_empty() {
            self.status_message_task = None;
            return;
        }
        self.status_message_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Self::STATUS_MESSAGE_TIMEOUT)
                .await;
            _ = this.update(cx, |this, cx| {
                this.edit_error = None;
                this.message.clear();
                this.status_message_task = None;
                workspace::status_bar::StatusBar::sync(&this.status_bar, this, cx);
                cx.notify();
            });
        }));
    }

    fn complete_edit(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.clear_edit_error(cx),
            Err(error) => self.set_edit_error(error.to_string(), cx),
        }
        self.redraw(cx);
    }

    fn complete_preview_edit(&mut self, result: anyhow::Result<()>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.clear_edit_error(cx),
            Err(error) => self.set_edit_error(error.to_string(), cx),
        }
        self.redraw_canvas(cx);
        if let Some(dock) = &self.dock {
            for kind in [
                editor_core::PanelKind::Keyforms,
                editor_core::PanelKind::DrawOrder,
            ] {
                App::notify(cx, dock.document_panels[&kind].entity_id());
            }
        }
    }

    fn redraw(&mut self, cx: &mut Context<Self>) {
        self.sync_viewports();
        self.request_preview();
        self.sync_animation_clock(cx);
        workspace::status_bar::StatusBar::sync(&self.status_bar, self, cx);
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
            if dock.displayed(editor_core::PanelKind::Properties, cx)
                && self
                    .editor
                    .active_inspection()
                    .and_then(|i| i.object())
                    .is_some_and(|o| {
                        matches!(
                            o.kind,
                            editor_core::ObjectKind::PhysicsGroup
                                | editor_core::ObjectKind::PhysicsInput
                                | editor_core::ObjectKind::PhysicsOutput
                                | editor_core::ObjectKind::PhysicsNode
                        )
                    })
            {
                App::notify(cx, dock.properties.entity_id());
            }
            for kind in [editor_core::PanelKind::AnimationPose] {
                if dock.contains(kind, cx) {
                    App::notify(cx, dock.animation_inspectors[&kind].entity_id());
                }
            }
        }
    }
    fn notify_canvas(&self, cx: &mut Context<Self>) {
        if let Some(dock) = &self.dock {
            for canvas in dock.viewport_surfaces() {
                App::notify(cx, canvas.entity_id());
            }
        }
        #[cfg(any(test, feature = "test-support"))]
        if let Some(scene) = &self.viewport_scene {
            for surface in &scene.surfaces {
                App::notify(cx, surface.entity_id());
            }
        }
    }

    fn redraw_canvas(&mut self, cx: &mut Context<Self>) {
        self.redraw_canvas_for_input(cx, frame_trace::current_input());
    }

    fn redraw_canvas_for_input(&mut self, cx: &mut Context<Self>, input: u64) {
        self.request_preview_for_input(input);
        self.notify_canvas(cx);
        workspace::status_bar::StatusBar::sync(&self.status_bar, self, cx);
    }

    fn request_preview(&mut self) {
        self.request_preview_for_input(frame_trace::current_input());
    }
}

pub fn run() {
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
