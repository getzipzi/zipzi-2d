//! Immutable scene construction and reuse of position, topology, and appearance data.
use super::{CanvasScene, ControlGeometry, ControlKind, MeshGeometry, PICK_ALPHA, bounds};
use crate::{Document, ModelPoint, ObjectKind as K, ObjectRef, SceneSnapshot};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use zipzi_core::evaluation::RenderCommand;

impl CanvasScene {
    pub fn new(document: &Document, source: SceneSnapshot) -> Self {
        Self::build(document, source, None)
    }
    /// Controls and authoring handles are captured anew even when meshes are reused.
    pub(crate) fn with_positions(&self, document: &Document, source: SceneSnapshot) -> Self {
        Self::build(document, source, Some(self))
    }
    fn build(document: &Document, source: SceneSnapshot, previous: Option<&Self>) -> Self {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = zipzi_core::benchmark::span("editor.canvas_geometry_build");
        let topology = document.canvas_topology();
        let index = topology.index.clone();
        let previous = previous.filter(|previous| previous.topology.same_geometry(&topology));
        let visible_meshes = visible_meshes(&source.viewport_frame);
        let to_model = |p: zipzi_core::Vec2| ModelPoint { x: p.x, y: p.y };
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
                rotation_axis: evaluated.rotation_axis.map(|axis| crate::ModelVector {
                    x: axis.x,
                    y: axis.y,
                }),
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
        for g in topology.glues.iter() {
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
                rotation_axis: None,
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
            isolation_scope: None,
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
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = zipzi_core::benchmark::span("editor.canvas_geometry_appearance");
        let topology = document.canvas_topology();
        if !self.topology.same_geometry(&topology) {
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
            isolation_scope: None,
            topology,
        }
    }
}
/// Structural visibility for component overlays and rectangle selection. Texture
/// holes and masks still allow editing the selected mesh's complete topology.
fn visible_meshes(frame: &zipzi_core::DrawableFrame) -> HashSet<String> {
    let eligible = |d: &zipzi_core::Drawable, opacity: f32| {
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
