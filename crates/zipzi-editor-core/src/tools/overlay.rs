//! Overlay data and construction from the displayed scene and current gesture.
use super::{CanvasFrame, ToolController};
use crate::{
    ComponentRef, ControlKind, Document, HitTarget, ModelPoint, ObjectKind, ObjectRef,
    ViewportPoint,
};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
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
    WarpGrid,
    Warp,
    RotationCenter,
    RotationTip,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayDeformer {
    pub outline_only: bool,
    pub kind: ControlKind,
    pub lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub selected: bool,
    pub hovered: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayRotationBone {
    pub points: [ViewportPoint; 4],
    pub scale: f32,
    pub selected: bool,
    pub hovered: bool,
}
/// Glue endpoints describe a relationship, not draggable components.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayGluePoint {
    pub position: ViewportPoint,
    pub state: GluePointState,
    pub selected: bool,
    pub hovered: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverlaySnapshot {
    /// The operation owned by this viewport's captured gesture, independent of live keys.
    pub active_gesture: Option<CanvasGestureKind>,
    /// Actual selection semantics; captured operations keep their press-time value.
    pub selection_operation: Option<CanvasSelectionOperation>,
    pub selection_weights: Option<OverlaySelectionWeights>,
    /// A selection brush in viewport logical pixels; never a geometry edit.
    pub selection_brush: Option<OverlaySelectionBrush>,
    /// Name of the object beneath the pointer in this presented viewport.
    pub hovered_object_label: Option<String>,
    pub manual_mesh_feedback: Option<super::ManualMeshFeedback>,
    /// Referenced Glue endpoints can move and connect, but cannot be deleted.
    pub manual_mesh_delete_unavailable: bool,
    pub vertices: Vec<OverlayVertex>,
    /// Mesh topology; controls retain their own semantic line groups below.
    pub lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub automatic_lines: Vec<(ViewportPoint, ViewportPoint)>,
    /// Selected mesh and group boundaries.
    pub accent_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub hover_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub reference_lines: Vec<(ViewportPoint, ViewportPoint)>,
    /// Source image extent, distinct from editable mesh topology.
    pub image_bounds: Vec<(ViewportPoint, ViewportPoint)>,
    pub deformers: Vec<OverlayDeformer>,
    pub glues: Vec<OverlayGlue>,
    pub marquee: Option<(ViewportPoint, ViewportPoint)>,
    /// Sampled freehand path, including the current pointer position.
    pub lasso: Vec<ViewportPoint>,
    pub unavailable: Option<String>,
    /// Surface this restriction only after an edit attempt, not during pose previews.
    pub unavailable_on_edit_only: bool,
    pub target_label: Option<String>,
    /// Zero denotes the whole Warp; a positive count denotes the selected controls.
    pub warp_transform_points: Option<usize>,
    pub warp_frame_adjusting: bool,
    pub warp_bezier_mode: Option<zipzi_core::WarpBezierEditMode>,
    pub deformer_keep_children: bool,
    /// Control supplying the hovered or captured handle/component's orientation.
    pub active_control_kind: Option<ControlKind>,
    /// Screen-space frame orientation, or bone-axis direction for Rotation controls.
    pub active_handle_radians: f32,
    /// Context for a hovered control, separate from edit errors and parameter previews.
    pub point_unavailable: Option<String>,
    pub gizmo_lines: Vec<(ViewportPoint, ViewportPoint)>,
    /// Bezier curves remain a backdrop to the stronger tangent controls.
    pub bezier_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub bezier_tangents: Vec<(ViewportPoint, ViewportPoint)>,
    /// Outer Warp transform frame, distinct from its lattice and Bezier tangents.
    pub warp_frame_lines: Vec<(ViewportPoint, ViewportPoint)>,
    /// Selected bone outlines use a stronger stroke.
    pub rotation_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub rotation_bones: Vec<OverlayRotationBone>,
    pub rotation_length_adjusting: bool,
    pub guide_lines: Vec<(ViewportPoint, ViewportPoint)>,
    pub handles: Vec<OverlayHandle>,
    pub interaction_label: Option<String>,
    pub active_handle: Option<crate::DeformerHandle>,
    /// A component under the pointer, or captured by the current edit.
    pub active_component: Option<ComponentRef>,
    pub origin_crosshair: Option<ViewportPoint>,
}
/// Actual captured canvas operations. Rendering clients choose their own cursor artwork.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasGestureKind {
    BoxSelect,
    LassoSelect,
    BrushSelect,
    BrushResize,
    Move,
    Rotate,
    Scale,
    BoneLength,
    MeshAdd,
    MeshDelete,
    MeshConnect,
    MeshFixEdge,
    Unavailable,
    Cancelled,
}
/// Selection feedback shared by canvas tools, without widget or cursor dependencies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasSelectionOperation {
    Replace,
    Add,
    Remove,
    Toggle,
}
impl From<super::region::SelectionOperation> for CanvasSelectionOperation {
    fn from(operation: super::region::SelectionOperation) -> Self {
        match operation {
            super::region::SelectionOperation::Replace => Self::Replace,
            super::region::SelectionOperation::Add => Self::Add,
            super::region::SelectionOperation::Remove => Self::Remove,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlaySelectionBrush {
    pub position: ViewportPoint,
    pub radius: f32,
    pub subtract: bool,
    pub shape: super::BrushShape,
    pub angle: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OverlaySelectionWeights {
    pub color: [f32; 4],
    pub points: Vec<(ViewportPoint, f32, OverlayVertexKind)>,
    pub triangles: Vec<([ViewportPoint; 3], [f32; 3])>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayHandle {
    /// A Bezier tangent endpoint, distinct from transform-frame handles.
    pub tangent: bool,
    pub rotation: bool,
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
        if self.rotation_length_target().is_some() {
            return None;
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
                    && scene.control_visible(
                        c,
                        &selection,
                        self.active,
                        self.deformer_visibility_for(frame),
                    )
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
        if self.rotation_length_target() == Some(target.object()) {
            return document
                .ensure_editable(std::slice::from_ref(target.object()))
                .err()
                .map(|e| e.to_string());
        }
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
            return manual.overlay(frame, self.shift, self.alt);
        }
        #[cfg(test)]
        self.overlay_builds
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("overlay.total");
        let scene = &frame.geometry;
        let t = frame.transform;
        let warp_selection = self.warp_selection_gizmo(document, frame);
        let selection = self.overlay_selection(document, scene);
        let selection = selection.as_ref();
        let hovered_target = self
            .hovered
            .clone()
            .filter(|_| self.hovered_binding == Some(frame.version.binding));
        let mut out = OverlaySnapshot {
            active_gesture: self
                .gesture
                .as_ref()
                .filter(|g| g.press().version.binding == frame.version.binding)
                .map(super::gesture::Gesture::kind),
            ..Default::default()
        };
        out.selection_operation = if let Some(gesture) = self
            .gesture
            .as_ref()
            .filter(|g| g.press().version.binding == frame.version.binding)
        {
            gesture.selection_operation()
        } else if self.active == super::ToolId::BrushSelect {
            Some(super::region::SelectionOperation::from_modifiers(self.shift, self.alt).into())
        } else if let Some(HitTarget::Component(component)) = &hovered_target {
            let rotation_center = component.id == 0
                && scene
                    .control(&component.object.id)
                    .is_some_and(|control| control.kind == ControlKind::Rotation);
            if self.alt && !rotation_center {
                Some(CanvasSelectionOperation::Remove)
            } else if self.shift && self.components.contains(component) {
                Some(CanvasSelectionOperation::Toggle)
            } else {
                None
            }
        } else if self.active == super::ToolId::Lasso {
            let operation = super::region::SelectionOperation::from_modifiers(self.shift, self.alt);
            Some(
                if operation == super::region::SelectionOperation::Add
                    && matches!(&hovered_target, Some(HitTarget::Object(object)) if selection.contains(object))
                    && !Self::component_surface(
                        scene,
                        selection,
                        hovered_target.as_ref(),
                        &document.object_state_index(),
                    )
                {
                    CanvasSelectionOperation::Toggle
                } else {
                    operation.into()
                },
            )
        } else {
            None
        };
        out.hovered_object_label = hovered_target
            .as_ref()
            .filter(|hit| {
                let object = hit.object();
                scene.index.get(object).is_some()
                    && scene.in_isolation_scope(object)
                    && (object.kind != ObjectKind::Mesh || scene.mesh_visible(&object.id))
                    && scene.control(&object.id).is_none_or(|c| {
                        scene.control_visible(
                            c,
                            selection,
                            self.active,
                            self.deformer_visibility_for(frame),
                        )
                    })
            })
            .map(|hit| scene.index.name(hit.object()).to_owned());
        out.warp_frame_adjusting = self
            .gesture
            .as_ref()
            .is_some_and(|g| g.press().adjust_frame)
            || warp_selection
                .as_ref()
                .is_some_and(|g| !g.adjusting.is_empty());
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
        if active_target.is_some_and(|target| {
            scene
                .control(&target.object().id)
                .is_some_and(|control| control.kind == ControlKind::Warp)
        }) {
            let local = matches!(active_target, Some(HitTarget::Component(_)))
                || self
                    .gesture
                    .as_ref()
                    .map_or(warp_selection.as_ref().is_some_and(|g| g.local), |g| {
                        g.press().warp_selection.as_ref().is_some_and(|g| g.local)
                    });
            let components = self
                .gesture
                .as_ref()
                .map_or(self.components.as_slice(), |g| {
                    g.press().components.as_slice()
                });
            out.warp_transform_points = Some(if local {
                components
                    .iter()
                    .filter(|c| {
                        scene
                            .control(&c.object.id)
                            .is_some_and(|control| control.kind == ControlKind::Warp)
                    })
                    .count()
            } else {
                0
            });
            if local
                && (matches!(active_target, Some(HitTarget::Component(c))
                    if crate::warp_bezier::component(c.id).is_some())
                    || components
                        .iter()
                        .any(|c| crate::warp_bezier::component(c.id).is_some()))
            {
                out.warp_bezier_mode = active_target
                    .and_then(|hit| {
                        document
                            .model()
                            .transform(&hit.object().id)
                            .and_then(|t| t.warp_authoring.as_ref().map(|s| s.edit_mode))
                    })
                    .filter(|mode| *mode != zipzi_core::WarpBezierEditMode::Local);
            }
        }
        out.active_component = active_target.and_then(|hit| match hit {
            HitTarget::Component(component) => Some(component.clone()),
            _ => None,
        });
        out.deformer_keep_children = active_target.is_some_and(|target| {
            scene
                .control(&target.object().id)
                .is_some_and(|c| matches!(c.kind, ControlKind::Warp | ControlKind::Rotation))
        }) && self
            .gesture
            .as_ref()
            .is_some_and(|g| g.press().keep_children)
            && !out.warp_frame_adjusting;
        out.active_handle = active_target.and_then(|hit| match hit {
            HitTarget::DeformerHandle(_, handle) => Some(*handle),
            HitTarget::RotationHandle(_) => Some(crate::DeformerHandle::Rotate),
            _ => None,
        });
        out.rotation_length_adjusting = match self.gesture.as_ref() {
            Some(super::gesture::Gesture::Controls(g)) => g.adjusting_bone_length(),
            Some(_) => false,
            None => self.alt && out.active_handle == Some(crate::DeformerHandle::RotationScale),
        };
        let active_control = active_target.and_then(|hit| scene.control(&hit.object().id));
        out.active_control_kind = active_control.map(|control| control.kind);
        out.active_handle_radians = active_control
            .and_then(|control| scene.gizmo_for_selection(control, t, warp_selection.as_ref()))
            .map_or(0., |g| g.frame_angle);
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
                out.point_unavailable = if out.rotation_length_adjusting {
                    document
                        .ensure_editable(std::slice::from_ref(target.object()))
                        .err()
                        .map(|e| e.to_string())
                } else {
                    access
                        .object_restriction(target.object())
                        .map(str::to_owned)
                };
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
            if !scene.control_visible(
                control,
                selection,
                self.active,
                self.deformer_visibility_for(frame),
            ) {
                continue;
            }
            let selected = selection.contains(&control.object);
            let detail = selected
                || self.deformer_visibility.all
                || self.deformer_visibility.ancestors
                    && selection
                        .iter()
                        .any(|r| scene.ancestors(r).contains(&control.object));
            let points = if control.kind == ControlKind::Warp && !detail {
                Vec::new()
            } else {
                scene.control_points(control, t)
            };
            let hovered = control.kind != ControlKind::Rotation
                && hovered_target
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
                outline_only: control.kind == ControlKind::Warp && !detail,
                kind: control.kind,
                lines: Vec::new(),
                selected,
                hovered,
            };
            let adjusting_frame = self.active != super::ToolId::BrushSelect
                && control.kind == ControlKind::Warp
                && self
                    .warp_frame_state(document, frame.version.binding.viewport, &control.object)
                    .is_some_and(|state| state.info.adjusting);
            if let Some(bezier) = (selected && self.active != super::ToolId::BrushSelect)
                .then(|| scene.bezier(&control.object.id))
                .flatten()
            {
                out.bezier_lines.extend(
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
                        editable: !adjusting_frame && matches!(access.probe(&c), Some(Ok(()))),
                    });
                }
                for (id, a, b) in &bezier.handles {
                    let c = ComponentRef {
                        object: control.object.clone(),
                        id: *id,
                    };
                    out.bezier_tangents
                        .push((t.to_viewport(*a), t.to_viewport(*b)));
                    out.handles.push(OverlayHandle {
                        tangent: true,
                        rotation: false,
                        position: t.to_viewport(*b),
                        round: true,
                        editable: !adjusting_frame && matches!(access.probe(&c), Some(Ok(()))),
                        selected: access.component_selected(&c),
                        hovered: hovered_target == Some(HitTarget::Component(c.clone())),
                    });
                }
            }
            {
                if control.kind == ControlKind::Warp && !detail {
                    deformer.lines = scene.warp_outline(control, t);
                } else if control.kind != ControlKind::Rotation {
                    deformer.lines.extend(
                        control
                            .edges
                            .iter()
                            .filter_map(|&(a, b)| Some((*points.get(a)?, *points.get(b)?))),
                    );
                }
                if control.kind == ControlKind::Rotation && !selected {
                    if let Some(gizmo) = scene.deformer_gizmo(control, t) {
                        out.rotation_bones.push(OverlayRotationBone {
                            points: std::array::from_fn(|i| gizmo.outline[i].0),
                            scale: scene.rotation_screen_length(control, t)
                                / control.rotation_bone_length,
                            selected: false,
                            hovered: hovered_target
                                .as_ref()
                                .is_some_and(|hit| hit.object() == &control.object),
                        });
                    }
                }
                if selected {
                    for (i, p) in points.iter().enumerate() {
                        // The bone tip is the scale handle, not a second rotation component.
                        if control.kind == ControlKind::Rotation && i == 1 {
                            continue;
                        }
                        let c = ComponentRef {
                            object: control.object.clone(),
                            id: i as u32,
                        };
                        out.vertices.push(OverlayVertex {
                            id: i as u32,
                            kind: match (control.kind, i) {
                                (ControlKind::Rotation, 0) => OverlayVertexKind::RotationCenter,
                                (ControlKind::Rotation, _) => OverlayVertexKind::RotationTip,
                                _ => OverlayVertexKind::WarpGrid,
                            },
                            editable: !adjusting_frame && matches!(access.probe(&c), Some(Ok(()))),
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
                if let Some(gizmo) = (control.kind == ControlKind::Rotation
                    || !warp_selection
                        .as_ref()
                        .is_some_and(|s| !s.objects.is_empty()))
                .then(|| scene.gizmo_for_selection(control, t, warp_selection.as_ref()))
                .flatten()
                {
                    if control.kind == ControlKind::Rotation {
                        out.rotation_bones.push(OverlayRotationBone {
                            points: std::array::from_fn(|i| gizmo.outline[i].0),
                            scale: scene.rotation_screen_length(control, t)
                                / control.rotation_bone_length,
                            selected: true,
                            hovered: false,
                        });
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
                                    tangent: false,
                                    rotation: control.kind == ControlKind::Rotation,
                                    position,
                                    editable: if out.rotation_length_adjusting
                                        && control.kind == ControlKind::Rotation
                                    {
                                        document
                                            .ensure_editable(std::slice::from_ref(&control.object))
                                            .is_ok()
                                    } else {
                                        access.object_editable(&control.object)
                                    },
                                    round: matches!(kind, crate::DeformerHandle::Rotate),
                                    selected: !out.rotation_length_adjusting
                                        && self.gesture.as_ref().is_some_and(|p| {
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
        if let Some(frames) = warp_selection.as_ref().filter(|s| !s.objects.is_empty()) {
            for (object, gizmo) in &frames.items {
                let editable = frames.objects.iter().all(|o| access.object_editable(o));
                out.warp_frame_lines.extend(gizmo.outline.iter().copied());
                out.handles
                    .extend(gizmo.handles.iter().map(|(kind, position)| OverlayHandle {
                        tangent: false,
                        rotation: false,
                        position: *position,
                        editable,
                        round: matches!(kind, crate::DeformerHandle::Rotate),
                        selected: self.gesture.as_ref().is_some_and(|g| {
                            g.target() == Some(&HitTarget::DeformerHandle(object.clone(), *kind))
                        }),
                        hovered: hovered_target
                            == Some(HitTarget::DeformerHandle(object.clone(), *kind)),
                    }));
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
        if self.active == super::ToolId::BrushSelect {
            out.handles.clear();
            out.warp_frame_lines.clear();
            out.warp_frame_adjusting = false;
            out.warp_transform_points = None;
            out.warp_bezier_mode = None;
            out.selection_brush = self
                .pointer
                .filter(|(binding, _)| {
                    *binding == frame.version.binding
                        && !matches!(self.gesture, Some(super::gesture::Gesture::Controls(_)))
                })
                .map(|(_, position)| OverlaySelectionBrush {
                    position: self
                        .gesture
                        .as_ref()
                        .and_then(|g| match g {
                            super::gesture::Gesture::BrushSelect(g) if g.resizing => {
                                Some(g.press.point)
                            }
                            _ => None,
                        })
                        .unwrap_or(position),
                    radius: self.brush_selection.diameter / 2.,
                    shape: self.brush_selection.shape,
                    angle: self.brush_selection.angle,
                    subtract: self.gesture.as_ref().map_or(self.alt, |g| {
                        g.press().operation == super::region::SelectionOperation::Remove
                    }),
                });
        }
        if !self.component_weights.is_empty() {
            let mut weights = OverlaySelectionWeights {
                color: self.brush_selection.color,
                points: Vec::new(),
                triangles: Vec::new(),
            };
            let selected: std::collections::HashSet<_> = self.components.iter().collect();
            let weight = |object: &ObjectRef, id| {
                let c = ComponentRef {
                    object: object.clone(),
                    id,
                };
                if selected.contains(&c) {
                    self.component_weights.get(&c).copied().unwrap_or(1.)
                } else {
                    0.
                }
            };
            for object in selection {
                if object.kind == ObjectKind::Mesh && scene.mesh_visible(&object.id) {
                    if let Some(mesh) = scene.mesh(&object.id) {
                        let positions: std::collections::HashMap<_, _> = mesh
                            .vertices
                            .iter()
                            .map(|(id, p)| (*id, t.to_viewport(*p)))
                            .collect();
                        for &(id, model_position) in &mesh.vertices {
                            let position = t.to_viewport(model_position);
                            let w = weight(object, id);
                            if w > 0. {
                                weights.points.push((position, w, OverlayVertexKind::Mesh));
                            }
                        }
                        for ids in mesh.triangles.iter() {
                            let w = ids.map(|id| weight(object, id));
                            if w.iter().any(|w| *w > 0.) {
                                if let [Some(a), Some(b), Some(c)] =
                                    ids.map(|id| positions.get(&id).copied())
                                {
                                    weights.triangles.push(([a, b, c], w));
                                }
                            }
                        }
                    }
                } else if let Some(control) = scene
                    .control(&object.id)
                    .filter(|c| c.kind == ControlKind::Warp && c.enabled && c.visible)
                {
                    for (id, p) in control.points.iter().enumerate() {
                        let w = weight(object, id as u32);
                        if w > 0. {
                            weights.points.push((
                                t.to_viewport(*p),
                                w,
                                OverlayVertexKind::WarpGrid,
                            ));
                        }
                    }
                    if let Some((rows, columns, _)) = scene
                        .topology
                        .controls
                        .get(&object.id)
                        .and_then(|c| c.warp_shape)
                    {
                        let (rows, columns) = (rows as usize, columns as usize);
                        for y in 0..rows {
                            for x in 0..columns {
                                let a = y * (columns + 1) + x;
                                let b = a + 1;
                                let c = a + columns + 1;
                                let d = c + 1;
                                for ids in [[a, b, c], [b, d, c]] {
                                    let w = ids.map(|id| weight(object, id as u32));
                                    if w.iter().any(|w| *w > 0.) {
                                        weights.triangles.push((
                                            ids.map(|id| t.to_viewport(control.points[id])),
                                            w,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            out.selection_weights = Some(weights);
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
