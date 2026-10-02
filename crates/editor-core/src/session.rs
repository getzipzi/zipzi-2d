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
        self.geometry = None;
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
                | EditorCommand::SelectObject(_)
                | EditorCommand::OpenBinding(_)
                | EditorCommand::Navigate(_)
                | EditorCommand::NavigateBack
        );
        let context_only = matches!(
            &command,
            EditorCommand::Select(_)
                | EditorCommand::SelectObjects(_)
                | EditorCommand::SelectObject(_)
                | EditorCommand::OpenBinding(_)
                | EditorCommand::Navigate(_)
                | EditorCommand::NavigateBack
                | EditorCommand::FocusParameter(_)
        );
        let invalidate =
            !(context_only || matches!(&command, EditorCommand::FinishEdit)) || had_pending;
        let result = match command {
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
        if selection {
            self.tools.clear();
        }
        if invalidate
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
                self.geometry = Some(if let Some(previous) = self.appearance_geometry.take() {
                    Arc::new(previous.with_appearance(&self.document, scene.clone()))
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
