use crate::{
    CanvasFrame, CanvasMetrics, CanvasPoint, Document, EditorCommand, MeshGeometry,
    OverlaySnapshot, SceneSnapshot, ToolController, ViewState, ViewportTransform,
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
    geometry: Option<Arc<[MeshGeometry]>>,
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
            displayed: None,
            tools: ToolController::default(),
            evaluations: 0,
        }
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn view(&self) -> ViewState {
        self.view
    }
    fn invalidate_scene(&mut self) {
        self.scene_revision += 1;
        self.cache = None;
        self.geometry = None;
        self.tools.reconcile(&self.document);
    }
    pub fn cancel_gesture(&mut self) {
        if self.tools.cancel(&mut self.document) {
            self.invalidate_scene();
        }
    }
    pub fn execute(&mut self, command: EditorCommand) -> Result<()> {
        self.cancel_gesture();
        let unchanged_geometry = if matches!(
            &command,
            EditorCommand::PreviewOpacity(_) | EditorCommand::PreviewName(_)
        ) {
            self.geometry.clone()
        } else {
            None
        };
        let had_pending = self.document.has_pending_edit();
        let selection = matches!(&command, EditorCommand::Select(_));
        let invalidate = !matches!(
            &command,
            EditorCommand::Select(_) | EditorCommand::FinishEdit
        ) || had_pending;
        let result = match command {
            EditorCommand::Select(id) => self.document.select(&id),
            EditorCommand::PreviewName(name) => self.document.preview_name(&name),
            EditorCommand::PreviewOpacity(v) => self.document.preview_opacity(v),
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
        if invalidate {
            self.invalidate_scene();
            if result.is_ok() {
                self.geometry = unchanged_geometry;
            }
        }
        result
    }
    pub fn save(&mut self, path: &Path) -> Result<SaveReceipt> {
        self.document.save(path)
    }
    pub fn zoom_by(&mut self, delta: f32) {
        if !self.tools.dragging() && delta.is_finite() {
            let zoom = (self.view.zoom + delta).clamp(0.25, 3.);
            if zoom != self.view.zoom {
                self.view.zoom = zoom;
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
    pub fn scene(&mut self) -> Result<(SceneSnapshot, Arc<[MeshGeometry]>)> {
        if self.cache.is_none() {
            let scene = self.document.snapshot()?;
            if self.geometry.is_none() {
                self.geometry = Some(self.document.geometry(&scene));
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
        let geometry = frame
            .geometry
            .iter()
            .find(|g| Some(g.id.as_str()) == self.document.selected_id());
        self.tools.overlay(geometry, frame.transform)
    }
    pub fn input(&mut self, event: crate::CanvasInput, metrics: CanvasMetrics) -> Result<bool> {
        if matches!(event, crate::CanvasInput::Cancel) {
            let was = self.tools.dragging();
            self.cancel_gesture();
            return Ok(was);
        }
        let Some(frame) = self.displayed.clone() else {
            return Ok(false);
        };
        let Some(g) = frame
            .geometry
            .iter()
            .find(|g| Some(g.id.as_str()) == self.document.selected_id())
        else {
            return Ok(false);
        };
        let transform = ViewportTransform::new(self.document_canvas(), metrics, self.view)
            .unwrap_or(frame.transform);
        let result = (|| -> Result<bool> {
            Ok(match event {
                crate::CanvasInput::Down { point, shift } => {
                    ensure!(
                        frame.version.scene == self.scene_revision
                            && frame.version.viewport == self.viewport_revision,
                        "Wait for the current model frame"
                    );
                    self.document.finish_edit()?;
                    self.tools
                        .begin(&mut self.document, g, frame.transform, point, shift)?;
                    false
                }
                crate::CanvasInput::Move { point, left_down } => {
                    if self.tools.dragging() && !left_down {
                        self.cancel_gesture();
                        return Ok(true);
                    }
                    if self.tools.dragging() {
                        self.tools.update(&mut self.document, transform, point)?
                    } else {
                        self.tools.hover(g, frame.transform, point);
                        false
                    }
                }
                crate::CanvasInput::Up { point } => {
                    self.tools.finish(&mut self.document, transform, point)?
                }
                crate::CanvasInput::Cancel => unreachable!(),
            })
        })();
        let changed = match result {
            Ok(changed) => changed,
            Err(error) => {
                self.cancel_gesture();
                self.invalidate_scene();
                return Err(error);
            }
        };
        if changed {
            self.invalidate_scene();
        }
        Ok(changed)
    }
    fn document_canvas(&self) -> kasane_core::Canvas {
        self.document.canvas()
    }
}
