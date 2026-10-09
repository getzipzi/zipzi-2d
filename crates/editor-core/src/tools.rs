//! Canvas interaction state machine. Object selection, component selection and
//! write targets are independent; a click never opens a model transaction.
use crate::control_edit::{ControlChange, ControlEdit};
use crate::{
    CanvasPoint, CanvasScene, ComponentRef, ControlKind, Document, HitTarget, ModelPoint,
    ObjectKind, ObjectRef, ViewportTransform,
};
use anyhow::{Result, ensure};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolId {
    #[default]
    Select,
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
    pub kind: OverlayVertexKind,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OverlayVertexKind {
    #[default]
    Mesh,
    Warp,
    RotationCenter,
    RotationTip,
}
#[derive(Clone, Debug)]
pub struct OverlayDeformer {
    pub lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub selected: bool,
    pub hovered: bool,
}
/// Glue endpoints describe a relationship, not draggable components.
#[derive(Clone, Debug)]
pub struct OverlayGlue {
    pub pairs: Vec<(CanvasPoint, CanvasPoint)>,
    pub selected: bool,
    pub hovered: bool,
}
#[derive(Clone, Debug, Default)]
pub struct OverlaySnapshot {
    pub vertices: Vec<OverlayVertex>,
    /// Mesh topology; controls retain their own semantic line groups below.
    pub lines: Vec<(CanvasPoint, CanvasPoint)>,
    /// Selected mesh and group boundaries.
    pub accent_lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub hover_lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub reference_lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub deformers: Vec<OverlayDeformer>,
    pub glues: Vec<OverlayGlue>,
    pub marquee: Option<(CanvasPoint, CanvasPoint)>,
    pub unavailable: Option<String>,
    pub target_label: Option<String>,
    pub gizmo_lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub guide_lines: Vec<(CanvasPoint, CanvasPoint)>,
    pub handles: Vec<OverlayHandle>,
    pub interaction_label: Option<String>,
    pub active_handle: Option<crate::DeformerHandle>,
}
#[derive(Clone, Debug)]
pub struct OverlayHandle {
    pub position: CanvasPoint,
    pub round: bool,
    pub hovered: bool,
    pub selected: bool,
}
struct Press {
    version: FrameVersion,
    point: CanvasPoint,
    transform: ViewportTransform,
    scene: Arc<CanvasScene>,
    target: Option<HitTarget>,
    component_surface: bool,
    shift: bool,
    selection: Vec<ObjectRef>,
    current: CanvasPoint,
    edit: Option<Vec<ControlEdit>>,
    last: Option<ControlChange>,
}
#[derive(Default)]
pub struct ToolController {
    pub active: ToolId,
    pub deformer_visibility: DeformerVisibility,
    pub deformer_grid: bool,
    pub warp_fine: bool,
    components: Vec<ComponentRef>,
    hovered: Option<HitTarget>,
    press: Option<Press>,
}
impl ToolController {
    pub fn dragging(&self) -> bool {
        self.press.is_some()
    }
    /// Completed poses from this control edit can lag subsequent pointer moves.
    /// The press fixes the authoring target; new gestures still require a current frame.
    pub(crate) fn accepts_drag_frame(&self, version: FrameVersion) -> bool {
        self.press.as_ref().is_some_and(|press| {
            press.edit.is_some()
                && version.document == press.version.document
                && version.scene > press.version.scene
                && version.viewport == press.version.viewport
                && version.request > press.version.request
        })
    }
    pub fn clear(&mut self) {
        self.components.clear();
        self.hovered = None;
    }
    pub fn reconcile(&mut self, document: &Document) {
        let locks = document.object_state_index();
        let model = document.model();
        self.components.retain(|c| {
            !locks.is_locked(&c.object)
                && document.context().selections.contains(&c.object)
                && match c.object.kind {
                    ObjectKind::Mesh => model.get_mesh(&c.object.id).is_some(),
                    ObjectKind::Deformer => {
                        model.get_transform(&c.object.id).is_some()
                            && document.deformer_controls_visible(&c.object.id)
                    }
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
                        version: frame.version,
                        point,
                        transform: t,
                        scene: scene.clone(),
                        target: hit,
                        component_surface,
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
                } else if press.target.is_none() {
                    let rectangle = crate::picking::distance2(press.point, point) >= 9.;
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
            document.ensure_editable(&document.context().selections)?;
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
        let delta = ModelPoint {
            x: end.x - start.x,
            y: end.y - start.y,
        };
        let change = match target {
            HitTarget::RotationHandle(_) => ControlChange::Rotation {
                translation: ModelPoint::default(),
                angle: edits[0].rotation_delta(start, end)?,
                scale: 1.,
            },
            HitTarget::DeformerHandle(object, handle) => {
                use crate::DeformerHandle as H;
                let control = press
                    .scene
                    .controls
                    .iter()
                    .find(|c| &c.object == object)
                    .unwrap();
                let gizmo = press
                    .scene
                    .deformer_gizmo(control, press.transform)
                    .unwrap();
                match handle {
                    H::Move => ControlChange::MoveAll(delta),
                    H::Rotate => ControlChange::Rotation {
                        translation: ModelPoint::default(),
                        angle: edits[0].rotation_delta(start, end)?,
                        scale: 1.,
                    },
                    H::RotationScale => {
                        let center = press.transform.to_model(gizmo.center);
                        let before = (start.x - center.x).hypot(start.y - center.y);
                        let after = (end.x - center.x).hypot(end.y - center.y);
                        ensure!(before > 1e-6, "Rotation scale handle is unavailable");
                        ControlChange::Rotation {
                            translation: ModelPoint::default(),
                            angle: 0.,
                            scale: (after / before).max(0.0001),
                        }
                    }
                    H::Scale(index) => {
                        let (corner, anchor) = gizmo.scale_anchor(*index);
                        let ratio = |d: f32, span: f32| {
                            if span.abs() < 1e-5 {
                                1.
                            } else {
                                (1. + d / span).max(0.001)
                            }
                        };
                        let sx = ratio(point.x - press.point.x, corner.x - anchor.x);
                        let sy = ratio(point.y - press.point.y, corner.y - anchor.y);
                        let scale = if press.shift && index % 2 == 0 {
                            let s = if (sx - 1.).abs() > (sy - 1.).abs() {
                                sx
                            } else {
                                sy
                            };
                            [s, s]
                        } else {
                            [sx, sy]
                        };
                        ControlChange::Transform {
                            translation: ModelPoint::default(),
                            pivot: press.transform.to_model(anchor),
                            scale,
                            radians: 0.,
                        }
                    }
                }
            }
            _ => ControlChange::Translate(delta),
        };
        if press.last == Some(change) {
            return Ok(false);
        }
        document.preview_control_edit(edits, &self.components, change)?;
        press.last = Some(change);
        Ok(true)
    }
    pub fn overlay(&self, document: &Document, frame: &CanvasFrame) -> OverlaySnapshot {
        let locks = document.object_state_index();
        let scene = &frame.geometry;
        let t = frame.transform;
        let selection = &document.context().selections;
        let mut out = OverlaySnapshot::default();
        out.active_handle = self
            .press
            .as_ref()
            .and_then(|p| p.target.as_ref())
            .or(self.hovered.as_ref())
            .and_then(|hit| match hit {
                HitTarget::DeformerHandle(_, handle) => Some(*handle),
                HitTarget::RotationHandle(_) => Some(crate::DeformerHandle::Rotate),
                _ => None,
            });
        for r in selection {
            if r.kind == ObjectKind::Mesh {
                if let Some(m) = scene.mesh(&r.id).filter(|_| scene.mesh_visible(&r.id)) {
                    out.accent_lines.extend(
                        m.boundary
                            .iter()
                            .map(|(a, b)| (t.to_canvas(*a), t.to_canvas(*b))),
                    );
                    out.lines.extend(m.edges.iter().filter_map(|&(a, b)| {
                        Some((
                            t.to_canvas(m.vertices.get(a)?.1),
                            t.to_canvas(m.vertices.get(b)?.1),
                        ))
                    }));
                    if locks.is_locked(r) {
                        describe_edit(&mut out, document, scene, r);
                        continue;
                    }
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
                            kind: OverlayVertexKind::Mesh,
                        });
                    }
                    describe_edit(&mut out, document, scene, r);
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
        let mut outlined_meshes: HashSet<&str> = selection
            .iter()
            .filter(|r| r.kind == ObjectKind::Mesh)
            .map(|r| r.id.as_str())
            .collect();
        for control in &scene.controls {
            if !scene.control_visible(control, selection, self.active, self.deformer_visibility) {
                continue;
            }
            let points = scene.control_points(control, t);
            let selected = selection.contains(&control.object);
            if locks.is_locked(&control.object) {
                out.deformers.push(OverlayDeformer {
                    lines: control
                        .edges
                        .iter()
                        .filter_map(|&(a, b)| Some((*points.get(a)?, *points.get(b)?)))
                        .collect(),
                    selected,
                    hovered: false,
                });
                if selected {
                    describe_edit(&mut out, document, scene, &control.object);
                }
                continue;
            }
            let hovered = self
                .hovered
                .as_ref()
                .is_some_and(|h| h.object() == &control.object);
            if control.kind == ControlKind::Glue {
                out.glues.push(OverlayGlue {
                    pairs: control
                        .edges
                        .iter()
                        .filter_map(|&(a, b)| Some((*points.get(a)?, *points.get(b)?)))
                        .collect(),
                    selected,
                    hovered,
                });
                if selected || hovered {
                    if let Some(glue) = scene
                        .topology
                        .glues
                        .iter()
                        .find(|g| g.object == control.object)
                    {
                        for id in [&glue.mesh_a, &glue.mesh_b] {
                            // An explicitly selected mesh already has its own outline.
                            if outlined_meshes.insert(id.as_str()) {
                                if let Some(mesh) = scene.mesh(id) {
                                    out.reference_lines.extend(
                                        mesh.boundary
                                            .iter()
                                            .map(|(a, b)| (t.to_canvas(*a), t.to_canvas(*b))),
                                    );
                                }
                            }
                        }
                    }
                }
                continue;
            }
            let mut deformer = OverlayDeformer {
                lines: Vec::new(),
                selected,
                hovered,
            };
            if let Some(bezier) = (selected && !self.warp_fine)
                .then(|| scene.bezier(&control.object.id))
                .flatten()
            {
                deformer.lines.extend(
                    bezier
                        .curves
                        .iter()
                        .map(|(a, b)| (t.to_canvas(*a), t.to_canvas(*b))),
                );
                for (i, p) in bezier.anchors.iter().enumerate() {
                    let id = crate::warp_bezier::component_id(i, 0);
                    let c = ComponentRef {
                        object: control.object.clone(),
                        id,
                    };
                    out.vertices.push(OverlayVertex {
                        id,
                        position: t.to_canvas(*p),
                        selected: self.components.contains(&c),
                        hovered: self.hovered == Some(HitTarget::Component(c)),
                        kind: OverlayVertexKind::Warp,
                    });
                }
                for (id, a, b) in &bezier.handles {
                    let c = ComponentRef {
                        object: control.object.clone(),
                        id: *id,
                    };
                    out.gizmo_lines.push((t.to_canvas(*a), t.to_canvas(*b)));
                    out.handles.push(OverlayHandle {
                        position: t.to_canvas(*b),
                        round: true,
                        selected: self.components.contains(&c),
                        hovered: self.hovered == Some(HitTarget::Component(c)),
                    });
                }
                if self.deformer_grid {
                    out.guide_lines
                        .extend(control.edges.iter().filter_map(|&(a, b)| {
                            Some((
                                t.to_canvas(*control.points.get(a)?),
                                t.to_canvas(*control.points.get(b)?),
                            ))
                        }));
                }
            } else {
                deformer.lines.extend(
                    control
                        .edges
                        .iter()
                        .filter_map(|&(a, b)| Some((*points.get(a)?, *points.get(b)?))),
                );
                if selected {
                    for (i, p) in points.iter().enumerate() {
                        let c = ComponentRef {
                            object: control.object.clone(),
                            id: i as u32,
                        };
                        out.vertices.push(OverlayVertex {
                            id: i as u32,
                            kind: match (control.kind, i) {
                                (ControlKind::Rotation, 0) => OverlayVertexKind::RotationCenter,
                                (ControlKind::Rotation, _) => OverlayVertexKind::RotationTip,
                                _ => OverlayVertexKind::Warp,
                            },
                            position: *p,
                            selected: self.components.contains(&c)
                                || (control.kind == ControlKind::Rotation
                                    && i == 1
                                    && self.press.as_ref().is_some_and(|p| {
                                        p.target
                                            == Some(HitTarget::RotationHandle(
                                                control.object.clone(),
                                            ))
                                    })),
                            hovered: self.hovered == Some(HitTarget::Component(c))
                                || (i == 1
                                    && self.hovered
                                        == Some(HitTarget::RotationHandle(control.object.clone()))),
                        });
                    }
                }
            }
            out.deformers.push(deformer);
            if selected {
                describe_edit(&mut out, document, scene, &control.object);
                if self.deformer_grid && control.kind == ControlKind::Warp {
                    // Completed drag frames can retain a different lattice size.
                    // Sample their points using their own captured topology.
                    if let Some(shape) = scene
                        .topology
                        .controls
                        .get(&control.object.id)
                        .and_then(|t| t.warp_shape)
                    {
                        out.guide_lines
                            .extend(crate::deformer::sample_grid(control, shape, t));
                    }
                }
                if let Some(gizmo) = scene.deformer_gizmo(control, t) {
                    out.gizmo_lines.extend(gizmo.outline);
                    out.handles
                        .extend(
                            gizmo
                                .handles
                                .into_iter()
                                .map(|(kind, position)| OverlayHandle {
                                    position,
                                    round: matches!(kind, crate::DeformerHandle::Rotate),
                                    selected: self.press.as_ref().is_some_and(|p| {
                                        p.target
                                            == Some(HitTarget::DeformerHandle(
                                                control.object.clone(),
                                                kind,
                                            ))
                                    }),
                                    hovered: self.hovered
                                        == Some(HitTarget::DeformerHandle(
                                            control.object.clone(),
                                            kind,
                                        )),
                                }),
                        );
                }
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
            out.interaction_label = p.last.map(|change| match change {
                ControlChange::Translate(d) | ControlChange::MoveAll(d) => {
                    format!("ΔX {:.2}  ΔY {:.2}", d.x, d.y)
                }
                ControlChange::Transform { scale, radians, .. } if radians == 0. => {
                    format!("X {:.1}%  Y {:.1}%", scale[0] * 100., scale[1] * 100.)
                }
                ControlChange::Transform { radians, .. } => {
                    format!("{:.1}°", -radians.to_degrees())
                }
                ControlChange::Rotation { angle, scale, .. } => {
                    let angle = if p.scene.source.viewport_frame.canvas.flag & 1 == 0 {
                        -angle
                    } else {
                        angle
                    };
                    format!("Δ {:.1}°  × {:.3}", angle, scale)
                }
            });
        }
        if let Err(error) = document.ensure_editable(selection) {
            out.vertices.clear();
            out.handles.clear();
            out.gizmo_lines.clear();
            out.active_handle = None;
            out.unavailable = Some(error.to_string());
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
