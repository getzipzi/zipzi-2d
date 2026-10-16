//! Editor integration and measured-layout reconciliation.
use super::*;

impl EditorView {
    pub(crate) fn switch_workspace(
        &mut self,
        workspace: WorkspaceKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.workspace() == workspace {
            return;
        }
        // Let the existing input blur rules finish before changing context.
        window.focus(&self.focus, cx);
        if let Some(dock) = &self.dock {
            for canvas in dock.viewport_surfaces() {
                let token = canvas.update(cx, |canvas, _| canvas.cancel_pointer());
                if let Some(token) = token {
                    self.editor.cancel_viewport_interaction(token);
                }
            }
            for panel in dock.document_panels.values() {
                panel.update(cx, |panel, _| panel.cancel_drag());
            }
        }
        if let Err(error) = self.editor.set_workspace(workspace) {
            self.edit_error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.sync_workspace_layout(window, cx);
        self.redraw(cx);
    }

    /// Mode can change through tab activation, closing a tab, or save completion.
    pub(crate) fn sync_workspace_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace = self.editor.workspace();
        if self
            .dock
            .as_ref()
            .is_none_or(|dock| dock.workspace == workspace)
        {
            return;
        }
        self.focused_viewport = None;
        self.input_router.menu_target.set(None);
        self.reset_viewport_images();
        self.animation_anchor = None;
        self.frozen_frames.clear();
        if let Some(dock) = &mut self.dock {
            if let Err(error) = dock.switch(workspace, window, cx) {
                self.edit_error = Some(error.to_string());
            }
        }
        self.save_layout_after_frame(window, cx);
        if let Err(error) = cx
            .global_mut::<settings::Settings>()
            .set_workspace(workspace)
        {
            self.edit_error = Some(error.to_string());
        }
    }

    pub(crate) fn reveal_keyforms(&mut self, window: &mut Window, cx: &mut App) {
        if self.editor.workspace() != WorkspaceKind::Modeling {
            return;
        }
        if let Some(dock) = &self.dock {
            dock.show(PanelKind::Keyforms, None, window, cx);
        }
    }
    pub(crate) fn init_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dock = DockWorkspace::new(
            self.editor.workspace(),
            cx.weak_entity(),
            self.viewport_slots,
            self.mesh_thumbnails.clone(),
            window,
            cx,
        );
        let errors = dock.errors.take().unwrap();
        cx.spawn(async move |this, cx| {
            while let Ok(error) = errors.recv().await {
                if this
                    .update(cx, |this, cx| {
                        this.edit_error = Some(error);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        if let Err(error) = dock.restore(window, cx) {
            self.message = error.to_string();
            dock.reset(window, cx);
        }
        self.ui_subscriptions.push(cx.subscribe_in(
            &dock.area,
            window,
            |this, area, event, window, cx| {
                if !matches!(event, DockEvent::LayoutChanged) {
                    return;
                }
                if this.sync_viewport_visibility(window, cx) {
                    this.redraw(cx);
                }
                this.sync_timeline_visibility(window, cx);
                let has_empty_dock = [
                    DockPlacement::Left,
                    DockPlacement::Right,
                    DockPlacement::Bottom,
                ]
                .into_iter()
                .any(|placement| {
                    area.read(cx)
                        .layout(placement)
                        .is_some_and(|tree| tree.panels().next().is_none())
                });
                if has_empty_dock {
                    // Dragging the last panel out should release the old dock's
                    // space just like closing that panel through the menu.
                    let area = area.clone();
                    window.defer(cx, move |window, cx| {
                        area.update(cx, |area, cx| prune_empty_docks(area, window, cx))
                    });
                    return;
                }
                let state = area.read(cx).dump(cx);
                let Some(dock) = &mut this.dock else {
                    return;
                };
                if !dock.displayed(PanelKind::AnimationTimeline, cx) {
                    dock.timeline
                        .update(cx, |timeline, cx| timeline.deactivate(window, cx));
                }
                for panel in dock.document_panels.values() {
                    panel.update(cx, |panel, _| panel.cancel_drag());
                }

                if dock.pending_layout_measurement {
                    return;
                }
                if state == dock.last_valid {
                    return;
                }
                if validate_layout(&state, dock.workspace).is_err() {
                    let previous = dock.last_valid.clone();
                    let _ = area.update(cx, |area, cx| area.load(previous, window, cx));
                    return;
                }
                dock.last_valid = state.clone();
                if let Err(error) = dock.writer.save(dock.workspace, state) {
                    this.edit_error = Some(error.to_string());
                }
                cx.notify();
            },
        ));
        dock.pending_layout_measurement = true;
        self.dock = Some(dock);
        self.save_layout_after_frame(window, cx);
    }
    fn save_layout_after_frame(&self, window: &Window, cx: &Context<Self>) {
        let Some(dock) = &self.dock else { return };
        let workspace = dock.workspace;
        let epoch = dock.layout_epoch;
        let owner = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = owner.update(cx, |view, cx| {
                let Some(dock) = &mut view.dock else { return };
                if dock.workspace != workspace || dock.layout_epoch != epoch {
                    return;
                }
                let state = dock.area.read(cx).dump(cx);
                if validate_layout(&state, workspace).is_err() {
                    let previous = dock.last_valid.clone();
                    dock.area
                        .update(cx, |area, cx| area.load(previous, window, cx))
                        .ok();
                    dock.pending_layout_measurement = false;
                    return;
                }
                dock.last_valid = state.clone();
                dock.pending_layout_measurement = false;
                if let Err(error) = dock.writer.save(dock.workspace, state) {
                    view.edit_error = Some(error.to_string());
                    cx.notify();
                }
            });
        });
    }
    pub(crate) fn reset_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dock) = &mut self.dock {
            dock.pending_layout_measurement = true;
            dock.layout_epoch += 1;
            dock.reset(window, cx);
            self.save_layout_after_frame(window, cx);
        }
        cx.notify();
    }
}
