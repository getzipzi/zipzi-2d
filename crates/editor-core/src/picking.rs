//! Immutable canvas objects and pixel-aware picking, paired with the displayed frame.
use crate::{
    CanvasPoint, DeformerVisibility, Document, ModelPoint, ObjectKind as K, ObjectRef,
    SceneSnapshot, ToolId, ViewportTransform,
};
use kasane_core::{Vec2, evaluation::RenderCommand};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[cfg(test)]
mod tests;

pub const PICK_ALPHA: f32 = 1. / 255.;
pub const CONTROL_RADIUS: f32 = 7.;

#[derive(Clone, Debug)]
pub struct MeshGeometry {
    pub id: String,
    pub vertices: Vec<(u32, ModelPoint)>,
    pub triangles: Arc<[[u32; 3]]>,
    /// Stable vertex slots, shared with all frames using the same topology.
    pub edges: Arc<[(usize, usize)]>,
    pub bounds: Option<[f32; 4]>,
    pub boundary: Vec<(ModelPoint, ModelPoint)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Warp,
    Rotation,
    Glue,
}
#[derive(Clone, Debug)]
pub struct ControlGeometry {
    pub object: ObjectRef,
    pub kind: ControlKind,
    pub points: Vec<ModelPoint>,
    pub edges: Arc<[(usize, usize)]>,
    pub enabled: bool,
    /// Editor display only. Hidden controls still supply their children's coordinate space.
    pub visible: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentRef {
    pub object: ObjectRef,
    /// Stable VertexId for meshes, point index for deformer controls.
    pub id: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HitTarget {
    Object(ObjectRef),
    Component(ComponentRef),
    RotationHandle(ObjectRef),
    DeformerHandle(ObjectRef, crate::DeformerHandle),
}
impl HitTarget {
    pub fn object(&self) -> &ObjectRef {
        match self {
            Self::Object(r) | Self::RotationHandle(r) | Self::DeformerHandle(r, _) => r,
            Self::Component(c) => &c.object,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PickCandidate {
    pub object: ObjectRef,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct CanvasScene {
    /// Unchanged meshes can be shared without rewriting an older frame's pose.
    pub meshes: Arc<[Arc<MeshGeometry>]>,
    pub controls: Vec<ControlGeometry>,
    pub index: Arc<crate::CanvasObjectIndex>,
    pub source: SceneSnapshot,
    beziers: HashMap<String, Arc<crate::warp_bezier::LazyBezierGeometry>>,
    mesh_slots: Arc<HashMap<String, usize>>,
    control_slots: Arc<HashMap<String, usize>>,
    texture_slots: HashMap<String, usize>,
    visible_meshes: HashSet<String>,
    pub(crate) topology: Arc<crate::canvas_topology::CanvasTopology>,
}
impl CanvasScene {
    pub fn new(document: &Document, source: SceneSnapshot) -> Self {
        Self::build(document, source, None)
    }
    /// Controls and authoring handles are captured anew even when meshes are reused.
    pub(crate) fn with_positions(&self, document: &Document, source: SceneSnapshot) -> Self {
        Self::build(document, source, Some(self))
    }
    fn build(document: &Document, source: SceneSnapshot, previous: Option<&Self>) -> Self {
        let topology = document.canvas_topology();
        let index = topology.index.clone();
        let canvas = source.viewport_frame.canvas;
        let previous = previous.filter(|previous| {
            Arc::ptr_eq(&previous.topology, &topology)
                && previous.source.viewport_frame.canvas == canvas
        });
        let visible_meshes = visible_meshes(&source.viewport_frame);
        let to_model = |p| ViewportTransform::evaluated_to_source(canvas, p);
        let meshes: Arc<[Arc<MeshGeometry>]> = source
            .viewport_frame
            .drawables
            .iter()
            .enumerate()
            .filter_map(|(slot, d)| {
                let m = topology.meshes.get(&d.id)?;
                if let Some(previous) = previous
                    && previous
                        .source
                        .viewport_frame
                        .drawables
                        .get(slot)
                        .is_some_and(|old| old.id == d.id && old.positions == d.positions)
                    && let Some(&mesh_slot) = previous.mesh_slots.get(&d.id)
                {
                    return Some(previous.meshes[mesh_slot].clone());
                }
                let vertices: Vec<_> = m
                    .vertex_ids
                    .iter()
                    .copied()
                    .zip(d.positions.iter().copied().map(to_model))
                    .collect();
                let boundary = m
                    .boundary
                    .iter()
                    .filter_map(|&(a, b)| Some((vertices.get(a)?.1, vertices.get(b)?.1)))
                    .collect();
                Some(Arc::new(MeshGeometry {
                    id: d.id.clone(),
                    bounds: bounds(vertices.iter().map(|(_, p)| *p)),
                    vertices,
                    triangles: m.triangles.clone(),
                    edges: m.edges.clone(),
                    boundary,
                }))
            })
            .collect();
        let mut controls = Vec::new();
        for evaluated in &source.viewport_frame.deformers {
            let Some(t) = topology.controls.get(&evaluated.id) else {
                continue;
            };
            controls.push(ControlGeometry {
                object: ObjectRef::new(K::Deformer, &evaluated.id),
                kind: t.kind,
                points: evaluated.points.iter().copied().map(to_model).collect(),
                edges: t.edges.clone(),
                enabled: evaluated.enabled,
                visible: document.deformer_controls_visible(&evaluated.id),
            });
        }
        let mesh_slots = previous
            .filter(|previous| {
                previous.meshes.len() == meshes.len()
                    && previous
                        .meshes
                        .iter()
                        .zip(meshes.iter())
                        .all(|(a, b)| a.id == b.id)
            })
            .map(|previous| previous.mesh_slots.clone())
            .unwrap_or_else(|| {
                Arc::new(
                    meshes
                        .iter()
                        .enumerate()
                        .map(|(i, m)| (m.id.clone(), i))
                        .collect(),
                )
            });
        for g in &topology.glues {
            let (Some(&a), Some(&b)) = (mesh_slots.get(&g.mesh_a), mesh_slots.get(&g.mesh_b))
            else {
                continue;
            };
            let mut points = Vec::new();
            let mut edges = Vec::new();
            for &(a_slot, b_slot) in &g.pairs {
                if let (Some((_, a)), Some((_, b))) = (
                    meshes[a].vertices.get(a_slot),
                    meshes[b].vertices.get(b_slot),
                ) {
                    let i = points.len();
                    points.extend([*a, *b]);
                    edges.push((i, i + 1));
                }
            }
            controls.push(ControlGeometry {
                object: g.object.clone(),
                kind: ControlKind::Glue,
                points,
                edges: edges.into(),
                enabled: visible_meshes.contains(&g.mesh_a) && visible_meshes.contains(&g.mesh_b),
                visible: document.object_state(&g.object).visible(),
            });
        }
        let texture_slots = source
            .textures
            .iter()
            .enumerate()
            .map(|(i, t)| (t.asset_id.clone(), i))
            .collect();
        let control_slots = previous
            .filter(|previous| {
                previous.controls.len() == controls.len()
                    && previous
                        .controls
                        .iter()
                        .zip(&controls)
                        .all(|(a, b)| a.object == b.object)
            })
            .map(|previous| previous.control_slots.clone())
            .unwrap_or_else(|| {
                Arc::new(
                    controls
                        .iter()
                        .enumerate()
                        .map(|(i, c)| (c.object.id.clone(), i))
                        .collect(),
                )
            });
        let mut scene = Self {
            beziers: HashMap::new(),
            meshes,
            controls,
            index,
            source,
            mesh_slots,
            control_slots,
            texture_slots,
            visible_meshes,
            topology,
        };
        let parameters = scene
            .source
            .viewport_frame
            .parameters
            .iter()
            .map(|p| (p.id.as_str(), p.value))
            .collect();
        for c in &scene.controls {
            if c.kind == ControlKind::Warp {
                if let Some(authoring) =
                    crate::warp_bezier::authored(document, &c.object, &parameters)
                {
                    scene
                        .beziers
                        .insert(c.object.id.clone(), Arc::new(authoring));
                }
            }
        }
        scene
    }
    /// Reuse immutable positions while publishing the new opacity, masks and
    /// render plan for picking. Older presented frames retain their own source.
    pub(crate) fn with_appearance(&self, document: &Document, source: SceneSnapshot) -> Self {
        let topology = document.canvas_topology();
        if !Arc::ptr_eq(&self.topology, &topology) {
            return Self::new(document, source);
        }
        let visible_meshes = visible_meshes(&source.viewport_frame);
        let mut controls = self.controls.clone();
        for control in &mut controls {
            if control.object.kind == K::Deformer {
                control.visible = document.deformer_controls_visible(&control.object.id);
            }
            if control.kind == ControlKind::Glue {
                control.visible = document.object_state(&control.object).visible();
                if let Some(g) = topology.glues.iter().find(|g| g.object == control.object) {
                    control.enabled =
                        visible_meshes.contains(&g.mesh_a) && visible_meshes.contains(&g.mesh_b);
                }
            }
        }
        let texture_slots = source
            .textures
            .iter()
            .enumerate()
            .map(|(i, t)| (t.asset_id.clone(), i))
            .collect();
        Self {
            beziers: self.beziers.clone(),
            meshes: self.meshes.clone(),
            controls,
            index: topology.index.clone(),
            source,
            mesh_slots: self.mesh_slots.clone(),
            control_slots: self.control_slots.clone(),
            texture_slots,
            visible_meshes,
            topology,
        }
    }
    pub(crate) fn bezier(&self, id: &str) -> Option<&crate::warp_bezier::BezierGeometry> {
        self.beziers.get(id).and_then(|b| b.get(self))
    }
    pub(crate) fn control(&self, id: &str) -> Option<&ControlGeometry> {
        self.control_slots.get(id).map(|&i| &self.controls[i])
    }
    pub fn mesh(&self, id: &str) -> Option<&MeshGeometry> {
        self.mesh_slots.get(id).map(|&i| self.meshes[i].as_ref())
    }
    pub fn mesh_visible(&self, id: &str) -> bool {
        self.visible_meshes.contains(id)
    }
    pub fn ancestors(&self, object: &ObjectRef) -> &[ObjectRef] {
        self.index.ancestors.get(object).map_or(&[], Vec::as_slice)
    }
    pub fn member_meshes(&self, object: &ObjectRef) -> Vec<&MeshGeometry> {
        if object.kind == K::Mesh {
            return self
                .mesh(&object.id)
                .filter(|_| self.mesh_visible(&object.id))
                .into_iter()
                .collect();
        }
        let target = if object.kind == K::Offscreen {
            self.index
                .get(object)
                .map(|info| info.part_id.as_str())
                .map(|id| ObjectRef::new(K::Part, id))
        } else {
            Some(object.clone())
        };
        self.meshes
            .iter()
            .filter(|m| {
                self.mesh_visible(&m.id)
                    && target.as_ref().is_some_and(|r| {
                        self.ancestors(&ObjectRef::new(K::Mesh, &m.id)).contains(r)
                    })
            })
            .map(Arc::as_ref)
            .collect()
    }
    pub fn control_visible(
        &self,
        c: &ControlGeometry,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
    ) -> bool {
        c.enabled
            && c.visible
            && (selection.contains(&c.object)
                || match c.kind {
                    ControlKind::Glue => tool == ToolId::Glue,
                    _ => {
                        visibility.all
                            || visibility.ancestors
                                && selection
                                    .iter()
                                    .any(|r| self.ancestors(r).contains(&c.object))
                    }
                })
    }
    /// Rotation axes scale with the viewport, independently of the authored pose scale.
    pub fn control_points(&self, c: &ControlGeometry, t: ViewportTransform) -> Vec<CanvasPoint> {
        let mut points: Vec<_> = c.points.iter().map(|p| t.to_canvas(*p)).collect();
        if c.kind == ControlKind::Rotation && points.len() >= 2 {
            let d = CanvasPoint {
                x: points[1].x - points[0].x,
                y: points[1].y - points[0].y,
            };
            let length = d.x.hypot(d.y);
            if length > 1e-6 {
                let axis_length = crate::deformer::ROTATION_AXIS_LENGTH * t.scale;
                points[1] = CanvasPoint {
                    x: points[0].x + d.x / length * axis_length,
                    y: points[0].y + d.y / length * axis_length,
                };
            }
            points.truncate(2);
        }
        points
    }
    pub fn hit(
        &self,
        point: CanvasPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
    ) -> Option<HitTarget> {
        self.hit_with_mode(
            point,
            t,
            selection,
            tool,
            visibility,
            false,
            &crate::ObjectStateIndex::default(),
        )
    }
    pub(crate) fn hit_with_mode(
        &self,
        point: CanvasPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
        fine: bool,
        locks: &crate::ObjectStateIndex,
    ) -> Option<HitTarget> {
        if tool == ToolId::Hand {
            return None;
        }
        {
            let mut hits = Vec::new();
            for r in selection {
                if r.kind == K::Mesh && !locks.is_locked(r) {
                    if let Some(mesh) = self.mesh(&r.id).filter(|_| self.mesh_visible(&r.id)) {
                        for (id, p) in &mesh.vertices {
                            let distance = distance2(point, t.to_canvas(*p));
                            if distance <= CONTROL_RADIUS * CONTROL_RADIUS {
                                hits.push((distance, r.clone(), *id));
                            }
                        }
                    }
                }
            }
            hits.sort_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then(a.1.id.cmp(&b.1.id))
                    .then(a.2.cmp(&b.2))
            });
            if let Some((_, object, id)) = hits.into_iter().next() {
                return Some(HitTarget::Component(ComponentRef { object, id }));
            }
        }
        let mut controls: Vec<_> = self
            .controls
            .iter()
            .filter(|c| {
                !locks.is_locked(&c.object) && self.control_visible(c, selection, tool, visibility)
            })
            .collect();
        controls.sort_by_key(|c| !selection.contains(&c.object));
        let mut object_hit = None;
        for c in controls {
            let points = self.control_points(c, t);
            let editing = c.kind != ControlKind::Glue && selection.contains(&c.object);
            if editing && c.kind == ControlKind::Warp {
                let bezier = (!fine).then(|| self.bezier(&c.object.id)).flatten();
                let candidates: Vec<_> = if let Some(b) = bezier {
                    b.points().map(|(id, p)| (id, t.to_canvas(p))).collect()
                } else {
                    points
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (i as u32, *p))
                        .collect()
                };
                let nearest = candidates
                    .into_iter()
                    .map(|(id, p)| (id, distance2(point, p)))
                    .filter(|(_, d)| *d <= CONTROL_RADIUS * CONTROL_RADIUS)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let handle = self.deformer_gizmo(c, t).and_then(|g| {
                    g.handles
                        .into_iter()
                        .map(|(h, p)| (h, distance2(point, p)))
                        .filter(|(_, d)| *d <= CONTROL_RADIUS * CONTROL_RADIUS)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                });
                if let Some((h, d)) = handle {
                    if nearest.is_none_or(|(_, n)| d < n) {
                        return Some(HitTarget::DeformerHandle(c.object.clone(), h));
                    }
                }
                if let Some((id, _)) = nearest {
                    return Some(HitTarget::Component(ComponentRef {
                        object: c.object.clone(),
                        id,
                    }));
                }
                if let Some(bezier) = bezier {
                    if bezier.curves.iter().any(|(a, b)| {
                        segment_distance2(point, t.to_canvas(*a), t.to_canvas(*b)) <= 25.
                    }) {
                        object_hit.get_or_insert_with(|| HitTarget::Object(c.object.clone()));
                    }
                    continue;
                }
            }
            for (i, p) in points.iter().enumerate() {
                if distance2(point, *p) <= CONTROL_RADIUS * CONTROL_RADIUS {
                    if editing {
                        return Some(if c.kind == ControlKind::Rotation && i == 1 {
                            HitTarget::RotationHandle(c.object.clone())
                        } else {
                            HitTarget::Component(ComponentRef {
                                object: c.object.clone(),
                                id: i as u32,
                            })
                        });
                    }
                    object_hit.get_or_insert_with(|| HitTarget::Object(c.object.clone()));
                }
            }
            if c.edges.iter().any(|&(a, b)| {
                points
                    .get(a)
                    .zip(points.get(b))
                    .is_some_and(|(a, b)| segment_distance2(point, *a, *b) <= 25.)
            }) {
                object_hit.get_or_insert_with(|| HitTarget::Object(c.object.clone()));
            }
        }
        if tool != ToolId::Hand {
            for c in self
                .controls
                .iter()
                .filter(|c| selection.contains(&c.object) && !locks.is_locked(&c.object))
            {
                let Some(gizmo) = self.deformer_gizmo(c, t) else {
                    continue;
                };
                for (handle, p) in &gizmo.handles {
                    if distance2(point, *p) <= CONTROL_RADIUS * CONTROL_RADIUS {
                        return Some(HitTarget::DeformerHandle(c.object.clone(), *handle));
                    }
                }
                if gizmo
                    .outline
                    .iter()
                    .any(|&(a, b)| segment_distance2(point, a, b) <= 16.)
                {
                    return Some(HitTarget::DeformerHandle(
                        c.object.clone(),
                        if c.kind == ControlKind::Rotation {
                            crate::DeformerHandle::Rotate
                        } else {
                            crate::DeformerHandle::Move
                        },
                    ));
                }
            }
        }
        if object_hit.is_some() {
            return object_hit;
        }
        self.mesh_hits(t.to_model(point))
            .into_iter()
            .find(|id| !locks.is_locked(&ObjectRef::new(K::Mesh, id)))
            .map(|id| HitTarget::Object(ObjectRef::new(K::Mesh, id)))
    }
    pub fn candidates(
        &self,
        point: CanvasPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
    ) -> Vec<PickCandidate> {
        self.candidates_with_locks(
            point,
            t,
            selection,
            tool,
            visibility,
            &crate::ObjectStateIndex::default(),
        )
    }
    pub fn candidates_with_locks(
        &self,
        point: CanvasPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
        locks: &crate::ObjectStateIndex,
    ) -> Vec<PickCandidate> {
        let mut objects = Vec::new();
        if let Some(hit) = self.hit_with_mode(point, t, selection, tool, visibility, false, locks) {
            objects.push(hit.object().clone());
        }
        for id in self.mesh_hits(t.to_model(point)) {
            let mesh = ObjectRef::new(K::Mesh, id);
            objects.push(mesh.clone());
            let ancestors = self.ancestors(&mesh);
            objects.extend(ancestors.iter().cloned());
            for info in &self.index.objects {
                if info.reference.kind == K::Offscreen
                    && ancestors.contains(&ObjectRef::new(K::Part, &info.part_id))
                {
                    objects.push(info.reference.clone());
                }
            }
        }
        let mut seen = HashSet::new();
        objects
            .into_iter()
            .filter(|r| {
                r.kind != K::Deformer || self.control(&r.id).is_some_and(|c| c.enabled && c.visible)
            })
            .filter(|r| !locks.is_locked(r) && seen.insert(r.clone()))
            .map(|object| PickCandidate {
                label: format!("{} · {:?}", self.index.name(&object), object.kind),
                object,
            })
            .collect()
    }
    fn raw_alpha(&self, slot: usize, p: ModelPoint) -> f32 {
        let d = &self.source.viewport_frame.drawables[slot];
        let Some(mesh) = self.mesh(&d.id) else {
            return 0.;
        };
        if mesh
            .bounds
            .is_none_or(|b| p.x < b[0] || p.y < b[1] || p.x > b[2] || p.y > b[3])
        {
            return 0.;
        }
        let Some(texture) = self
            .texture_slots
            .get(&d.texture_asset_id)
            .map(|&i| &self.source.textures[i])
        else {
            return 0.;
        };
        let canvas = self.source.viewport_frame.canvas;
        let mut alpha: f32 = 0.;
        for indices in d.indices.chunks_exact(3) {
            let [a, b, c] = [
                indices[0] as usize,
                indices[1] as usize,
                indices[2] as usize,
            ];
            let Some((&pa, &pb, &pc)) = d
                .positions
                .get(a)
                .zip(d.positions.get(b))
                .zip(d.positions.get(c))
                .map(|((a, b), c)| (a, b, c))
            else {
                continue;
            };
            let Some(w) = barycentric(
                p,
                [pa, pb, pc].map(|p| ViewportTransform::evaluated_to_source(canvas, p)),
            ) else {
                continue;
            };
            if let (Some(a), Some(b), Some(c)) = (d.uvs.get(a), d.uvs.get(b), d.uvs.get(c)) {
                alpha = alpha.max(texture_alpha(
                    texture,
                    Vec2::new(
                        a.x * w[0] + b.x * w[1] + c.x * w[2],
                        a.y * w[0] + b.y * w[1] + c.y * w[2],
                    ),
                ));
            }
        }
        alpha
    }
    /// Follow the actual compositing plan. Flattening render_order loses group ordering.
    pub fn mesh_hits(&self, p: ModelPoint) -> Vec<String> {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Vec::new();
        }
        let frame = &self.source.viewport_frame;
        let raw: HashMap<_, _> = frame
            .drawables
            .iter()
            .enumerate()
            .map(|(i, d)| (d.id.as_str(), self.raw_alpha(i, p)))
            .collect();
        let mask = |ids: &[String], inverted: bool| {
            if ids.is_empty() {
                return 1.;
            }
            let coverage = 1.
                - ids.iter().fold(1., |a, id| {
                    a * (1. - raw.get(id.as_str()).copied().unwrap_or(0.))
                });
            if inverted { 1. - coverage } else { coverage }
        };
        let by_id: HashMap<_, _> = frame.drawables.iter().map(|d| (d.id.as_str(), d)).collect();
        let offscreens: HashMap<_, _> = frame
            .offscreens
            .iter()
            .map(|d| (d.id.as_str(), d))
            .collect();
        let mut stack = vec![Vec::<(String, f32)>::new()];
        let fallback;
        let plan = if frame.render_plan.is_empty() {
            let mut draws: Vec<_> = frame.drawables.iter().enumerate().collect();
            draws.sort_by_key(|(i, d)| (d.render_order, *i));
            fallback = draws
                .into_iter()
                .map(|(_, d)| RenderCommand::DrawMesh {
                    mesh_id: d.id.clone(),
                })
                .collect::<Vec<_>>();
            &fallback
        } else {
            &frame.render_plan
        };
        for command in plan {
            match command {
                RenderCommand::BeginOffscreen { .. } => stack.push(Vec::new()),
                RenderCommand::DrawMesh { mesh_id } => {
                    if let Some(d) = by_id
                        .get(mesh_id.as_str())
                        .filter(|d| d.enabled && d.visible)
                    {
                        let a = raw.get(mesh_id.as_str()).copied().unwrap_or(0.)
                            * d.opacity.clamp(0., 1.)
                            * mask(&d.masks, d.inverted_mask);
                        if a > PICK_ALPHA {
                            stack.last_mut().unwrap().push((mesh_id.clone(), a));
                        }
                    }
                }
                RenderCommand::EndOffscreen { offscreen_id } => {
                    if stack.len() == 1 {
                        continue;
                    }
                    let group = stack.pop().unwrap();
                    if let Some(os) = offscreens
                        .get(offscreen_id.as_str())
                        .filter(|os| os.enabled)
                    {
                        let factor = os.opacity.clamp(0., 1.) * mask(&os.masks, os.flags & 8 != 0);
                        stack
                            .last_mut()
                            .unwrap()
                            .extend(group.into_iter().filter_map(|(id, a)| {
                                (a * factor > PICK_ALPHA).then_some((id, a * factor))
                            }));
                    }
                }
            }
        }
        stack
            .into_iter()
            .next()
            .unwrap_or_default()
            .into_iter()
            .rev()
            .map(|(id, _)| id)
            .collect()
    }
}
/// Structural visibility for component overlays and rectangle selection. Texture
/// holes and masks still allow editing the selected mesh's complete topology.
fn visible_meshes(frame: &kasane_core::DrawableFrame) -> HashSet<String> {
    let eligible = |d: &kasane_core::Drawable, opacity: f32| {
        d.enabled && d.visible && d.opacity.clamp(0., 1.) * opacity > PICK_ALPHA
    };
    if frame.render_plan.is_empty() {
        return frame
            .drawables
            .iter()
            .filter(|d| eligible(d, 1.))
            .map(|d| d.id.clone())
            .collect();
    }
    let meshes: HashMap<_, _> = frame.drawables.iter().map(|d| (d.id.as_str(), d)).collect();
    let offscreens: HashMap<_, _> = frame
        .offscreens
        .iter()
        .map(|o| (o.id.as_str(), o))
        .collect();
    let mut opacity = vec![1.];
    let mut result = HashSet::new();
    for command in &frame.render_plan {
        match command {
            RenderCommand::BeginOffscreen { offscreen_id } => {
                let group = offscreens
                    .get(offscreen_id.as_str())
                    .filter(|o| o.enabled)
                    .map_or(0., |o| o.opacity.clamp(0., 1.));
                opacity.push(opacity.last().unwrap() * group);
            }
            RenderCommand::DrawMesh { mesh_id } => {
                if meshes
                    .get(mesh_id.as_str())
                    .is_some_and(|d| eligible(d, *opacity.last().unwrap()))
                {
                    result.insert(mesh_id.clone());
                }
            }
            RenderCommand::EndOffscreen { .. } => {
                if opacity.len() > 1 {
                    opacity.pop();
                }
            }
        }
    }
    result
}
pub fn bounds(points: impl IntoIterator<Item = ModelPoint>) -> Option<[f32; 4]> {
    points
        .into_iter()
        .filter(|p| p.x.is_finite() && p.y.is_finite())
        .fold(None, |b, p| {
            Some(match b {
                None => [p.x, p.y, p.x, p.y],
                Some([x0, y0, x1, y1]) => [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
            })
        })
}
pub fn distance2(a: CanvasPoint, b: CanvasPoint) -> f32 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}
pub fn segment_distance2(p: CanvasPoint, a: CanvasPoint, b: CanvasPoint) -> f32 {
    let length = distance2(a, b);
    let k = if length > 0. {
        ((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / length
    } else {
        0.
    }
    .clamp(0., 1.);
    distance2(
        p,
        CanvasPoint {
            x: a.x + k * (b.x - a.x),
            y: a.y + k * (b.y - a.y),
        },
    )
}
fn barycentric(p: ModelPoint, [a, b, c]: [ModelPoint; 3]) -> Option<[f32; 3]> {
    let cross = |x: ModelPoint, y: ModelPoint, z: ModelPoint| {
        (y.x - x.x) as f64 * (z.y - x.y) as f64 - (y.y - x.y) as f64 * (z.x - x.x) as f64
    };
    let area = cross(a, b, c);
    if area.abs() < 1e-12 {
        return None;
    }
    let weights = [
        cross(p, b, c) / area,
        cross(a, p, c) / area,
        cross(a, b, p) / area,
    ];
    weights
        .iter()
        .all(|w| *w >= -1e-7 && *w <= 1. + 1e-7)
        .then(|| weights.map(|w| w as f32))
}
fn texture_alpha(t: &crate::SceneTexture, uv: Vec2) -> f32 {
    if t.width == 0 || t.height == 0 || !uv.x.is_finite() || !uv.y.is_finite() {
        return 0.;
    }
    let x = uv.x.clamp(0., 1.) * t.width as f32 - 0.5;
    // Drawable UVs are bottom-up; decoded RGBA rows are top-down. Match the
    // conversion in kasane_render::gpu::append_vertices_for for draws and masks.
    let y = (1. - uv.y).clamp(0., 1.) * t.height as f32 - 0.5;
    let sample = |x: i32, y: i32| {
        let i = ((y.clamp(0, t.height as i32 - 1) as u32 * t.width
            + x.clamp(0, t.width as i32 - 1) as u32)
            * 4
            + 3) as usize;
        t.rgba.get(i).copied().unwrap_or(0) as f32 / 255.
    };
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    (sample(ix, iy) * (1. - fx) + sample(ix + 1, iy) * fx) * (1. - fy)
        + (sample(ix, iy + 1) * (1. - fx) + sample(ix + 1, iy + 1) * fx) * fy
}
