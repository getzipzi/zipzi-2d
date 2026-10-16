//! Per-instance GPU state and the window's shared content/render executors.
use crate::*;
use editor_core::{ViewportId, ViewportKind};

#[derive(Clone, Default)]
pub(crate) struct ViewportFrames {
    pub modeling: Option<preview::Presented>,
    pub animation: Option<animation::Presented>,
    pub error: Option<String>,
    pub stats: (u64, u64, u64),
}
impl EditorView {
    pub(crate) fn focused_viewport_id(&self, window: &Window, cx: &App) -> Option<ViewportId> {
        let surfaces = self
            .dock
            .as_ref()
            .map(|dock| dock.viewport_surfaces().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        #[cfg(any(test, feature = "test-support"))]
        let surfaces = {
            let mut surfaces = surfaces;
            if let Some(scene) = &self.viewport_scene {
                surfaces.extend(scene.surfaces.iter().cloned());
            }
            surfaces
        };
        surfaces.into_iter().find_map(|surface| {
            let surface = surface.read(cx);
            (surface.focus.is_focused(window)
                && self.editor.viewport_binding(surface.viewport).is_some())
            .then_some(surface.viewport)
        })
    }

    pub(crate) fn viewport_surface(
        &self,
        viewport: ViewportId,
        cx: &App,
    ) -> Option<Entity<viewports::ViewportSurface>> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(scene) = &self.viewport_scene {
            if let Some(surface) = scene
                .surfaces
                .iter()
                .find(|surface| surface.read(cx).viewport == viewport)
            {
                return Some(surface.clone());
            }
        }
        self.dock
            .as_ref()?
            .viewport_surfaces()
            .find(|surface| surface.read(cx).viewport == viewport)
            .cloned()
    }
    pub(crate) fn viewport_displayed(
        &self,
        viewport: ViewportId,
        window: &Window,
        cx: &App,
    ) -> bool {
        if !window.is_visible() {
            return false;
        }
        #[cfg(any(test, feature = "test-support"))]
        if let Some(scene) = &self.viewport_scene {
            return scene.instances.iter().any(|(id, _)| *id == viewport);
        }
        self.dock
            .as_ref()
            .is_some_and(|dock| dock.visible_viewports(cx).contains(&viewport))
    }
    pub(crate) fn modeling_frame_for(&self, viewport: ViewportId) -> Option<&preview::Presented> {
        self.viewport_frames
            .get(&viewport)?
            .modeling
            .as_ref()
            .filter(|frame| {
                self.editor.active_id() == Some(frame.canvas.version.binding.document)
                    && (self.editor.is_busy()
                        || self
                            .editor
                            .viewport_binding_valid(frame.canvas.version.binding))
            })
    }
    pub(crate) fn animation_frame_for(
        &self,
        viewport: ViewportId,
    ) -> Option<&animation::Presented> {
        self.viewport_frames
            .get(&viewport)?
            .animation
            .as_ref()
            .filter(|frame| {
                self.editor.active_id() == Some(frame.state.identity.document)
                    && (self.editor.is_busy()
                        || self
                            .editor
                            .can_display_animation_viewport(frame.state.viewport))
            })
    }
    pub(crate) fn modeling_frame(&self) -> Option<&preview::Presented> {
        self.modeling_frame_for(self.viewport_slots[0])
    }
    pub(crate) fn animation_frame(&self) -> Option<&animation::Presented> {
        self.animation_frame_for(self.viewport_slots[1])
    }
    pub(crate) fn viewport_error(&self, viewport: ViewportId) -> Option<&String> {
        self.viewport_frames.get(&viewport)?.error.as_ref()
    }
    pub(crate) fn render_error(&self) -> Option<String> {
        self.editor
            .primary_viewport_id()
            .and_then(|id| self.viewport_error(id))
            .cloned()
    }
    pub(crate) fn reset_viewport_images(&mut self) {
        self.viewport_frames.clear();
        self.animation_content = None;
    }

    pub(crate) fn connect_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        self._frames = None;
        self.requests = None;
        self.reset_viewport_images();
        self.editor.invalidate_animation();
        self.frozen_frames.clear();
        self.editor.invalidate_preview();
        let (requests, frames, device) = match preview::start(window) {
            Ok(connection) => connection,
            Err(error) => {
                for viewport in self.viewport_slots {
                    self.viewport_frames.entry(viewport).or_default().error =
                        Some(error.to_string());
                }
                return;
            }
        };
        if self.animation_worker.is_none() {
            self.init_animation(cx);
        }
        self.gpu_device = device;
        self._frames = Some(cx.spawn(async move |this, cx| {
            while let Ok(frame) = frames.recv().await {
                if this
                    .update(cx, |this, cx| this.receive_viewport_frame(frame, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.requests = Some(requests);
        self.redraw(cx);
    }
    pub(crate) fn receive_viewport_frame(&mut self, frame: preview::Frame, cx: &mut Context<Self>) {
        frame_trace::emit(frame.trace_id, "ui_received", &[]);
        let Some(binding) = frame.binding else {
            if let Err(error) = frame.image {
                for binding in self.editor.visible_viewport_bindings() {
                    self.viewport_frames
                        .entry(binding.viewport)
                        .or_default()
                        .error = Some(error.clone());
                }
                self.notify_canvas(cx);
            }
            return;
        };
        if !self.editor.viewport_binding_valid(binding) {
            return;
        }
        if let Some(state) = frame.animation {
            if state.viewport.binding != binding {
                return;
            }
            match frame.image {
                Ok(image) => {
                    if !self
                        .editor
                        .present_animation_viewport(state.viewport, state.snapshot.clone())
                    {
                        frame_trace::emit(frame.trace_id, "rejected", &[]);
                        return;
                    }
                    let previous = self.viewport_frames.entry(binding.viewport).or_default();
                    let changed = previous.animation.as_ref().is_none_or(|frame| {
                        !std::sync::Arc::ptr_eq(&frame.state.snapshot, &state.snapshot)
                    });
                    previous.animation = Some(animation::Presented { image, state });
                    previous.stats = frame.stats;
                    previous.error = None;
                    frame_trace::emit(frame.trace_id, "accepted", &[]);
                    self.sync_animation_clock(cx);
                    if changed {
                        self.notify_animation(cx);
                    } else {
                        self.notify_viewport(binding.viewport, cx);
                    }
                }
                Err(error) => {
                    if self.editor.accepts_animation_viewport_error(state.viewport) {
                        self.viewport_frames
                            .entry(binding.viewport)
                            .or_default()
                            .error = Some(error);
                        self.notify_viewport(binding.viewport, cx);
                    }
                }
            }
            return;
        }
        match frame.image {
            Ok(image) => {
                let Some(canvas) = frame.canvas else {
                    return;
                };
                if canvas.version.binding != binding {
                    return;
                }
                if !self.editor.present_frame(canvas.clone()) {
                    frame_trace::emit(frame.trace_id, "rejected", &[]);
                    return;
                }
                let state = self.viewport_frames.entry(binding.viewport).or_default();
                state.modeling = Some(preview::Presented { image, canvas });
                state.stats = frame.stats;
                state.error = None;
                frame_trace::emit(frame.trace_id, "accepted", &[]);
            }
            Err(error) => {
                if !self
                    .editor
                    .accepts_viewport_preview(binding, frame.revision)
                {
                    return;
                }
                self.viewport_frames
                    .entry(binding.viewport)
                    .or_default()
                    .error = Some(error);
            }
        }
        self.notify_viewport(binding.viewport, cx);
    }
    pub(crate) fn notify_viewport(&self, viewport: ViewportId, cx: &mut Context<Self>) {
        if let Some(dock) = &self.dock {
            for surface in dock.viewport_surfaces() {
                if surface.read(cx).viewport == viewport {
                    App::notify(cx, surface.entity_id());
                }
            }
        }
        #[cfg(any(test, feature = "test-support"))]
        if let Some(scene) = &self.viewport_scene {
            for surface in &scene.surfaces {
                if surface.read(cx).viewport == viewport {
                    App::notify(cx, surface.entity_id());
                }
            }
        }
    }
    pub(crate) fn request_preview_for_input(&mut self, input: u64) {
        let bindings = self.editor.visible_viewport_bindings();
        for binding in bindings
            .iter()
            .copied()
            .filter(|binding| binding.kind == ViewportKind::Modeling)
        {
            let trace_id = frame_trace::next_id();
            frame_trace::emit(trace_id, "scene_begin", &[("input", input)]);
            let preview = self.editor.preview_for_if_needed(binding);
            frame_trace::emit(trace_id, "scene_end", &[]);
            match preview {
                Ok(Some(preview)) => {
                    let Some(canvas) = preview.canvas else {
                        continue;
                    };
                    let Some((width, height)) = canvas.transform.metrics.physical_size() else {
                        continue;
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
                    self.send_viewport_request(preview::Request {
                        binding,
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
                    });
                }
                Ok(None) => {}
                Err(error) => {
                    self.viewport_frames
                        .entry(binding.viewport)
                        .or_default()
                        .error = Some(error.to_string())
                }
            }
        }
        let animation_visible = bindings.iter().any(|binding| {
            binding.kind == ViewportKind::Animation
                && self
                    .editor
                    .viewport_metrics(binding.viewport)
                    .is_some_and(|metrics| metrics.visible())
        });
        if animation_visible {
            if let Some(request) = self.editor.animation_request() {
                if let Some(worker) = &mut self.animation_worker {
                    worker.send(request);
                }
            }
            if let Some(content) = self
                .animation_content
                .clone()
                .filter(|content| self.editor.animation_content_valid(content.identity))
            {
                self.render_animation(content, frame_trace::next_id());
            }
        } else if let Some(worker) = &mut self.animation_worker {
            if worker.suspend() {
                // The cancelled request must not suppress its replacement when
                // a hidden viewport returns before any content was delivered.
                self.editor.invalidate_animation_requests();
                self.animation_content = None;
            }
        }
    }
    pub(crate) fn send_viewport_request(&mut self, request: preview::Request) {
        let binding = request.binding;
        if self
            .requests
            .as_ref()
            .is_none_or(|requests| requests.send(request).is_err())
        {
            self.viewport_frames
                .entry(binding.viewport)
                .or_default()
                .error = Some(t!("editor.preview_stopped").into_owned());
        }
    }
}
