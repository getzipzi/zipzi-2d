//! Pointer event dispatch, click selection, marquee selection, and edit completion.
use super::selection::{LassoGesture, SelectionOperation, SelectionRegion};
use super::{CanvasFrame, CanvasInput, Press, ToolController, ToolId};
use crate::{Document, HitTarget, ObjectKind};
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
        let locks = document.object_state_index();
        let scene = &frame.geometry;
        let t = frame.transform;
        match event {
            CanvasInput::Cancel => Ok(self.cancel(document)),
            CanvasInput::Leave => {
                if self.press.is_none() && self.hovered_binding == Some(frame.version.binding) {
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
                let selection: Vec<_> = document
                    .context()
                    .selections
                    .iter()
                    .filter(|object| scene.in_isolation_scope(object))
                    .cloned()
                    .collect();
                let hit = scene.hit_with_mode(
                    point,
                    t,
                    &selection,
                    self.active,
                    self.deformer_visibility,
                    self.warp_fine,
                    &locks,
                );
                self.hovered = hit.clone();
                self.hovered_binding = Some(frame.version.binding);
                let component_surface = matches!(&hit, Some(HitTarget::Object(r)) if selection.iter().any(|selected|
                    !locks.is_locked(selected) && ((selected.kind == ObjectKind::Mesh && selected == r)
                    || (selected.kind == ObjectKind::Deformer
                        && (selected == r || (r.kind == ObjectKind::Mesh && scene.ancestors(r).contains(selected)))))));
                // A selected surface permits component marquee selection without
                // replacing the shared object selection.
                let hit = match hit {
                    Some(HitTarget::Object(_)) if component_surface => None,
                    hit => hit,
                };
                match &hit {
                    Some(HitTarget::Component(c)) => {
                        if shift && self.components.contains(c) {
                            self.components.retain(|v| v != c);
                            return Ok(false);
                        }
                        if !self.components.contains(c) {
                            if !shift {
                                self.components.clear();
                            }
                            self.components.push(c.clone());
                        }
                    }
                    _ => {}
                }
                if self.active != ToolId::Hand {
                    let lasso = (self.active == ToolId::Lasso
                        && hit
                            .as_ref()
                            .is_none_or(|hit| matches!(hit, HitTarget::Object(_))))
                    .then(|| LassoGesture::new(point, shift, alt));
                    self.press = Some(Press {
                        version: frame.version,
                        point,
                        transform: t,
                        scene: scene.clone(),
                        target: hit,
                        component_surface,
                        shift,
                        lasso,
                        warp_fine: self.warp_fine,
                        selection,
                        current: point,
                        dragged: false,
                        edit: None,
                        last: None,
                    });
                }
                Ok(false)
            }
            CanvasInput::Move { point, left_down } => {
                if self.press.is_some() {
                    if !left_down {
                        return Ok(self.cancel(document));
                    }
                    self.update(document, point)
                } else {
                    self.hovered_binding = Some(frame.version.binding);
                    self.hovered = scene.hit_with_mode(
                        point,
                        t,
                        &document.context().selections,
                        self.active,
                        self.deformer_visibility,
                        self.warp_fine,
                        &locks,
                    );
                    Ok(false)
                }
            }
            CanvasInput::Up { point } => {
                if self.press.is_none() {
                    return Ok(false);
                }
                let changed = self.update(document, point)?;
                let mut press = self.press.take().unwrap();
                if let Some(lasso) = &mut press.lasso {
                    lasso.finish(point);
                }
                let region_target = matches!(&press.target, None | Some(HitTarget::Object(_)));
                if press.edit.is_some() {
                    document.finish_edit()?;
                } else if !press.dragged && press.target.is_none() && !press.component_surface {
                    // Both tools use the press hit: a true miss clears selection, even with modifiers.
                    document.select_objects(Vec::new())?;
                    self.clear();
                } else if press.dragged && region_target {
                    let (region, operation) = match &press.lasso {
                        Some(lasso) => (SelectionRegion::lasso(&lasso.points), lasso.operation),
                        None => (
                            Some(SelectionRegion::rectangle(press.point, point)),
                            SelectionOperation::from_modifiers(press.shift, false),
                        ),
                    };
                    if let Some(region) = region {
                        self.select_region(document, &press, &region, operation)?;
                    }
                } else if press.lasso.is_none() {
                    if let Some(HitTarget::Object(object)) = &press.target {
                        // Surface hits are candidates until release: dragging from
                        // an unselected surface must still allow a marquee.
                        let mut selection = if press.shift {
                            press.selection.clone()
                        } else {
                            Vec::new()
                        };
                        if press.shift && selection.contains(object) {
                            selection.retain(|r| r != object);
                        } else {
                            selection.push(object.clone());
                        }
                        document.select_objects(selection)?;
                        self.components.clear();
                    } else if region_target && !press.shift {
                        self.components.clear();
                    }
                }

                Ok(changed)
            }
        }
    }
}
