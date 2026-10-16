//! Per-document navigation addressed by runtime viewport identity.
use super::*;

impl Editor {
    pub fn viewport_state(
        &self,
        document: DocumentId,
        viewport: crate::ViewportId,
    ) -> Option<crate::ViewportState> {
        self.entry(document).ok()?.viewport_state(viewport)
    }
    pub(crate) fn ensure_viewport_states(&mut self, viewport: crate::ViewportId) {
        for entry in &mut self.documents {
            entry.ensure_viewport_state(viewport);
        }
    }
    pub(crate) fn invalidate_viewport_state(&mut self, viewport: crate::ViewportId) {
        for entry in &mut self.documents {
            if let Some(session) = entry.session_mut() {
                if let Some(state) = session.views.get_mut(&viewport) {
                    state.revision += 1;
                }
            }
        }
    }
    pub fn viewport_view(&self, binding: crate::ViewportBinding) -> Option<ViewState> {
        self.viewport_binding_valid(binding)
            .then(|| {
                self.viewport_state(binding.document, binding.viewport)
                    .map(|state| state.view)
            })
            .flatten()
    }
    fn navigate_viewport(
        &mut self,
        binding: crate::ViewportBinding,
        update: impl FnOnce(&mut crate::ViewportState, crate::CanvasMetrics),
    ) -> Result<()> {
        ensure!(
            self.viewport_binding_valid(binding),
            "Viewport binding changed"
        );
        ensure!(!self.gesture_active(), "A Modeling gesture is active");
        let metrics = self.viewport_metrics(binding.viewport).unwrap();
        let session = self.entry_mut(binding.document)?.session_mut().unwrap();
        update(session.views.entry(binding.viewport).or_default(), metrics);
        Ok(())
    }
    pub fn zoom_viewport(
        &mut self,
        binding: crate::ViewportBinding,
        delta: f32,
        focus: Option<crate::CanvasPoint>,
    ) -> Result<()> {
        self.navigate_viewport(binding, |state, metrics| {
            state.set_zoom(state.view.zoom + delta, focus, metrics)
        })
    }
    pub fn set_viewport_zoom(
        &mut self,
        binding: crate::ViewportBinding,
        zoom: f32,
        focus: Option<crate::CanvasPoint>,
    ) -> Result<()> {
        self.navigate_viewport(binding, |state, metrics| {
            state.set_zoom(zoom, focus, metrics)
        })
    }
    pub fn pan_viewport(
        &mut self,
        binding: crate::ViewportBinding,
        delta: crate::CanvasPoint,
    ) -> Result<()> {
        self.navigate_viewport(binding, |state, _| state.pan(delta))
    }
    pub fn reset_viewport(&mut self, binding: crate::ViewportBinding) -> Result<()> {
        self.navigate_viewport(binding, |state, _| state.reset())
    }
    pub fn fit_viewport(&mut self, binding: crate::ViewportBinding) -> Result<()> {
        let canvas = self
            .document()
            .ok_or_else(|| anyhow!("No active document"))?
            .canvas();
        self.navigate_viewport(binding, |state, metrics| {
            if metrics.visible() {
                state.default_fit = false;
                state.fit(canvas, metrics);
            }
        })
    }
    pub fn zoom_at(&mut self, delta: f32, focus: Option<crate::CanvasPoint>) {
        if let Some(binding) = self.primary_viewport_binding() {
            let _ = self.zoom_viewport(binding, delta, focus);
        }
    }
    pub fn zoom_by(&mut self, delta: f32) {
        self.zoom_at(delta, None);
    }
    pub fn set_zoom_at(&mut self, zoom: f32, focus: Option<crate::CanvasPoint>) {
        if let Some(binding) = self.primary_viewport_binding() {
            let _ = self.set_viewport_zoom(binding, zoom, focus);
        }
    }
    pub fn reset_view(&mut self) {
        if let Some(binding) = self.primary_viewport_binding() {
            let _ = self.reset_viewport(binding);
        }
    }
    pub fn fit_view(&mut self) {
        if let Some(binding) = self.primary_viewport_binding() {
            let _ = self.fit_viewport(binding);
        }
    }
    pub(crate) fn invalidate_primary_presentation(&mut self) {
        self.cancel_gesture();
        self.invalidate_preview();
        self.invalidate_animation();
    }
    pub fn invalidate_preview(&mut self) {
        for viewport in self.viewport_ids() {
            if let Some(state) = self.viewport_presentation_mut(viewport) {
                state.invalidate();
            }
        }
    }
    pub fn set_canvas_metrics(&mut self, metrics: crate::CanvasMetrics) {
        if let Some(viewport) = self.primary_viewport_id() {
            self.set_viewport_metrics(viewport, metrics);
        }
    }
    pub fn canvas_metrics(&self) -> crate::CanvasMetrics {
        self.primary_viewport_id()
            .and_then(|viewport| self.viewport_metrics(viewport))
            .unwrap_or_default()
    }
    pub fn pan_by(&mut self, delta: crate::CanvasPoint) {
        if let Some(binding) = self.primary_viewport_binding() {
            let _ = self.pan_viewport(binding, delta);
        }
    }
}
