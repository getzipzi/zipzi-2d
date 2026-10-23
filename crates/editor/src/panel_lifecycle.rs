//! Window and dock ownership transitions for registered panel instances.
use crate::*;

impl EditorView {
    pub(crate) fn input_regions(
        &self,
        window: &Window,
        cx: &App,
    ) -> Vec<crate::input_routing::InputRegion> {
        let mut regions: Vec<_> = self
            .editor
            .viewport_ids()
            .into_iter()
            .filter(|id| self.viewport_displayed(*id, window, cx))
            .filter_map(|id| self.viewport_surface(id, cx))
            .map(|surface| surface.read(cx).input_region.clone())
            .collect();
        #[cfg(any(test, feature = "test-support"))]
        if self.viewport_scene.is_some() {
            return regions;
        }
        if let Some(dock) = &self.dock {
            regions.extend(dock.visible_input_regions(cx));
        }
        regions
    }

    pub(crate) fn all_input_regions(&self, cx: &App) -> Vec<crate::input_routing::InputRegion> {
        let mut regions: Vec<_> = self
            .editor
            .viewport_ids()
            .into_iter()
            .filter_map(|id| self.viewport_surface(id, cx))
            .map(|surface| surface.read(cx).input_region.clone())
            .collect();
        if let Some(dock) = &self.dock {
            regions.extend(dock.all_input_regions(cx));
        }
        regions
    }

    pub(super) fn suspend_input(
        &mut self,
        reason: editor_core::EditingLifecycle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // A dialog may already have ended editing before capturing its context
        // token. Releasing leftover UI must not invalidate that new token.
        let end =
            reason != editor_core::EditingLifecycle::BlockingUi || self.editor.editing_active();
        if reason == editor_core::EditingLifecycle::WindowDeactivated {
            self.input_router.deactivate();
        }
        self.cancel_input_captures(window, cx);
        if end {
            if let Some(document) = self.editor.active_id() {
                if let Err(error) = self.editor.end_editing(document, reason) {
                    self.edit_error = Some(error.to_string());
                }
            }
        }
        end
    }
    pub(super) fn connect_panel_lifecycle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ui_subscriptions
            .push(cx.observe_window_visibility(window, |this, _, window, cx| {
                if !window.is_visible() {
                    this.suspend_input(
                        editor_core::EditingLifecycle::WindowDeactivated,
                        window,
                        cx,
                    );
                    for viewport in this.editor.viewport_ids() {
                        this.editor
                            .set_viewport_metrics(viewport, Default::default());
                    }
                }
                this.sync_viewport_visibility(window, cx);
                this.redraw(cx);
            }));
        self.ui_subscriptions
            .push(cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.suspend_input(
                        editor_core::EditingLifecycle::WindowDeactivated,
                        window,
                        cx,
                    );
                    this.redraw(cx);
                }
            }));
    }
    pub(super) fn sync_viewport_visibility(&mut self, window: &Window, cx: &App) -> bool {
        let visible = self
            .dock
            .as_ref()
            .map(|dock| dock.visible_viewports(cx))
            .unwrap_or_default();
        #[cfg(any(test, feature = "test-support"))]
        let visible = self.viewport_scene.as_ref().map_or(visible, |scene| {
            scene.instances.iter().map(|(id, _)| *id).collect()
        });
        let editing = self.editor.gesture_active();
        let mut changed = false;
        for viewport in self.editor.viewport_ids() {
            let displayed = window.is_visible() && visible.contains(&viewport);
            let before = self.editor.viewport_binding(viewport);
            self.editor.set_viewport_visible(viewport, displayed);
            changed |= before != self.editor.viewport_binding(viewport);
            if !displayed {
                self.viewport_frames.remove(&viewport);
                if let Some(requests) = &self.requests {
                    requests.cancel(viewport);
                }
            }
        }
        changed || (editing && !self.editor.gesture_active())
    }

    pub(super) fn sync_viewports(&mut self) {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(scene) = &self.viewport_scene {
            for &(id, kind) in &scene.instances {
                self.editor
                    .register_viewport(id, kind)
                    .expect("fixed viewport kind");
            }
        }
        for (kind, viewport) in [
            editor_core::ViewportKind::Modeling,
            editor_core::ViewportKind::Animation,
        ]
        .into_iter()
        .zip(self.viewport_slots)
        {
            self.editor
                .register_primary_viewport(viewport, kind)
                .expect("fixed viewport kind");
        }
    }

    pub(crate) fn sync_timeline_visibility(&mut self, window: &Window, cx: &App) {
        if let Some(dock) = &self.dock {
            let visible = window.is_visible()
                && dock.displayed(editor_core::PanelKind::AnimationTimeline, cx);
            #[cfg(any(test, feature = "test-support"))]
            let visible = visible && self.viewport_scene.is_none();
            self.editor
                .set_timeline_visible(dock.timeline.read(cx).id, visible);
            let inspector_visible = window.is_visible()
                && dock.displayed(editor_core::PanelKind::Properties, cx)
                && crate::properties::target(&self.editor)
                    .is_some_and(|target| target.timeline.is_some());
            self.editor
                .set_timeline_visible(dock.properties.read(cx).timeline_id, inspector_visible);
        }
    }
}
