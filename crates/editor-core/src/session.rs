use crate::scene::{MAX_ZOOM, MIN_ZOOM};
use crate::{
    CanvasFrame, CanvasMetrics, CanvasPoint, CanvasScene, Document, EditorCommand, OverlaySnapshot,
    SceneSnapshot, ToolController, ViewState,
};
use anyhow::{Result, ensure};
use kasane_sdk::SaveReceipt;
use std::{path::Path, sync::Arc};

pub struct DocumentSession {
    document: Document,
    view: ViewState,
    pub scene_revision: u64,
    pub viewport_revision: u64,
    cache: Option<SceneSnapshot>,
    geometry: Option<Arc<CanvasScene>>,
    previous_geometry: Option<Arc<CanvasScene>>,
    appearance_geometry: Option<Arc<CanvasScene>>,
    pub displayed: Option<CanvasFrame>,
    pub tools: ToolController,
    pub evaluations: u64,
}
impl DocumentSession {
    pub fn from_document(document: Document) -> Self {
        Self {
            document,
            view: ViewState::default(),
            scene_revision: 1,
            viewport_revision: 1,
            cache: None,
            geometry: None,
            previous_geometry: None,
            appearance_geometry: None,
            displayed: None,
            tools: ToolController::default(),
            evaluations: 0,
        }
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn evaluated_frame(&self) -> Option<Arc<kasane_core::DrawableFrame>> {
        self.cache.as_ref().map(|scene| scene.frame.clone())
    }
    pub fn view(&self) -> ViewState {
        self.view
    }
    fn invalidate_scene(&mut self) {
        self.scene_revision += 1;
        self.cache = None;
        self.previous_geometry = self
            .geometry
            .take()
            .or_else(|| self.previous_geometry.take());
        self.appearance_geometry = None;
        self.tools.reconcile(&self.document);
    }
    pub fn cancel_gesture(&mut self) {
        if self.tools.cancel(&mut self.document) {
            self.invalidate_scene();
        }
    }
    pub fn execute(&mut self, command: EditorCommand) -> Result<()> {
        self.cancel_gesture();
        let reusable = if matches!(
            command,
            EditorCommand::PreviewOpacity(_) | EditorCommand::PreviewName(_)
        ) {
            self.geometry
                .clone()
                .or_else(|| self.appearance_geometry.clone())
        } else {
            None
        };
        let had_pending = self.document.has_pending_edit();
        let evaluation_before = self.document.evaluation_revision();
        let history_locks = matches!(&command, EditorCommand::Undo | EditorCommand::Redo)
            .then(|| self.document.explicit_locks().clone());
        let is_lock = matches!(&command, EditorCommand::SetObjectsLocked { .. });
        let lock_or_history = matches!(
            &command,
            EditorCommand::SetObjectsLocked { .. } | EditorCommand::Undo | EditorCommand::Redo
        );
        let preview_before = self.document.preview_revision();
        let is_preview = matches!(
            &command,
            EditorCommand::SetPreviewParameter { .. }
                | EditorCommand::SetPreviewParameters { .. }
                | EditorCommand::PreviewKeyform { .. }
        );
        let selection = matches!(
            &command,
            EditorCommand::Select(_)
                | EditorCommand::SelectObjects(_)
                | EditorCommand::SelectCanvasObjects(_)
                | EditorCommand::SelectObject(_)
                | EditorCommand::OpenBinding(_)
                | EditorCommand::Navigate(_)
                | EditorCommand::NavigateBack
        );
        let context_only = matches!(
            &command,
            EditorCommand::Select(_)
                | EditorCommand::SelectObjects(_)
                | EditorCommand::SelectCanvasObjects(_)
                | EditorCommand::SelectObject(_)
                | EditorCommand::OpenBinding(_)
                | EditorCommand::Navigate(_)
                | EditorCommand::NavigateBack
                | EditorCommand::FocusParameter(_)
        );
        let invalidate =
            !(context_only || matches!(&command, EditorCommand::FinishEdit)) || had_pending;
        let result = match command {
            EditorCommand::CreateParameter(draft) => {
                self.document.create_parameter(draft).map(|_| ())
            }
            EditorCommand::ReorderTree {
                deformer,
                source,
                target,
                position,
            } => self
                .document
                .reorder_tree(deformer, &source, &target, position),
            EditorCommand::MoveOrganization {
                source,
                destination,
            } => self.document.move_organization(&source, destination),
            EditorCommand::MoveParameter { id, destination } => {
                self.document.move_parameter(&id, destination)
            }
            EditorCommand::ReorderParameterGroup {
                source,
                target,
                position,
            } => self
                .document
                .reorder_parameter_group(&source, &target, position),
            EditorCommand::SetTool(tool) => {
                self.tools.set_tool(tool, &mut self.document);
                return Ok(());
            }
            EditorCommand::ToggleDeformers => {
                self.tools.deformer_visibility.all = !self.tools.deformer_visibility.all;
                return Ok(());
            }
            EditorCommand::ToggleAncestorDeformers => {
                self.tools.deformer_visibility.ancestors =
                    !self.tools.deformer_visibility.ancestors;
                return Ok(());
            }
            EditorCommand::SelectObjects(objects) => self.document.select_objects(objects),
            EditorCommand::SelectCanvasObjects(objects) => {
                self.document.select_canvas_objects(objects)
            }
            EditorCommand::SetObjectsLocked { objects, locked } => {
                self.document.set_objects_locked(&objects, locked)
            }
            EditorCommand::ToggleWarpFine => {
                self.tools.warp_fine = !self.tools.warp_fine;
                self.tools.clear();
                return Ok(());
            }
            EditorCommand::ToggleDeformerGrid => {
                self.tools.deformer_grid = !self.tools.deformer_grid;
                return Ok(());
            }
            EditorCommand::AdjustDeformer { object, adjustment } => {
                self.document.adjust_deformer(&object, adjustment)
            }
            EditorCommand::SetObjectProperty {
                object,
                field,
                value,
            } => self.document.set_object_property(&object, &field, value),
            EditorCommand::Select(id) => self.document.select(&id),
            EditorCommand::SelectObject(r) => self.document.select_object(r),
            EditorCommand::FocusParameter(id) => self.document.focus_parameter(&id),
            EditorCommand::OpenBinding(r) => self.document.open_binding(r),
            EditorCommand::Navigate(r) => self.document.navigate(r),
            EditorCommand::NavigateBack => self.document.navigate_back(),
            EditorCommand::PreviewKeyform { binding, point } => {
                self.document.preview_keyform(&binding, point)
            }
            EditorCommand::PreviewName(name) => self.document.preview_name(&name),
            EditorCommand::PreviewOpacity(v) => self.document.preview_opacity(v),
            EditorCommand::SetPreviewParameter {
                parameter_id,
                value,
            } => self.document.set_preview_parameter(&parameter_id, value),
            EditorCommand::SetPreviewParameters { values } => {
                self.document.set_preview_parameters(&values)
            }
            EditorCommand::SetVisible(v) => self.document.set_visible(v),
            EditorCommand::SetLayerVisible { layer_id, visible } => {
                self.document.set_layer_visible(&layer_id, visible)
            }
            EditorCommand::FinishEdit => self.document.finish_edit(),
            EditorCommand::CancelEdit => {
                self.document.cancel_edit();
                Ok(())
            }
            EditorCommand::Undo => self.document.undo(),
            EditorCommand::Redo => self.document.redo(),
        };
        if selection || lock_or_history {
            self.tools.clear();
        }
        // Editor lock edits are standalone transactions. Other metadata history
        // (for example names) must refresh labels captured in canvas topology.
        let lock_history =
            history_locks.is_some_and(|before| before != *self.document.explicit_locks());
        if invalidate
            && (!(is_lock || lock_history)
                || had_pending
                || self.document.evaluation_revision() != evaluation_before)
            && (!is_preview || had_pending || self.document.preview_revision() != preview_before)
        {
            self.invalidate_scene();
            if result.is_ok() {
                self.appearance_geometry = reusable;
            }
        }
        result
    }
    pub fn save(&mut self, path: &Path) -> Result<SaveReceipt> {
        self.document.save(path)
    }
    pub fn zoom_at(&mut self, delta: f32, focus: Option<CanvasPoint>, metrics: CanvasMetrics) {
        if !self.tools.dragging() && delta.is_finite() {
            let old_zoom = self.view.zoom;
            let new_zoom = (old_zoom + delta).clamp(MIN_ZOOM, MAX_ZOOM);
            if new_zoom != old_zoom {
                if metrics.visible() && old_zoom > 0. {
                    let k = new_zoom / old_zoom;
                    let anchor = focus.unwrap_or(CanvasPoint {
                        x: metrics.width / 2.0,
                        y: metrics.height / 2.0,
                    });
                    if anchor.x.is_finite() && anchor.y.is_finite() {
                        let vx = anchor.x - metrics.width / 2.0;
                        let vy = anchor.y - metrics.height / 2.0;
                        self.view.pan.x += (1.0 - k) * (vx - self.view.pan.x);
                        self.view.pan.y += (1.0 - k) * (vy - self.view.pan.y);
                    }
                }
                self.view.zoom = new_zoom;
                self.viewport_revision += 1;
            }
        }
    }
    pub fn pan_by(&mut self, delta: CanvasPoint) {
        if !self.tools.dragging()
            && delta.x.is_finite()
            && delta.y.is_finite()
            && (delta.x != 0. || delta.y != 0.)
        {
            self.view.pan.x += delta.x;
            self.view.pan.y += delta.y;
            self.viewport_revision += 1;
        }
    }
    pub fn reset_view(&mut self) {
        if !self.tools.dragging() {
            self.view = ViewState::default();
            self.viewport_revision += 1;
        }
    }
    pub fn scene(&mut self) -> Result<(SceneSnapshot, Arc<CanvasScene>)> {
        if self.cache.is_none() {
            let scene = self.document.snapshot()?;
            if self.geometry.is_none() {
                let previous = self.previous_geometry.take();
                self.geometry = Some(if let Some(previous) = self.appearance_geometry.take() {
                    Arc::new(previous.with_appearance(&self.document, scene.clone()))
                } else if let Some(previous) = previous {
                    Arc::new(previous.with_positions(&self.document, scene.clone()))
                } else {
                    self.document.geometry(&scene)
                });
            }
            self.cache = Some(scene);
            self.evaluations += 1;
        }
        Ok((
            self.cache.as_ref().unwrap().clone(),
            self.geometry.as_ref().unwrap().clone(),
        ))
    }
    pub fn overlay(&self) -> OverlaySnapshot {
        let Some(frame) = &self.displayed else {
            return OverlaySnapshot::default();
        };
        self.tools.overlay(&self.document, frame)
    }
    pub fn input(&mut self, event: crate::CanvasInput, _metrics: CanvasMetrics) -> Result<bool> {
        if matches!(event, crate::CanvasInput::Cancel) {
            let was = self.tools.dragging();
            self.cancel_gesture();
            return Ok(was);
        }
        let Some(frame) = self.displayed.clone() else {
            return Ok(false);
        };
        if matches!(event, crate::CanvasInput::Down { .. }) {
            ensure!(
                frame.version.scene == self.scene_revision
                    && frame.version.viewport == self.viewport_revision,
                "Wait for the current model frame"
            );
            self.document.finish_edit()?;
        }
        let result = self.tools.input(&mut self.document, &frame, event);
        if result.as_ref().is_ok_and(|changed| *changed) || result.is_err() {
            self.invalidate_scene();
        }
        result
    }
}
