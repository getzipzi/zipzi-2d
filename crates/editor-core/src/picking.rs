//! Immutable canvas objects and pixel-aware picking, paired with the displayed frame.
mod alpha;
mod controls;
mod geometry;
mod hit;
mod scene;

#[cfg(test)]
mod tests;

use crate::{ModelPoint, ObjectKind as K, ObjectRef, SceneSnapshot};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub use geometry::{bounds, distance2, segment_distance2};

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
    pub(crate) isolation_scope: Option<Arc<HashSet<ObjectRef>>>,
    pub(crate) topology: Arc<crate::canvas_topology::CanvasTopology>,
}
impl CanvasScene {
    pub(crate) fn in_isolation_scope(&self, object: &ObjectRef) -> bool {
        self.isolation_scope
            .as_ref()
            .is_none_or(|scope| scope.contains(object))
    }
    pub(crate) fn isolate(&self, scope: Arc<HashSet<ObjectRef>>) -> Self {
        let mut scene = self.clone();
        scene
            .visible_meshes
            .retain(|id| scope.contains(&ObjectRef::new(K::Mesh, id)));
        for control in &mut scene.controls {
            control.visible &= scope.contains(&control.object);
        }
        scene.isolation_scope = Some(scope);
        scene
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
    pub fn content_bounds(&self) -> Option<[f32; 4]> {
        let mut result: Option<[f32; 4]> = None;
        for mesh in self
            .meshes
            .iter()
            .filter(|mesh| self.mesh_visible(&mesh.id))
        {
            if let Some(b) = mesh.bounds {
                result = Some(match result {
                    None => b,
                    Some([x0, y0, x1, y1]) => {
                        [x0.min(b[0]), y0.min(b[1]), x1.max(b[2]), y1.max(b[3])]
                    }
                });
            }
        }
        result
    }
}
