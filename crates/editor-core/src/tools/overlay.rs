//! Overlay data and construction from the displayed scene and current gesture.
use super::{CanvasFrame, ToolController};
use crate::control_edit::{ControlChange, ControlEdit};
use crate::{
    CanvasPoint, CanvasScene, ComponentRef, ControlKind, Document, HitTarget, ModelPoint,
    ObjectKind, ObjectRef,
};
use std::collections::HashSet;

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
impl ToolController {
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
            if p.dragged && (p.target.is_none() || matches!(p.target, Some(HitTarget::Object(_)))) {
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
fn add_rect(lines: &mut Vec<(CanvasPoint, CanvasPoint)>, a: CanvasPoint, b: CanvasPoint) {
    let c = CanvasPoint { x: b.x, y: a.y };
    let d = CanvasPoint { x: a.x, y: b.y };
    lines.extend([(a, c), (c, b), (b, d), (d, a)]);
}
