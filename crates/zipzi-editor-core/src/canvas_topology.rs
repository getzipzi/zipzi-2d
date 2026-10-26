//! Authoring structure shared by evaluated canvas frames. Pose and appearance
//! changes do not alter vertex IDs, edge connectivity or parent relationships.
use crate::{ControlKind, ObjectKind as K, ObjectRef};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

/// Only the metadata needed by canvas selection; inspector properties and
/// keyform tables stay in ObjectIndex and are not rebuilt for canvas previews.
#[derive(Clone, Debug)]
pub struct CanvasObject {
    pub reference: ObjectRef,
    pub name: String,
    pub part_id: String,
    parents: Vec<ObjectRef>,
}
#[derive(Clone, Debug, Default)]
pub struct CanvasObjectIndex {
    pub objects: Vec<CanvasObject>,
    slots: HashMap<ObjectRef, usize>,
    pub(crate) ancestors: HashMap<ObjectRef, Vec<ObjectRef>>,
}
impl CanvasObjectIndex {
    pub fn get(&self, object: &ObjectRef) -> Option<&CanvasObject> {
        self.slots.get(object).map(|&i| &self.objects[i])
    }
    pub fn name<'a>(&'a self, object: &'a ObjectRef) -> &'a str {
        self.get(object).map_or(&object.id, |o| &o.name)
    }
    fn add(&mut self, kind: K, id: &str, name: &str, part: &str, deformer: &str) {
        let reference = ObjectRef::new(kind, id);
        let mut parents = Vec::new();
        if !part.is_empty() {
            parents.push(ObjectRef::new(K::Part, part));
        }
        if !deformer.is_empty() {
            parents.push(ObjectRef::new(K::Deformer, deformer));
        }
        self.slots.insert(reference.clone(), self.objects.len());
        self.objects.push(CanvasObject {
            reference,
            name: if name.is_empty() { id } else { name }.into(),
            part_id: part.into(),
            parents,
        });
    }
    fn build_ancestors(&mut self) {
        for object in &self.objects {
            let mut result = Vec::new();
            let mut pending = vec![&object.reference];
            let mut seen = HashSet::from([&object.reference]);
            while let Some(child) = pending.pop() {
                for parent in self.get(child).into_iter().flat_map(|o| &o.parents) {
                    if self.get(parent).is_some() && seen.insert(parent) {
                        result.push(parent.clone());
                        pending.push(parent);
                    }
                }
            }
            self.ancestors.insert(object.reference.clone(), result);
        }
    }
}

#[derive(Debug)]
pub(crate) struct MeshTopology {
    pub vertex_ids: Arc<[u32]>,
    pub triangles: Arc<[[u32; 3]]>,
    pub edges: Arc<[(usize, usize)]>,
    pub boundary: Vec<(usize, usize)>,
    pub user_edges: Arc<[(usize, usize)]>,
    pub support_edges: Arc<[(usize, usize)]>,
    pub automatic_edges: Arc<[(usize, usize)]>,
    meshing: Option<zipzi_core::MeshMeshing>,
    slots: HashMap<u32, usize>,
}
impl MeshTopology {
    fn matches(&self, mesh: &zipzi_core::Mesh) -> bool {
        self.vertex_ids.as_ref() == mesh.vertex_ids
            && self.triangles.as_ref() == mesh.triangles
            && self.meshing == mesh.meshing
    }
    fn new(m: &zipzi_core::Mesh) -> Self {
        let slots: HashMap<_, _> = m
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, i))
            .collect();
        let mut counts = BTreeMap::<(u32, u32), usize>::new();
        for t in &m.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *counts.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        let mut edges = Vec::new();
        let mut boundary = Vec::new();
        let mut user = Vec::new();
        let mut support = Vec::new();
        let mut automatic = Vec::new();
        let constraints: HashMap<_, _> = m
            .meshing
            .as_ref()
            .map(|v| {
                v.constraints
                    .iter()
                    .map(|c| (c.endpoints, c.kind))
                    .collect()
            })
            .unwrap_or_default();
        for ((a, b), count) in counts {
            let (Some(&sa), Some(&sb)) = (slots.get(&a), slots.get(&b)) else {
                continue;
            };
            let pair = (sa, sb);
            edges.push(pair);
            if count == 1 {
                boundary.push(pair);
            } else if let Some(kind) = constraints.get(&[a, b]) {
                match kind {
                    zipzi_core::MeshConstraintKind::User => user.push(pair),
                    zipzi_core::MeshConstraintKind::EdgeSupport => support.push(pair),
                }
            } else {
                automatic.push(pair);
            }
        }
        Self {
            vertex_ids: m.vertex_ids.clone().into(),
            triangles: m.triangles.clone().into(),
            edges: edges.into(),
            boundary,
            user_edges: user.into(),
            support_edges: support.into(),
            automatic_edges: automatic.into(),
            meshing: m.meshing.clone(),
            slots,
        }
    }
}
#[derive(Debug)]
pub(crate) struct ControlTopology {
    pub kind: ControlKind,
    pub edges: Arc<[(usize, usize)]>,
    pub warp_shape: Option<(u32, u32, bool)>,
}
#[derive(Debug)]
pub(crate) struct GlueTopology {
    pub object: ObjectRef,
    pub mesh_a: String,
    pub mesh_b: String,
    pub pairs: Vec<(usize, usize)>,
}
#[derive(Clone, Debug)]
pub(crate) struct CanvasTopology {
    pub index: Arc<CanvasObjectIndex>,
    pub meshes: Arc<HashMap<String, Arc<MeshTopology>>>,
    pub controls: Arc<HashMap<String, ControlTopology>>,
    pub glues: Arc<[GlueTopology]>,
}
impl CanvasTopology {
    pub fn same_geometry(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.meshes, &other.meshes)
            && Arc::ptr_eq(&self.controls, &other.controls)
            && Arc::ptr_eq(&self.glues, &other.glues)
    }

    pub fn rename(&mut self, object: &ObjectRef, name: &str) {
        let index = Arc::make_mut(&mut self.index);
        if let Some(&slot) = index.slots.get(object) {
            index.objects[slot].name = if name.is_empty() { &object.id } else { name }.into();
        }
    }
    pub fn new(model: &zipzi_core::Document) -> Self {
        Self::with_previous(model, None, None)
    }
    /// `changed` is a proven set of mesh topology writes. Other mesh Arcs can be
    /// reused without rescanning their triangles.
    pub fn with_previous(
        model: &zipzi_core::Document,
        previous: Option<&Self>,
        changed: Option<&HashSet<String>>,
    ) -> Self {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = zipzi_core::benchmark::span("editor.canvas_topology_build");
        let mut index = CanvasObjectIndex::default();
        let mut meshes = HashMap::new();
        let mut controls = HashMap::new();
        let mut glues = Vec::new();
        for id in model.part_order() {
            if let Some(p) = model.get_part(id) {
                index.add(K::Part, id, &p.name, &p.parent_id, "");
            }
        }
        for id in model.mesh_order() {
            let Some(m) = model.get_mesh(id) else {
                continue;
            };
            index.add(K::Mesh, id, &m.name, &m.part_id, &m.deformer_id);
            let old = previous.and_then(|p| p.meshes.get(id));
            let reuse =
                old.filter(|old| changed.is_some_and(|set| !set.contains(id)) || old.matches(m));
            meshes.insert(
                id.clone(),
                reuse
                    .cloned()
                    .unwrap_or_else(|| Arc::new(MeshTopology::new(m))),
            );
        }
        for id in model.transform_order() {
            let Some(t) = model.get_transform(id) else {
                continue;
            };
            index.add(K::Deformer, id, &t.name, t.part(), t.parent());
            let mut edges = Vec::new();
            let kind = if let Some(w) = t.warp() {
                let width = w.columns as usize + 1;
                for row in 0..=w.rows as usize {
                    for col in 0..width {
                        let i = row * width + col;
                        if col + 1 < width {
                            edges.push((i, i + 1));
                        }
                        if row < w.rows as usize {
                            edges.push((i, i + width));
                        }
                    }
                }
                ControlKind::Warp
            } else {
                edges.push((0, 1));
                ControlKind::Rotation
            };
            controls.insert(
                id.clone(),
                ControlTopology {
                    kind,
                    edges: edges.into(),
                    warp_shape: t.warp().map(|w| (w.rows, w.columns, w.quad)),
                },
            );
        }
        for id in model.offscreen_order() {
            if let Some(o) = model.get_offscreen(id) {
                index.add(K::Offscreen, id, &o.name, &o.part_id, "");
            }
        }
        for id in model.glue_order() {
            let Some(g) = model.get_glue(id) else {
                continue;
            };
            index.add(K::Glue, id, &g.name, "", "");
            let (Some(a), Some(b)) = (
                meshes.get(g.mesh_a_id.as_str()),
                meshes.get(g.mesh_b_id.as_str()),
            ) else {
                continue;
            };
            let pairs = g
                .pairs
                .iter()
                .filter_map(|p| Some((*a.slots.get(&p.vertex_a?)?, *b.slots.get(&p.vertex_b?)?)))
                .collect();
            glues.push(GlueTopology {
                object: ObjectRef::new(K::Glue, id),
                mesh_a: g.mesh_a_id.as_str().to_owned(),
                mesh_b: g.mesh_b_id.as_str().to_owned(),
                pairs,
            });
        }
        index.build_ancestors();
        Self {
            index: Arc::new(index),
            meshes: Arc::new(meshes),
            controls: Arc::new(controls),
            glues: glues.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zipzi_core::{Mesh, MeshConstraint, MeshConstraintKind, MeshMeshing, Vec2};

    #[test]
    fn support_edge_classification_changes_with_constraints_but_not_positions() {
        let mut mesh = Mesh {
            vertex_ids: vec![0, 1, 2, 3],
            base_positions: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            meshing: Some(MeshMeshing {
                boundary: vec![0, 1, 2, 3],
                constraints: vec![MeshConstraint {
                    endpoints: [0, 2],
                    kind: MeshConstraintKind::EdgeSupport,
                }],
            }),
            ..Default::default()
        };
        let topology = MeshTopology::new(&mesh);
        assert_eq!(topology.boundary.len(), 4);
        assert_eq!(topology.support_edges.as_ref(), &[(0, 2)]);
        assert!(topology.user_edges.is_empty());
        assert!(topology.automatic_edges.is_empty());
        mesh.base_positions[0] = Vec2::new(0.1, 0.1);
        mesh.uvs[0] = Vec2::new(0.1, 0.1);
        assert!(topology.matches(&mesh));
        mesh.meshing.as_mut().unwrap().constraints[0].kind = MeshConstraintKind::User;
        assert!(!topology.matches(&mesh));
    }
}
