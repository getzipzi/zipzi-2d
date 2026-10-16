//! Pointer event dispatch, click selection, marquee selection, and edit completion.
use super::{CanvasFrame, CanvasInput, Press, ToolController, ToolId};
use crate::{CanvasPoint, ComponentRef, ControlKind, Document, HitTarget, ObjectKind, ObjectRef};
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
                if self.press.is_none() {
                    self.hovered = None;
                }
                Ok(false)
            }
            CanvasInput::Down { point, shift } => {
                ensure!(
                    point.x.is_finite() && point.y.is_finite(),
                    "Invalid pointer position"
                );
                self.cancel(document);
                let selection = document.context().selections.clone();
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
                    self.press = Some(Press {
                        version: frame.version,
                        point,
                        transform: t,
                        scene: scene.clone(),
                        target: hit,
                        component_surface,
                        shift,
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
                let press = self.press.take().unwrap();
                if press.edit.is_some() {
                    document.finish_edit()?;
                } else if let Some(HitTarget::Object(object)) = &press.target
                    && !press.dragged
                {
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
                } else if press.target.is_none()
                    || matches!(press.target, Some(HitTarget::Object(_)))
                {
                    let rectangle = press.dragged;
                    if !rectangle && !press.component_surface {
                        // A true hit-test miss clears selection, even with Shift.
                        // Selected surfaces are retained for component selection.
                        document.select_objects(Vec::new())?;
                        self.clear();
                    } else if (rectangle || press.component_surface)
                        && press.selection.iter().any(|r| {
                            !locks.is_locked(r)
                                && match r.kind {
                                    ObjectKind::Mesh => press.scene.mesh_visible(&r.id),
                                    ObjectKind::Deformer => press
                                        .scene
                                        .controls
                                        .iter()
                                        .any(|c| c.object == *r && c.enabled && c.visible),
                                    _ => false,
                                }
                        })
                    {
                        if !press.shift {
                            self.components.clear();
                        }
                        if rectangle {
                            for c in &press.scene.controls {
                                if c.kind != ControlKind::Warp
                                    || locks.is_locked(&c.object)
                                    || !c.enabled
                                    || !c.visible
                                    || !press.selection.contains(&c.object)
                                {
                                    continue;
                                }
                                let points: Vec<_> = if let Some(bezier) = (!self.warp_fine)
                                    .then(|| press.scene.bezier(&c.object.id))
                                    .flatten()
                                {
                                    bezier
                                        .anchors
                                        .iter()
                                        .enumerate()
                                        .map(|(i, p)| (crate::warp_bezier::component_id(i, 0), *p))
                                        .collect()
                                } else {
                                    c.points
                                        .iter()
                                        .enumerate()
                                        .map(|(i, p)| (i as u32, *p))
                                        .collect()
                                };
                                for (id, p) in points {
                                    if in_rect(press.transform.to_canvas(p), press.point, point) {
                                        let component = ComponentRef {
                                            object: c.object.clone(),
                                            id,
                                        };
                                        if !self.components.contains(&component) {
                                            self.components.push(component);
                                        }
                                    }
                                }
                            }
                            for r in &press.selection {
                                if let Some(m) = press.scene.mesh(&r.id).filter(|_| {
                                    r.kind == ObjectKind::Mesh
                                        && !locks.is_locked(r)
                                        && press.scene.mesh_visible(&r.id)
                                }) {
                                    for (id, p) in &m.vertices {
                                        if in_rect(
                                            press.transform.to_canvas(*p),
                                            press.point,
                                            point,
                                        ) {
                                            let c = ComponentRef {
                                                object: r.clone(),
                                                id: *id,
                                            };
                                            if !self.components.contains(&c) {
                                                self.components.push(c);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if self.active == ToolId::Select || self.active == ToolId::Glue {
                        let mut selection = if press.shift {
                            press.selection.clone()
                        } else {
                            Vec::new()
                        };
                        if rectangle {
                            for m in press.scene.meshes.iter() {
                                if !locks.is_locked(&ObjectRef::new(ObjectKind::Mesh, &m.id))
                                    && press.scene.mesh_visible(&m.id)
                                    && !m.vertices.is_empty()
                                    && m.vertices.iter().all(|(_, p)| {
                                        in_rect(press.transform.to_canvas(*p), press.point, point)
                                    })
                                {
                                    let r = ObjectRef::new(ObjectKind::Mesh, &m.id);
                                    if !selection.contains(&r) {
                                        selection.push(r);
                                    }
                                }
                            }
                        }
                        document.select_objects(selection)?;
                        self.components.clear();
                    } else if !press.shift {
                        self.components.clear();
                    }
                }
                Ok(changed)
            }
        }
    }
}
fn in_rect(p: CanvasPoint, a: CanvasPoint, b: CanvasPoint) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}
