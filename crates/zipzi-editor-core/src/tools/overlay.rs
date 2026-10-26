//! Overlay data and construction from the displayed scene and current gesture.
use super::{CanvasFrame, ToolController};
use crate::{
    ComponentRef, ControlKind, Document, HitTarget, ModelPoint, ObjectKind, ObjectRef,
    ViewportPoint,
};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct OverlayVertex {
    pub id: u32,
    pub position: ViewportPoint,
    pub selected: bool,
    pub hovered: bool,
    pub kind: OverlayVertexKind,
    pub editable: bool,
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
    pub lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub selected: bool,
    pub hovered: bool,
}
/// Glue endpoints describe a relationship, not draggable components.
#[derive(Clone, Debug)]
pub struct OverlayGlue {
    pub pairs: Vec<(ViewportPoint, ViewportPoint)>,
    pub points: Vec<OverlayGluePoint>,
    pub selected: bool,
    pub hovered: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GluePointState {
    Reference,
    Restricted,
    Editable,
}
#[derive(Clone, Debug)]
pub struct OverlayGluePoint {
    pub position: ViewportPoint,
    pub state: GluePointState,
    pub selected: bool,
    pub hovered: bool,
}
#[derive(Clone, Debug, Default)]
pub struct OverlaySnapshot {
    pub vertices: Vec<OverlayVertex>,
    /// Mesh topology; controls retain their own semantic line groups below.
    pub lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub automatic_lines: Vec<(ViewportPoint, ViewportPoint)>,
    /// Selected mesh and group boundaries.
    pub accent_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub hover_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub reference_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub deformers: Vec<OverlayDeformer>,
    pub glues: Vec<OverlayGlue>,
    pub marquee: Option<(ViewportPoint, ViewportPoint)>,
    /// Sampled freehand path, including the current pointer position.
    pub lasso: Vec<ViewportPoint>,
    pub unavailable: Option<String>,
    /// Surface this restriction only after an edit attempt, not during pose previews.
    pub unavailable_on_edit_only: bool,
    pub target_label: Option<String>,
    /// Context for a hovered control, separate from edit errors and parameter previews.
    pub point_unavailable: Option<String>,
    pub gizmo_lines: Vec<(ViewportPoint, ViewportPoint)>,
    /// Outer Warp transform frame, distinct from its lattice and Bezier tangents.
    pub warp_frame_lines: Vec<(ViewportPoint, ViewportPoint)>,
    /// Rotation rings and their scale-handle spokes use a stronger stroke.
    pub rotation_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub guide_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub handles: Vec<OverlayHandle>,
    pub interaction_label: Option<String>,
    pub active_handle: Option<crate::DeformerHandle>,
    /// A component under the pointer, or captured by the current edit.
    pub active_component: Option<ComponentRef>,
    pub origin_crosshair: Option<ViewportPoint>,
}
#[derive(Clone, Debug)]
pub struct OverlayHandle {
    pub position: ViewportPoint,
    pub round: bool,
    pub hovered: bool,
    pub selected: bool,
    pub editable: bool,
}
impl ToolController {
    fn overlay_selection<'a>(
        &self,
        document: &'a Document,
        scene: &crate::CanvasScene,
    ) -> std::borrow::Cow<'a, [ObjectRef]> {
        if scene.isolation_scope.is_some() {
            std::borrow::Cow::Owned(
                document
                    .context()
                    .selections
                    .iter()
                    .filter(|object| scene.in_isolation_scope(object))
                    .cloned()
                    .collect(),
            )
        } else {
            std::borrow::Cow::Borrowed(&document.context().selections)
        }
    }

    /// Status queries share the edit capabilities, without producing screen geometry.
    pub(crate) fn notice(&self, document: &Document, frame: &CanvasFrame) -> Option<String> {
        if let Some(manual) = &self.manual {
            return manual.diagnostic.clone();
        }
        let scene = &frame.geometry;
        let selection = self.overlay_selection(document, scene);
        if let Err(error) = document.ensure_editable(&selection) {
            return Some(error.to_string());
        }
        let access = self.edit_access(document, scene, &selection);
        let meshes = selection
            .iter()
            .filter(|r| r.kind == ObjectKind::Mesh && scene.mesh_visible(&r.id));
        let controls = scene
            .controls
            .iter()
            .filter(|c| {
                c.kind != ControlKind::Glue
                    && selection.contains(&c.object)
                    && scene.control_visible(c, &selection, self.active, self.deformer_visibility)
            })
            .map(|c| &c.object);
        meshes
            .chain(controls)
            .filter_map(|r| access.object(r))
            .filter_map(|object| {
                object
                    .restriction
                    .as_ref()
                    .map(|reason| (reason, object.on_edit_only))
            })
            .last()
            .filter(|(_, on_edit_only)| !on_edit_only)
            .map(|(reason, _)| reason.to_string())
    }

    pub(crate) fn hover_hint(&self, document: &Document, frame: &CanvasFrame) -> Option<String> {
        if self.hovered_binding != Some(frame.version.binding) {
            return None;
        }
        let target = self.hovered.as_ref()?;
        let selection = self.overlay_selection(document, &frame.geometry);
        let access = self.edit_access(document, &frame.geometry, &selection);
        match target {
            HitTarget::Component(component) => access.probe(component)?.err().map(str::to_owned),
            HitTarget::DeformerHandle(..) | HitTarget::RotationHandle(..) => access
                .object_restriction(target.object())
                .map(str::to_owned),
            _ => None,
        }
    }

    pub fn overlay(&self, document: &Document, frame: &CanvasFrame) -> OverlaySnapshot {
        if let Some(manual) = &self.manual {
            return manual.overlay(frame);
        }
        #[cfg(test)]
        self.overlay_builds
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("overlay.total");
        let scene = &frame.geometry;
        let t = frame.transform;
        let selection = self.overlay_selection(document, scene);
        let selection = selection.as_ref();
        let hovered_target = self
            .hovered
            .clone()
            .filter(|_| self.hovered_binding == Some(frame.version.binding));
        let mut out = OverlaySnapshot::default();
        if self.original_points {
            out.interaction_label =
                Some("Editing original points · Glue preview remains active".into());
        }
        let origin_pt = ModelPoint::default();
        out.origin_crosshair = Some(t.to_viewport(origin_pt));
        let active_target = self
            .gesture
            .as_ref()
            .filter(|press| press.press().version.binding == frame.version.binding)
            .and_then(|p| p.target())
            .or(hovered_target.as_ref());
        out.active_component = active_target.and_then(|hit| match hit {
            HitTarget::Component(component) => Some(component.clone()),
            _ => None,
        });
        out.active_handle = active_target.and_then(|hit| match hit {
            HitTarget::DeformerHandle(_, handle) => Some(*handle),
            HitTarget::RotationHandle(_) => Some(crate::DeformerHandle::Rotate),
            _ => None,
        });
        let access = self.edit_access(document, scene, selection);
        if let Some(component) = &out.active_component {
            if let Some(Err(reason)) = access.probe(component) {
                out.point_unavailable = Some(reason.into());
            }
        } else if let Some(target) = active_target {
            if matches!(
                target,
                HitTarget::DeformerHandle(..) | HitTarget::RotationHandle(..)
            ) {
                out.point_unavailable = access
                    .object_restriction(target.object())
                    .map(str::to_owned);
            }
        }
        for r in selection {
            #[cfg(feature = "bench-instrumentation")]
            let _span = zipzi_core::benchmark::span("overlay.selected_geometry");
            if r.kind == ObjectKind::Mesh {
                if let Some(m) = scene.mesh(&r.id).filter(|_| scene.mesh_visible(&r.id)) {
                    out.accent_lines.extend(
                        m.boundary
                            .iter()
                            .map(|(a, b)| (t.to_viewport(*a), t.to_viewport(*b))),
                    );
                    out.lines.extend(m.edges.iter().filter_map(|&(a, b)| {
                        Some((
                            t.to_viewport(m.vertices.get(a)?.1),
                            t.to_viewport(m.vertices.get(b)?.1),
                        ))
                    }));
                    let uniform = access.uniform_editable(r);
                    let object = access.object(r).unwrap();
                    let hovered_id = hovered_target.as_ref().and_then(|hit| match hit {
                        HitTarget::Component(c) if &c.object == r => Some(c.id),
                        _ => None,
                    });
                    for (id, p) in &m.vertices {
                        let editable = uniform.unwrap_or_else(|| {
                            matches!(
                                access.probe(&ComponentRef {
                                    object: r.clone(),
                                    id: *id
                                }),
                                Some(Ok(()))
                            )
                        });
                        out.vertices.push(OverlayVertex {
                            id: *id,
                            position: t.to_viewport(*p),
                            selected: object.selected_ids.contains(id),
                            hovered: hovered_id == Some(*id),
                            kind: OverlayVertexKind::Mesh,
                            editable,
                        });
                    }
                    describe_edit(&mut out, &access, r);
                }
            } else if matches!(r.kind, ObjectKind::Part | ObjectKind::Offscreen) {
                let members = scene.member_meshes(r);
                for m in &members {
                    out.accent_lines.extend(
                        m.boundary
                            .iter()
                            .map(|(a, b)| (t.to_viewport(*a), t.to_viewport(*b))),
                    );
                }
                if let Some([x0, y0, x1, y1]) = crate::picking::bounds(
                    members
                        .iter()
                        .flat_map(|m| m.vertices.iter().map(|(_, p)| *p)),
                ) {
                    add_rect(
                        &mut out.accent_lines,
                        t.to_viewport(ModelPoint { x: x0, y: y0 }),
                        t.to_viewport(ModelPoint { x: x1, y: y1 }),
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
            #[cfg(feature = "bench-instrumentation")]
            let _span = zipzi_core::benchmark::span("overlay.controls");
            if !scene.control_visible(control, selection, self.active, self.deformer_visibility) {
                continue;
            }
            let points = scene.control_points(control, t);
            let selected = selection.contains(&control.object);
            let hovered = hovered_target
                .as_ref()
                .is_some_and(|h| h.object() == &control.object);
            if control.kind == ControlKind::Glue {
                let mut glue_points = Vec::new();
                if let Some(g) = scene
                    .topology
                    .glues
                    .iter()
                    .find(|g| g.object == control.object)
                {
                    for &(a, b) in &g.pairs {
                        for (mesh, slot) in [(&g.mesh_a, a), (&g.mesh_b, b)] {
                            let Some((id, position)) =
                                scene.mesh(mesh).and_then(|m| m.vertices.get(slot))
                            else {
                                continue;
                            };
                            let component = ComponentRef {
                                object: ObjectRef::new(ObjectKind::Mesh, mesh),
                                id: *id,
                            };
                            let selected = access.component_selected(&component);
                            let hovered = out.active_component.as_ref() == Some(&component);
                            let restriction = access.probe(&component);
                            let state = match restriction {
                                None => GluePointState::Reference,
                                Some(Ok(())) => GluePointState::Editable,
                                Some(Err(reason)) => {
                                    if hovered {
                                        out.point_unavailable = Some(reason.into());
                                    }
                                    GluePointState::Restricted
                                }
                            };
                            glue_points.push(OverlayGluePoint {
                                position: t.to_viewport(*position),
                                state,
                                selected,
                                hovered,
                            });
                        }
                    }
                }
                out.glues.push(OverlayGlue {
                    points: glue_points,
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
                                            .map(|(a, b)| (t.to_viewport(*a), t.to_viewport(*b))),
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
                        .map(|(a, b)| (t.to_viewport(*a), t.to_viewport(*b))),
                );
                for (i, p) in bezier.anchors.iter().enumerate() {
                    let id = crate::warp_bezier::component_id(i, 0);
                    let c = ComponentRef {
                        object: control.object.clone(),
                        id,
                    };
                    out.vertices.push(OverlayVertex {
                        id,
                        position: t.to_viewport(*p),
                        selected: access.component_selected(&c),
                        hovered: hovered_target == Some(HitTarget::Component(c.clone())),
                        kind: OverlayVertexKind::Warp,
                        editable: matches!(access.probe(&c), Some(Ok(()))),
                    });
                }
                for (id, a, b) in &bezier.handles {
                    let c = ComponentRef {
                        object: control.object.clone(),
                        id: *id,
                    };
                    out.gizmo_lines.push((t.to_viewport(*a), t.to_viewport(*b)));
                    out.handles.push(OverlayHandle {
                        position: t.to_viewport(*b),
                        round: true,
                        editable: matches!(access.probe(&c), Some(Ok(()))),
                        selected: access.component_selected(&c),
                        hovered: hovered_target == Some(HitTarget::Component(c.clone())),
                    });
                }
                if self.deformer_grid {
                    out.guide_lines
                        .extend(control.edges.iter().filter_map(|&(a, b)| {
                            Some((
                                t.to_viewport(*control.points.get(a)?),
                                t.to_viewport(*control.points.get(b)?),
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
                            editable: matches!(access.probe(&c), Some(Ok(()))),
                            position: *p,
                            selected: access.component_selected(&c)
                                || (control.kind == ControlKind::Rotation
                                    && i == 1
                                    && self.gesture.as_ref().is_some_and(|p| {
                                        p.target()
                                            == Some(&HitTarget::RotationHandle(
                                                control.object.clone(),
                                            ))
                                    })),
                            hovered: hovered_target == Some(HitTarget::Component(c.clone()))
                                || (i == 1
                                    && hovered_target
                                        == Some(HitTarget::RotationHandle(control.object.clone()))),
                        });
                    }
                }
            }
            out.deformers.push(deformer);
            if selected {
                describe_edit(&mut out, &access, &control.object);
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
                    if control.kind == ControlKind::Rotation {
                        out.rotation_lines.extend(gizmo.outline);
                    } else {
                        out.warp_frame_lines.extend(gizmo.outline);
                    }
                    out.handles
                        .extend(
                            gizmo
                                .handles
                                .into_iter()
                                .map(|(kind, position)| OverlayHandle {
                                    position,
                                    editable: access.object_editable(&control.object),
                                    round: matches!(kind, crate::DeformerHandle::Rotate),
                                    selected: self.gesture.as_ref().is_some_and(|p| {
                                        p.target()
                                            == Some(&HitTarget::DeformerHandle(
                                                control.object.clone(),
                                                kind,
                                            ))
                                    }),
                                    hovered: hovered_target
                                        == Some(HitTarget::DeformerHandle(
                                            control.object.clone(),
                                            kind,
                                        )),
                                }),
                        );
                }
            }
        }
        if let Some(hit) = &hovered_target {
            if let Some(mesh) = scene
                .mesh(&hit.object().id)
                .filter(|_| hit.object().kind == ObjectKind::Mesh)
            {
                out.hover_lines.extend(
                    mesh.boundary
                        .iter()
                        .map(|(a, b)| (t.to_viewport(*a), t.to_viewport(*b))),
                );
            }
        }
        if let Some(p) = self
            .gesture
            .as_ref()
            .filter(|press| press.press().version.binding == frame.version.binding)
        {
            p.overlay(&mut out);
        }
        if let Err(error) = document.ensure_editable(selection) {
            out.unavailable = Some(error.to_string());
            out.unavailable_on_edit_only = false;
        }
        out
    }
}
fn describe_edit(out: &mut OverlaySnapshot, access: &super::access::EditAccess, r: &ObjectRef) {
    if let Some(object) = access.object(r) {
        if let Some(reason) = &object.restriction {
            out.unavailable = Some(reason.to_string());
            out.unavailable_on_edit_only = object.on_edit_only;
        } else if let Some(label) = &object.label {
            out.target_label = Some(label.clone());
        }
    }
}
fn add_rect(lines: &mut Vec<(ViewportPoint, ViewportPoint)>, a: ViewportPoint, b: ViewportPoint) {
    let c = ViewportPoint { x: b.x, y: a.y };
    let d = ViewportPoint { x: a.x, y: b.y };
    lines.extend([(a, c), (c, b), (b, d), (d, a)]);
}
