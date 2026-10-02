//! Canvas interaction state machine. Object selection, component selection and
//! write targets are independent; a click never opens a model transaction.
use crate::control_edit::ControlEdit;
use crate::{
    CanvasPoint, CanvasScene, ComponentRef, ControlKind, Document, HitTarget, ModelPoint,
    ObjectKind, ObjectRef, ViewportTransform,
};
use anyhow::{Result, ensure};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolId {
    #[default]
    Select,
    Vertices,
    Deformers,
    Glue,
    Hand,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeformerVisibility {
    pub ancestors: bool,
    pub all: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameVersion {
    pub document: crate::DocumentId,
    pub scene: u64,
    pub viewport: u64,
    pub request: u64,
}
#[derive(Clone, Debug)]
pub struct CanvasFrame {
    pub version: FrameVersion,
    pub transform: ViewportTransform,
    pub geometry: Arc<CanvasScene>,
}
#[derive(Clone, Debug)]
pub struct OverlayVertex {
    pub id: u32,
    pub position: CanvasPoint,
    pub selected: bool,
    pub hovered: bool,
}
#[derive(Clone, Debug, Default)]
pub struct OverlaySnapshot {
    pub vertices: Vec<OverlayVertex>,
    pub lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub accent_lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub hover_lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub marquee: Option<(CanvasPoint, CanvasPoint)>,
    pub unavailable: Option<String>,
    pub target_label: Option<String>,
}
struct Press {
    point: CanvasPoint,
    transform: ViewportTransform,
    scene: Arc<CanvasScene>,
    target: Option<HitTarget>,
    shift: bool,
    selection: Vec<ObjectRef>,
    current: CanvasPoint,
    edit: Option<Vec<ControlEdit>>,
    last: Option<(ModelPoint, Option<f32>)>,
}
#[derive(Default)]
pub struct ToolController {
    pub active: ToolId,
    pub deformer_visibility: DeformerVisibility,
    components: Vec<ComponentRef>,
    hovered: Option<HitTarget>,
    press: Option<Press>,
}
impl ToolController {
    pub fn dragging(&self) -> bool {
        self.press.is_some()
    }
    pub fn clear(&mut self) {
        self.components.clear();
        self.hovered = None;
    }
    pub fn reconcile(&mut self, document: &Document) {
        let model = document.model();
        self.components.retain(|c| {
            document.context().selections.contains(&c.object)
                && match c.object.kind {
                    ObjectKind::Mesh => model.get_mesh(&c.object.id).is_some(),
                    ObjectKind::Deformer => model.get_transform(&c.object.id).is_some(),
                    _ => false,
                }
        });
        self.hovered = None;
    }
    pub fn cancel(&mut self, document: &mut Document) -> bool {
        let changed = self.press.take().is_some_and(|p| p.edit.is_some());
        if changed {
            document.cancel_edit();
        }
        self.hovered = None;
        changed
    }
    pub fn set_tool(&mut self, tool: ToolId, document: &mut Document) -> bool {
        let changed = self.cancel(document);
        self.active = tool;
        self.clear();
        changed
    }
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
                let hit = scene.hit(point, t, &selection, self.active, self.deformer_visibility);
                self.hovered = hit.clone();
                // An already selected surface is empty space in component mode:
                // its interior must permit a marquee without replacing the object set.
                let hit = match hit {
                    Some(HitTarget::Object(ref r))
                        if self.active == ToolId::Vertices
                            && r.kind == ObjectKind::Mesh
                            && selection.contains(r) =>
                    {
                        None
                    }
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
                    Some(HitTarget::Object(r)) => {
                        let mut selected = if shift { selection.clone() } else { Vec::new() };
                        if shift && selected.contains(r) {
                            selected.retain(|v| v != r);
                        } else {
                            selected.push(r.clone());
                        }
                        document.select_objects(selected)?;
                        self.components.clear();
                    }
                    _ => {}
                }
                if self.active != ToolId::Hand {
                    self.press = Some(Press {
                        point,
                        transform: t,
                        scene: scene.clone(),
                        target: hit,
                        shift,
                        selection,
                        current: point,
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
                    self.hovered = scene.hit(
                        point,
                        t,
                        &document.context().selections,
                        self.active,
                        self.deformer_visibility,
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
                } else if press.target.is_none() {
                    let rectangle = crate::picking::distance2(press.point, point) >= 9.;
                    if self.active == ToolId::Vertices && rectangle {
                        if !press.shift {
                            self.components.clear();
                        }
                        for r in &press.selection {
                            if let Some(m) = press.scene.mesh(&r.id).filter(|_| {
                                r.kind == ObjectKind::Mesh && press.scene.mesh_visible(&r.id)
                            }) {
                                for (id, p) in &m.vertices {
                                    if in_rect(press.transform.to_canvas(*p), press.point, point) {
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
                    } else if self.active == ToolId::Select
                        || self.active == ToolId::Deformers
                        || self.active == ToolId::Glue
                    {
                        let mut selection = if press.shift {
                            press.selection.clone()
                        } else {
                            Vec::new()
                        };
                        if rectangle {
                            for m in press.scene.meshes.iter() {
                                if press.scene.mesh_visible(&m.id)
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
    fn update(&mut self, document: &mut Document, point: CanvasPoint) -> Result<bool> {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid pointer position"
        );
        let Some(press) = self.press.as_mut() else {
            return Ok(false);
        };
        press.current = point;
        let Some(target) = press
            .target
            .as_ref()
            .filter(|h| !matches!(h, HitTarget::Object(_)))
        else {
            return Ok(false);
        };
        if press.edit.is_none() && crate::picking::distance2(press.point, point) < 9. {
            return Ok(false);
        }
        if press.edit.is_none() {
            let mut targets = vec![target.object().clone()];
            if matches!(target, HitTarget::Component(_)) {
                for c in &self.components {
                    if !targets.contains(&c.object) {
                        targets.push(c.object.clone());
                    }
                }
            }
            ensure!(
                !targets.iter().any(|r| press
                    .scene
                    .ancestors(r)
                    .iter()
                    .any(|parent| targets.contains(parent))),
                "Select controls from one deformation level at a time"
            );
            let edits = targets
                .iter()
                .map(|r| ControlEdit::prepare(document, &press.scene, r))
                .collect::<Result<Vec<_>>>()?;
            document.begin_control_edit(&edits)?;
            press.edit = Some(edits);
        }
        let edits = press.edit.as_ref().unwrap();
        let start = press.transform.to_model(press.point);
        let end = press.transform.to_model(point);
        let rotate = matches!(target, HitTarget::RotationHandle(_));
        let delta = if rotate {
            ModelPoint::default()
        } else {
            ModelPoint {
                x: end.x - start.x,
                y: end.y - start.y,
            }
        };
        let angle = if rotate {
            Some(edits[0].rotation_delta(start, end)?)
        } else {
            None
        };
        if press.last == Some((delta, angle)) {
            return Ok(false);
        }
        document.preview_control_edit(edits, &self.components, delta, angle)?;
        press.last = Some((delta, angle));
        Ok(true)
    }
    pub fn overlay(&self, document: &Document, frame: &CanvasFrame) -> OverlaySnapshot {
        let scene = &frame.geometry;
        let t = frame.transform;
        let selection = &document.context().selections;
        let mut out = OverlaySnapshot::default();
        for r in selection {
            if r.kind == ObjectKind::Mesh {
                if let Some(m) = scene.mesh(&r.id).filter(|_| scene.mesh_visible(&r.id)) {
                    out.accent_lines.extend(
                        m.boundary
                            .iter()
                            .map(|(a, b)| (t.to_canvas(*a), t.to_canvas(*b))),
                    );
                    if self.active == ToolId::Vertices {
                        out.lines.extend(m.edges.iter().filter_map(|&(a, b)| {
                            Some((
                                t.to_canvas(m.vertices.get(a)?.1),
                                t.to_canvas(m.vertices.get(b)?.1),
                            ))
                        }));
                        for (id, p) in &m.vertices {
                            let c = ComponentRef {
                                object: r.clone(),
                                id: *id,
                            };
                            out.vertices.push(OverlayVertex {
                                id: *id,
                                position: t.to_canvas(*p),
                                selected: self.components.contains(&c),
                                hovered: self.hovered == Some(HitTarget::Component(c)),
                            });
                        }
                        describe_edit(&mut out, document, scene, r);
                    }
                }
            } else if matches!(r.kind, ObjectKind::Part | ObjectKind::Offscreen) {
                let members = scene.member_meshes(r);
                for m in &members {
                    out.accent_lines.extend(
                        m.boundary
                            .iter()
                            .map(|(a, b)| (t.to_canvas(*a), t.to_canvas(*b))),
                    );
                }
                if let Some([x0, y0, x1, y1]) = crate::picking::bounds(
                    members
                        .iter()
                        .flat_map(|m| m.vertices.iter().map(|(_, p)| *p)),
                ) {
                    add_rect(
                        &mut out.accent_lines,
                        t.to_canvas(ModelPoint { x: x0, y: y0 }),
                        t.to_canvas(ModelPoint { x: x1, y: y1 }),
                    );
                }
            }
        }
        for control in &scene.controls {
            if !scene.control_visible(control, selection, self.active, self.deformer_visibility) {
                continue;
            }
            let points = scene.control_points(control, t);
            let selected = selection.contains(&control.object);
            let lines = if selected {
                &mut out.accent_lines
            } else {
                &mut out.lines
            };
            lines.extend(
                control
                    .edges
                    .iter()
                    .filter_map(|&(a, b)| Some((*points.get(a)?, *points.get(b)?))),
            );
            if selected && self.active == ToolId::Deformers
                || matches!(control.kind, ControlKind::Rotation | ControlKind::Glue)
            {
                for (i, p) in points.iter().enumerate() {
                    let c = ComponentRef {
                        object: control.object.clone(),
                        id: i as u32,
                    };
                    out.vertices.push(OverlayVertex {
                        id: i as u32,
                        position: *p,
                        selected: self.components.contains(&c),
                        hovered: self
                            .hovered
                            .as_ref()
                            .is_some_and(|h| h.object() == &control.object),
                    });
                }
            }
            if self
                .hovered
                .as_ref()
                .is_some_and(|h| h.object() == &control.object)
            {
                out.hover_lines.extend(
                    control
                        .edges
                        .iter()
                        .filter_map(|&(a, b)| Some((*points.get(a)?, *points.get(b)?))),
                );
            }
            if selected && self.active == ToolId::Deformers && control.kind != ControlKind::Glue {
                describe_edit(&mut out, document, scene, &control.object);
            }
        }
        if let Some(hit) = &self.hovered {
            if let Some(mesh) = scene
                .mesh(&hit.object().id)
                .filter(|_| hit.object().kind == ObjectKind::Mesh)
            {
                out.hover_lines.extend(
                    mesh.boundary
                        .iter()
                        .map(|(a, b)| (t.to_canvas(*a), t.to_canvas(*b))),
                );
            }
        }
        if let Some(p) = &self.press {
            if p.target.is_none() && crate::picking::distance2(p.point, p.current) >= 9. {
                out.marquee = Some((p.point, p.current));
            }
        }
        out
    }
}
fn describe_edit(
    out: &mut OverlaySnapshot,
    document: &Document,
    scene: &CanvasScene,
    r: &ObjectRef,
) {
    match ControlEdit::describe(document, scene, r) {
        Ok(label) => out.target_label = Some(label),
        Err(e) => out.unavailable = Some(e.to_string()),
    }
}
fn in_rect(p: CanvasPoint, a: CanvasPoint, b: CanvasPoint) -> bool {
    p.x >= a.x.min(b.x) && p.x <= a.x.max(b.x) && p.y >= a.y.min(b.y) && p.y <= a.y.max(b.y)
}
fn add_rect(lines: &mut Vec<(CanvasPoint, CanvasPoint)>, a: CanvasPoint, b: CanvasPoint) {
    let c = CanvasPoint { x: b.x, y: a.y };
    let d = CanvasPoint { x: a.x, y: b.y };
    lines.extend([(a, c), (c, b), (b, d), (d, a)]);
}
#[derive(Clone, Copy, Debug)]
pub enum CanvasInput {
    Down { point: CanvasPoint, shift: bool },
    Move { point: CanvasPoint, left_down: bool },
    Up { point: CanvasPoint },
    Cancel,
    Leave,
}
