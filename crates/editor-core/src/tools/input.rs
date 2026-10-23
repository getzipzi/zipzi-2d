//! Pointer normalization and gesture dispatch. Operation rules live with each gesture.
use super::{CanvasFrame, CanvasInput, ToolController};
use crate::Document;
use anyhow::{Result, ensure};

impl ToolController {
    pub fn input(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        event: CanvasInput,
    ) -> Result<bool> {
        let result = self.process(document, frame, event);
        if result.is_err() {
            self.cancel(document);
        }
        result
    }
    fn process(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        event: CanvasInput,
    ) -> Result<bool> {
        let scene = &frame.geometry;
        let t = frame.transform;
        match event {
            CanvasInput::Cancel => Ok(self.cancel(document)),
            CanvasInput::Leave => {
                if self.gesture.is_none() && self.hovered_binding == Some(frame.version.binding) {
                    self.hovered = None;
                    self.hovered_binding = None;
                }
                Ok(false)
            }
            CanvasInput::Down { point, shift, alt } => {
                ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer position"
                );
                self.cancel(document);
                self.begin_selection(document, frame, point, shift, alt);
                Ok(false)
            }
            CanvasInput::Move { point, left_down } => {
                if self.gesture.is_some() {
                    if !left_down {
                        return Ok(self.cancel(document));
                    }
                    self.gesture
                        .as_mut()
                        .unwrap()
                        .update(document, &self.components, point)
                } else {
                    self.hovered_binding = Some(frame.version.binding);
                    self.hovered = scene.hit_with_mode(
                        point,
                        t,
                        &document.context().selections,
                        self.active,
                        self.deformer_visibility,
                        self.warp_fine,
                        &document.object_state_index(),
                    );
                    Ok(false)
                }
            }
            CanvasInput::Up { point } => {
                if self.gesture.is_none() {
                    return Ok(false);
                }
                let gesture = self.gesture.as_mut().unwrap();
                let model_changed = gesture.update(document, &self.components, point)?;
                // Keep ownership until commit succeeds, so the error path can roll back.
                gesture.finish(document, &mut self.components)?;
                self.gesture = None;
                Ok(model_changed)
            }
        }
    }
}
