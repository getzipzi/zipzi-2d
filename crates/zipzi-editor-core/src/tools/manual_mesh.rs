//! One document-scoped mesh mode, shared by all Modeling viewports.
use super::{
    CanvasFrame, CanvasInput, FrameVersion, OverlaySnapshot, OverlayVertex, OverlayVertexKind,
};
use crate::{Document, ManualMeshTarget, ViewportPoint, ViewportTransform};
use anyhow::Result;
use std::{cell::RefCell, collections::HashMap, sync::Arc};
use zipzi_core::{Vec2, VertexId};
use zipzi_sdk::{ManualMeshOperation, ManualVertexIdAllocator, ManualVertexTarget, MeshGeometry};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ManualTool {
    #[default]
    Select,
    Create,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Vertex(VertexId),
    Edge([VertexId; 2], EdgeHitKind),
    Empty,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EdgeHitKind {
    Boundary,
    Constraint,
    Automatic,
}

enum GestureKind {
    Drag(Vec<ManualVertexTarget>),
    Marquee { shift: bool, alt: bool },
    Create(Option<VertexId>),
    Delete(Hit),
}

struct Gesture {
    version: FrameVersion,
    transform: ViewportTransform,
    start: ViewportPoint,
    current: ViewportPoint,
    kind: GestureKind,
    moved: bool,
}

type ScreenLine = (ViewportPoint, ViewportPoint);
#[derive(Clone)]
struct ProjectedEdges {
    scene_revision: u64,
    topology: usize,
    mesh: usize,
    transform: ViewportTransform,
    structural: Arc<[ScreenLine]>,
    automatic: Arc<[ScreenLine]>,
    truncated: bool,
}

pub(super) struct ManualMeshMode {
    pub target: ManualMeshTarget,
    pub tool: ManualTool,
    pub selected: Vec<VertexId>,
    pub selected_edge: Option<[VertexId; 2]>,
    pub diagnostic: Option<String>,
    hover: Hit,
    gesture: Option<Gesture>,
    allocator: ManualVertexIdAllocator,
    pub preview_result: bool,
    projected: RefCell<HashMap<crate::ViewportId, ProjectedEdges>>,
}

impl ManualMeshMode {
    pub fn target_matches(&self, document: &Document) -> bool {
        document.validate_manual_target(&self.target).is_ok()
    }

    pub(super) fn refresh_source(&mut self, document: &Document) -> Result<()> {
        self.target.source = document.manual_mesh_target(true)?.source;
        Ok(())
    }
    pub fn new(target: ManualMeshTarget, document: &Document) -> Self {
        let mesh = document
            .model()
            .get_mesh(&target.object.id)
            .unwrap()
            .clone();
        let allocator = ManualVertexIdAllocator::new(&MeshGeometry {
            vertex_ids: mesh.vertex_ids,
            positions: mesh.base_positions,
            uvs: mesh.uvs,
            triangles: mesh.triangles,
            meshing: mesh.meshing,
        });
        Self {
            target,
            tool: ManualTool::Select,
            selected: Vec::new(),
            selected_edge: None,
            diagnostic: None,
            hover: Hit::Empty,
            gesture: None,
            allocator,
            preview_result: false,
            projected: RefCell::new(HashMap::new()),
        }
    }

    pub fn dragging(&self) -> bool {
        self.gesture.is_some()
    }

    pub fn viewport(&self) -> Option<crate::ViewportId> {
        self.gesture
            .as_ref()
            .map(|gesture| gesture.version.binding.viewport)
    }

    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        self.gesture.as_ref().is_some_and(|gesture| {
            version.binding.document == gesture.version.binding.document
                && version.scene >= gesture.version.scene
        })
    }

    pub fn cancel(&mut self, document: &mut Document) -> bool {
        let active = self.gesture.take().is_some();
        let changed = document.cancel_manual_mesh_gesture();
        self.diagnostic = None;
        active || changed
    }
    pub fn settle(&mut self, document: &mut Document) -> Result<bool> {
        let active = self.gesture.take().is_some();
        let changed = document.has_pending_edit();
        document.finish_manual_mesh_gesture()?;
        if changed {
            self.refresh_source(document)?;
        }
        Ok(active || changed)
    }

    pub fn select_all(&mut self, document: &Document) {
        if let Some(mesh) = document.model().get_mesh(&self.target.object.id) {
            self.selected = mesh.vertex_ids.clone();
            self.selected_edge = None;
        }
    }

    pub fn delete_selection(&mut self, document: &mut Document) -> Result<bool> {
        let operation = if !self.selected.is_empty() {
            Some(ManualMeshOperation::DeleteVertices {
                ids: self.selected.clone(),
            })
        } else {
            self.selected_edge.and_then(|[a, b]| {
                let model = document.model();
                let meshing = model.get_mesh(&self.target.object.id)?.meshing.as_ref()?;
                meshing
                    .constraints
                    .iter()
                    .any(|constraint| constraint.endpoints == [a, b])
                    .then_some(ManualMeshOperation::ReleaseConstraint { a, b })
            })
        };
        let Some(operation) = operation else {
            return Ok(false);
        };
        let changed = self.apply_once(document, operation)?;
        if changed {
            self.selected.clear();
            self.selected_edge = None;
        }
        Ok(changed)
    }

    fn hit(&self, frame: &CanvasFrame, point: ViewportPoint) -> Hit {
        let Some(mesh) = frame.geometry.mesh(&self.target.object.id) else {
            return Hit::Empty;
        };
        let Some(topology) = frame.geometry.topology.meshes.get(&self.target.object.id) else {
            return Hit::Empty;
        };
        let project = |slot: usize| frame.transform.to_viewport(mesh.vertices[slot].1);
        if let Some((id, _)) = mesh
            .vertices
            .iter()
            .enumerate()
            .map(|(slot, &(id, _))| (id, crate::picking::distance2(point, project(slot))))
            .filter(|(_, distance)| *distance <= 49.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            return Hit::Vertex(id);
        }
        for (edges, kind) in [
            (topology.boundary.as_slice(), EdgeHitKind::Boundary),
            (topology.user_edges.as_ref(), EdgeHitKind::Constraint),
            (topology.support_edges.as_ref(), EdgeHitKind::Constraint),
            (topology.automatic_edges.as_ref(), EdgeHitKind::Automatic),
        ] {
            if let Some(((a, b), _)) = edges
                .iter()
                .copied()
                .map(|(a, b)| {
                    (
                        (a, b),
                        crate::picking::segment_distance2(point, project(a), project(b)),
                    )
                })
                .filter(|(_, distance)| *distance <= 25.)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            {
                let ids = [mesh.vertices[a].0, mesh.vertices[b].0];
                return Hit::Edge([ids[0].min(ids[1]), ids[0].max(ids[1])], kind);
            }
        }
        Hit::Empty
    }

    fn selected_starts(&self, document: &Document) -> Vec<ManualVertexTarget> {
        let model = document.model();
        let Some(mesh) = model.get_mesh(&self.target.object.id) else {
            return Vec::new();
        };
        mesh.vertex_ids
            .iter()
            .zip(&mesh.uvs)
            .filter(|(id, _)| self.selected.contains(id))
            .map(|(&id, &uv)| ManualVertexTarget { id, uv })
            .collect()
    }

    pub fn input(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
        event: CanvasInput,
    ) -> Result<bool> {
        match event {
            CanvasInput::Cancel => Ok(self.cancel(document)),
            CanvasInput::Leave => {
                self.hover = Hit::Empty;
                Ok(false)
            }
            CanvasInput::Down { point, shift, alt } => {
                self.cancel(document);
                self.hover = self.hit(frame, point);
                let kind = match self.tool {
                    ManualTool::Select => match self.hover {
                        Hit::Vertex(id) => {
                            if alt {
                                self.selected.retain(|selected| *selected != id);
                            } else if shift {
                                if !self.selected.contains(&id) {
                                    self.selected.push(id);
                                }
                            } else if !self.selected.contains(&id) {
                                self.selected = vec![id];
                            }
                            self.selected_edge = None;
                            GestureKind::Drag(self.selected_starts(document))
                        }
                        Hit::Edge(ids, _) => {
                            if !shift && !alt {
                                self.selected.clear();
                            }
                            self.selected_edge = Some(ids);
                            GestureKind::Marquee { shift, alt }
                        }
                        _ => GestureKind::Marquee { shift, alt },
                    },
                    ManualTool::Create => GestureKind::Create(match self.hover {
                        Hit::Vertex(id) => Some(id),
                        _ => None,
                    }),
                    ManualTool::Delete => GestureKind::Delete(self.hover),
                };
                self.gesture = Some(Gesture {
                    version: frame.version,
                    transform: frame.transform,
                    start: point,
                    current: point,
                    kind,
                    moved: false,
                });
                Ok(false)
            }
            CanvasInput::Move { point, left_down } => {
                if self.gesture.is_none() {
                    self.hover = self.hit(frame, point);
                    return Ok(false);
                }
                if !left_down {
                    return Ok(self.cancel(document));
                }
                self.update_drag(document, point)
            }
            CanvasInput::Up { point } => {
                let Some(gesture) = self.gesture.as_ref() else {
                    return Ok(false);
                };
                let drag = matches!(gesture.kind, GestureKind::Drag(_));
                let mut changed = if drag {
                    self.update_drag(document, point)?
                } else {
                    false
                };
                let gesture = self.gesture.take().unwrap();
                match gesture.kind {
                    GestureKind::Drag(_) => {
                        let had_candidate = document.has_pending_edit();
                        document.finish_manual_mesh_gesture()?;
                        changed |= had_candidate;
                        if had_candidate {
                            self.refresh_source(document)?;
                        }
                    }
                    GestureKind::Marquee { shift, alt } => {
                        if gesture.moved {
                            let Some(mesh) = frame.geometry.mesh(&self.target.object.id) else {
                                return Ok(false);
                            };
                            let min = ViewportPoint {
                                x: gesture.start.x.min(point.x),
                                y: gesture.start.y.min(point.y),
                            };
                            let max = ViewportPoint {
                                x: gesture.start.x.max(point.x),
                                y: gesture.start.y.max(point.y),
                            };
                            let ids: Vec<_> = mesh
                                .vertices
                                .iter()
                                .filter_map(|&(id, position)| {
                                    let p = gesture.transform.to_viewport(position);
                                    (p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y)
                                        .then_some(id)
                                })
                                .collect();
                            if alt {
                                self.selected.retain(|id| !ids.contains(id));
                            } else if shift {
                                for id in ids {
                                    if !self.selected.contains(&id) {
                                        self.selected.push(id);
                                    }
                                }
                            } else {
                                self.selected = ids;
                            }
                            self.selected_edge = None;
                        } else if gesture.start == point && self.hover == Hit::Empty && !shift {
                            self.selected.clear();
                            self.selected_edge = None;
                        }
                    }
                    GestureKind::Create(from) => {
                        if let Some(a) = from {
                            if let Hit::Vertex(b) = self.hit(frame, point) {
                                if a != b {
                                    changed = self.apply_once(
                                        document,
                                        ManualMeshOperation::ConstrainEdge { a, b },
                                    )?;
                                }
                            }
                        } else {
                            let position = gesture.transform.to_model(gesture.start);
                            let uv = self.target.mapping.uv(Vec2::new(position.x, position.y))?;
                            let mesh = document
                                .model()
                                .get_mesh(&self.target.object.id)
                                .unwrap()
                                .clone();
                            let geometry = MeshGeometry {
                                vertex_ids: mesh.vertex_ids,
                                positions: mesh.base_positions,
                                uvs: mesh.uvs,
                                triangles: mesh.triangles,
                                meshing: mesh.meshing,
                            };
                            let id = self.allocator.allocate(&geometry)?;
                            changed = self.apply_once(
                                document,
                                ManualMeshOperation::InsertVertex { id, uv },
                            )?;
                            if changed {
                                self.selected = vec![id];
                                self.selected_edge = None;
                            }
                        }
                    }
                    GestureKind::Delete(hit) => {
                        changed = match hit {
                            Hit::Vertex(id) => self.apply_once(
                                document,
                                ManualMeshOperation::DeleteVertices { ids: vec![id] },
                            )?,
                            Hit::Edge([a, b], EdgeHitKind::Constraint) => self.apply_once(
                                document,
                                ManualMeshOperation::ReleaseConstraint { a, b },
                            )?,
                            _ => false,
                        };
                    }
                }
                Ok(changed)
            }
        }
    }

    fn apply_once(
        &mut self,
        document: &mut Document,
        operation: ManualMeshOperation,
    ) -> Result<bool> {
        match document.apply_manual_mesh_once(&self.target, operation) {
            Ok(changed) => {
                self.diagnostic = None;
                if changed {
                    self.refresh_source(document)?;
                }
                Ok(changed)
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                Ok(false)
            }
        }
    }

    fn update_drag(&mut self, document: &mut Document, point: ViewportPoint) -> Result<bool> {
        let Some(gesture) = self.gesture.as_mut() else {
            return Ok(false);
        };
        gesture.current = point;
        gesture.moved |= crate::picking::distance2(gesture.start, point) >= 9.;
        let GestureKind::Drag(starts) = &gesture.kind else {
            return Ok(false);
        };
        if !gesture.moved || starts.is_empty() {
            return Ok(false);
        }
        let before = gesture.transform.to_model(gesture.start);
        let after = gesture.transform.to_model(point);
        let delta = Vec2::new(after.x - before.x, after.y - before.y);
        let targets: Result<Vec<_>> = starts
            .iter()
            .map(|start| {
                let original = self.target.mapping.position(start.uv);
                let target = Vec2::new(original.x + delta.x, original.y + delta.y);
                Ok(ManualVertexTarget {
                    id: start.id,
                    uv: self.target.mapping.uv(target)?,
                })
            })
            .collect();
        let current: HashMap<_, _> = {
            let model = document.model();
            let mesh = model.get_mesh(&self.target.object.id).unwrap();
            mesh.vertex_ids
                .iter()
                .copied()
                .zip(mesh.uvs.iter().copied())
                .collect()
        };
        let operation = ManualMeshOperation::MoveVertices {
            targets: targets?,
            motion_from: Some(
                starts
                    .iter()
                    .map(|start| ManualVertexTarget {
                        id: start.id,
                        uv: current[&start.id],
                    })
                    .collect(),
            ),
        };
        match document.preview_manual_mesh(&self.target, operation) {
            Ok(changed) => {
                self.diagnostic = None;
                Ok(changed)
            }
            Err(error) => {
                self.diagnostic = Some(error.to_string());
                Ok(false)
            }
        }
    }

    pub fn overlay(&self, frame: &CanvasFrame) -> OverlaySnapshot {
        let mut out = OverlaySnapshot::default();
        let Some(mesh) = frame.geometry.mesh(&self.target.object.id) else {
            return out;
        };
        let Some(topology) = frame.geometry.topology.meshes.get(&self.target.object.id) else {
            return out;
        };
        let line = |(a, b): (usize, usize)| {
            (
                frame.transform.to_viewport(mesh.vertices[a].1),
                frame.transform.to_viewport(mesh.vertices[b].1),
            )
        };
        let topology_identity = Arc::as_ptr(topology) as usize;
        let mesh_identity = mesh as *const _ as usize;
        let viewport = frame.version.binding.viewport;
        let projected = {
            let mut cache = self.projected.borrow_mut();
            let current = cache
                .get(&viewport)
                .filter(|value| {
                    value.scene_revision == frame.version.scene
                        && value.topology == topology_identity
                        && value.mesh == mesh_identity
                        && value.transform == frame.transform
                })
                .cloned();
            if let Some(current) = current {
                current
            } else {
                // Projection is local to a viewport. Topology remains shared by
                // all viewports; a fixed display budget bounds cached line memory.
                const MAX_VIEWPORTS: usize = 8;
                const MAX_LINES: usize = 100_000;
                if cache.len() >= MAX_VIEWPORTS && !cache.contains_key(&viewport) {
                    cache.clear();
                }
                let total = topology.boundary.len()
                    + topology.user_edges.len()
                    + topology.support_edges.len()
                    + topology.automatic_edges.len();
                let mut budget = MAX_LINES;
                let mut take = |edges: &[(usize, usize)]| {
                    let lines: Vec<_> = edges.iter().copied().take(budget).map(line).collect();
                    budget -= lines.len();
                    lines
                };
                let mut structural = take(&topology.boundary);
                structural.extend(take(&topology.user_edges));
                structural.extend(take(&topology.support_edges));
                let automatic = take(&topology.automatic_edges);
                let value = ProjectedEdges {
                    scene_revision: frame.version.scene,
                    topology: topology_identity,
                    mesh: mesh_identity,
                    transform: frame.transform,
                    structural: structural.into(),
                    automatic: automatic.into(),
                    truncated: total > MAX_LINES,
                };
                cache.insert(viewport, value.clone());
                value
            }
        };
        out.lines.extend_from_slice(&projected.structural);
        out.automatic_lines.extend_from_slice(&projected.automatic);
        let corners = [
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ]
        .map(|uv| {
            let p = self.target.mapping.position(uv);
            frame
                .transform
                .to_viewport(crate::ModelPoint { x: p.x, y: p.y })
        });
        for i in 0..4 {
            out.reference_lines.push((corners[i], corners[(i + 1) % 4]));
        }
        for edge in topology
            .boundary
            .iter()
            .chain(topology.user_edges.iter())
            .chain(topology.support_edges.iter())
            .chain(topology.automatic_edges.iter())
        {
            let ids = [mesh.vertices[edge.0].0, mesh.vertices[edge.1].0];
            let ids = [ids[0].min(ids[1]), ids[0].max(ids[1])];
            if self.selected_edge == Some(ids) {
                out.accent_lines.push(line(*edge));
            }
            if matches!(self.hover, Hit::Edge(hovered, _) if hovered == ids) {
                out.hover_lines.push(line(*edge));
            }
        }
        for &(id, position) in &mesh.vertices {
            out.vertices.push(OverlayVertex {
                id,
                position: frame.transform.to_viewport(position),
                selected: self.selected.contains(&id),
                hovered: self.hover == Hit::Vertex(id),
                kind: OverlayVertexKind::Mesh,
                editable: true,
            });
        }
        if let Some(gesture) = &self.gesture {
            match gesture.kind {
                GestureKind::Marquee { .. } if gesture.moved => {
                    out.marquee = Some((gesture.start, gesture.current));
                }
                GestureKind::Create(Some(id)) if gesture.moved => {
                    if let Some((_, position)) =
                        mesh.vertices.iter().find(|(vertex, _)| *vertex == id)
                    {
                        out.gizmo_lines
                            .push((frame.transform.to_viewport(*position), gesture.current));
                    }
                }
                _ => {}
            }
        }
        out.target_label = Some(self.target.object.id.clone());
        out.interaction_label = Some(if projected.truncated {
            format!("Manual mesh · {:?} · showing first 100000 edges", self.tool)
        } else {
            format!("Manual mesh · {:?}", self.tool)
        });
        out.unavailable = self.diagnostic.clone();
        out
    }
}
