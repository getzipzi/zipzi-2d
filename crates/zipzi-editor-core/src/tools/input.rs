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
        self.reconcile_component_weights();
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
        if let Some(manual) = &mut self.manual {
            if let CanvasInput::Down { shift, alt, .. }
            | CanvasInput::ModifiersChanged { shift, alt } = event
            {
                self.shift = shift;
                self.alt = alt;
            }
            return manual.input(document, frame, event);
        }
        match event {
            CanvasInput::BrushTick { elapsed } => match &mut self.gesture {
                Some(super::gesture::Gesture::Inflate(g)) => g.tick(document, elapsed),
                _ => Ok(false),
            },
            CanvasInput::Cancel => Ok(self.cancel(document)),
            CanvasInput::ModifiersChanged { shift, alt } => {
                if !self.set_gesture_modifiers(shift, alt) {
                    return Ok(false);
                }
                if let Some(gesture) = self.gesture.as_mut() {
                    let point = gesture.press().current;
                    gesture.update(
                        document,
                        &mut self.components,
                        point,
                        &mut self.brush_selection,
                        &mut self.component_weights,
                    )
                } else {
                    Ok(false)
                }
            }
            CanvasInput::Leave => {
                if self.gesture.is_none() && self.hovered_binding == Some(frame.version.binding) {
                    self.hovered = None;
                    self.hovered_binding = None;
                    self.pointer = None;
                }
                Ok(false)
            }
            CanvasInput::Down {
                point,
                shift,
                alt,
                primary,
            } => {
                ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer position"
                );
                self.cancel(document);
                self.alt = alt;
                self.shift = shift;
                self.pointer = Some((frame.version.binding, point));
                self.begin_selection(document, frame, point, shift, alt, primary, false);
                Ok(false)
            }
            CanvasInput::BrushResizeDown { point } => {
                ensure!(
                    self.active == super::ToolId::BrushSelect && !self.manual_active(),
                    "Select the brush selection tool before resizing"
                );
                ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer position"
                );
                self.cancel(document);
                self.pointer = Some((frame.version.binding, point));
                self.begin_selection(document, frame, point, false, false, false, true);
                Ok(false)
            }
            CanvasInput::Move { point, left_down } => {
                ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer position"
                );
                self.pointer = Some((frame.version.binding, point));
                if self.gesture.is_some() {
                    if !left_down {
                        return Ok(self.cancel(document));
                    }
                    self.gesture.as_mut().unwrap().update(
                        document,
                        &mut self.components,
                        point,
                        &mut self.brush_selection,
                        &mut self.component_weights,
                    )
                } else {
                    self.hovered_binding = Some(frame.version.binding);
                    self.hovered = self.canvas_hit(document, frame, point);
                    Ok(false)
                }
            }
            CanvasInput::Up { point } => {
                if self.gesture.is_none() {
                    return Ok(false);
                }
                let gesture = self.gesture.as_mut().unwrap();
                self.pointer = Some((frame.version.binding, point));
                let model_changed = gesture.update(
                    document,
                    &mut self.components,
                    point,
                    &mut self.brush_selection,
                    &mut self.component_weights,
                )?;
                let frame_change = if let super::gesture::Gesture::Controls(g) = gesture {
                    g.frame_preview().map(|(o, i)| (o.clone(), i))
                } else {
                    None
                };
                // Keep ownership until commit succeeds, so the error path can roll back.
                let replace_selection = matches!(
                    gesture,
                    super::gesture::Gesture::Select(_) | super::gesture::Gesture::Lasso(_)
                ) && gesture.press().operation
                    == super::region::SelectionOperation::Replace;
                gesture.finish(document, &mut self.components, &mut self.component_weights)?;
                if replace_selection {
                    self.component_weights.clear();
                }
                self.reconcile_component_weights();
                self.gesture = None;
                if let Some((object, info)) = frame_change {
                    self.set_warp_frame(document, frame.version.binding.viewport, object, info);
                }
                self.prune_warp_frames(document);
                Ok(model_changed)
            }
        }
    }
}
