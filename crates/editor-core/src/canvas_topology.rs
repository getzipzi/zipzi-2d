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
    slots: HashMap<u32, usize>,
}
#[derive(Debug)]
pub(crate) struct ControlTopology {
    pub kind: ControlKind,
    pub edges: Arc<[(usize, usize)]>,
}
#[derive(Debug)]
pub(crate) struct GlueTopology {
    pub object: ObjectRef,
    pub mesh_a: String,
    pub mesh_b: String,
    pub pairs: Vec<(usize, usize)>,
}
#[derive(Debug)]
pub(crate) struct CanvasTopology {
    pub index: Arc<CanvasObjectIndex>,
    pub meshes: HashMap<String, MeshTopology>,
    pub controls: HashMap<String, ControlTopology>,
    pub glues: Vec<GlueTopology>,
}
impl CanvasTopology {
    pub fn new(model: &kasane_core::Document) -> Self {
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
            for ((a, b), count) in counts {
                if let (Some(&a), Some(&b)) = (slots.get(&a), slots.get(&b)) {
                    edges.push((a, b));
                    if count == 1 {
                        boundary.push((a, b));
                    }
                }
            }
            meshes.insert(
                id.clone(),
                MeshTopology {
                    vertex_ids: m.vertex_ids.clone().into(),
                    triangles: m.triangles.clone().into(),
                    edges: edges.into(),
                    boundary,
                    slots,
                },
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
            let (Some(a), Some(b)) = (meshes.get(&g.mesh_a_id), meshes.get(&g.mesh_b_id)) else {
                continue;
            };
            let pairs = g
                .pairs
                .iter()
                .filter_map(|p| Some((*a.slots.get(&p.vertex_a)?, *b.slots.get(&p.vertex_b)?)))
                .collect();
            glues.push(GlueTopology {
                object: ObjectRef::new(K::Glue, id),
                mesh_a: g.mesh_a_id.clone(),
                mesh_b: g.mesh_b_id.clone(),
                pairs,
            });
        }
        index.build_ancestors();
        Self {
            index: Arc::new(index),
            meshes,
            controls,
            glues,
        }
    }
}
