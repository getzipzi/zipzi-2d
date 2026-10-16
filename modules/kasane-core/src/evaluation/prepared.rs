use std::collections::HashMap;
use std::sync::Arc;

use crate::document::Document;
use crate::types::{BindingAxis, BlendShapeConstraint, Status, Vec2};

#[derive(Debug, Clone)]
pub(super) struct BlendSelectionInput {
    pub parameter: usize,
    pub keys: Vec<f32>,
    pub base_key: usize,
    pub constraints: Vec<(usize, BlendShapeConstraint)>,
}

#[derive(Debug, Clone)]
pub(super) struct SelectionAxis {
    pub parameter: usize,
    pub keys: Vec<f32>,
    pub epsilon: f32,
}

#[derive(Debug, Clone)]
pub(super) struct StaticGeometry {
    pub(super) uvs: Arc<[Vec2]>,
    pub(super) indices: Arc<[u32]>,
}

#[derive(Debug, Clone)]
pub(super) enum DrawItemSource {
    Mesh(usize),
    Part(usize),
}

#[derive(Debug, Clone)]
pub(super) struct DrawItem {
    pub source: DrawItemSource,
    pub order_slot: usize,
    pub descendants: i32,
    pub offscreen: Option<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct DrawGroup {
    pub owner: usize,
    pub min_order: i32,
    pub max_order: i32,
    pub items: Vec<DrawItem>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct GlueVertexSlots {
    pub a: usize,
    pub b: usize,
}

/// Immutable evaluation inputs, owned by the document and invalidated by structural edits.
#[derive(Debug, Clone)]
pub(crate) struct PreparedEvaluation {
    pub(super) parts: Vec<String>,
    pub(super) part_slots: HashMap<String, usize>,
    pub(super) part_parents: Vec<Option<usize>>,
    pub(super) transforms: Vec<String>,
    pub(super) transform_parents: Vec<(Option<usize>, Option<usize>)>,
    pub(super) mesh_parents: Vec<(Option<usize>, Option<usize>)>,
    pub(super) assets: HashMap<String, usize>,
    pub(super) meshes: HashMap<String, usize>,
    pub(super) groups: Vec<DrawGroup>,
    pub(super) order_count: usize,
    pub(super) offscreen_slots: HashMap<String, usize>,
    pub(super) geometry: Vec<StaticGeometry>,
    pub(super) selections: Vec<Vec<SelectionAxis>>,
    pub(super) mesh_selections: Vec<Option<usize>>,
    pub(super) scene_selections: HashMap<String, usize>,
    pub(super) blend_selections: Vec<BlendSelectionInput>,
    pub(super) blend_binding_selections: HashMap<String, usize>,
    pub(super) glue_vertices: Vec<Vec<Option<GlueVertexSlots>>>,
}
impl PreparedEvaluation {
    pub(crate) fn new(doc: &Document) -> Result<Self, Status> {
        let groups = crate::draw_order::resolved_groups(doc);
        let totals: HashMap<_, _> =
            crate::draw_order::descendant_counts_with_offscreens(doc, &groups)
                .into_iter()
                .map(|(id, n)| (id.to_owned(), n))
                .collect();
        let mut geometry = Vec::new();
        for id in doc.mesh_order() {
            let mesh = doc.get_mesh(id).unwrap();
            let uvs = mesh
                .uvs
                .iter()
                .map(|uv| {
                    Vec2::new(
                        uv.x,
                        if doc.canvas().flag & 1 == 0 {
                            uv.y
                        } else {
                            1.0 - uv.y
                        },
                    )
                })
                .collect::<Vec<_>>();
            let mut indices = Vec::new();
            doc.render_indices_into(id, &mut indices)?;
            for tri in indices.as_chunks_mut::<3>().0 {
                tri.swap(1, 2);
                if doc.canvas().flag & 1 == 0 {
                    tri.swap(0, 2);
                }
            }
            geometry.push(StaticGeometry {
                uvs: uvs.into(),
                indices: indices.into(),
            });
        }
        let transforms = doc.sorted_transforms();
        let parts = doc.sorted_parts();
        let part_slots: HashMap<_, _> = parts
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let transform_slots: HashMap<_, _> = transforms
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let part_parents = parts
            .iter()
            .map(|id| {
                part_slots
                    .get(&doc.get_part(id).unwrap().parent_id)
                    .copied()
            })
            .collect();
        let transform_parents = transforms
            .iter()
            .map(|id| {
                let t = doc.get_transform(id).unwrap();
                (
                    part_slots.get(t.part()).copied(),
                    transform_slots.get(t.parent()).copied(),
                )
            })
            .collect();
        let mesh_parents = doc
            .mesh_order()
            .iter()
            .map(|id| {
                let m = doc.get_mesh(id).unwrap();
                (
                    part_slots.get(&m.part_id).copied(),
                    transform_slots.get(&m.deformer_id).copied(),
                )
            })
            .collect();
        let meshes: HashMap<_, _> = doc
            .mesh_order()
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let order_slots: HashMap<_, _> = doc
            .mesh_order()
            .iter()
            .chain(doc.part_order())
            .chain(std::iter::once(&String::new()))
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let offscreen_slots: HashMap<_, _> = doc
            .offscreen_order()
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        let groups = groups
            .into_iter()
            .map(|group| DrawGroup {
                owner: order_slots[&group.owner],
                min_order: group.min_order,
                max_order: group.max_order,
                items: group
                    .items
                    .iter()
                    .map(|id| DrawItem {
                        source: match meshes.get(id) {
                            Some(&slot) => DrawItemSource::Mesh(slot),
                            None => DrawItemSource::Part(part_slots[id]),
                        },
                        order_slot: order_slots[id],
                        descendants: totals.get(id).copied().unwrap_or(1) as i32,
                        offscreen: doc.offscreen_for_part(id).map(|os| offscreen_slots[&os.id]),
                    })
                    .collect(),
            })
            .collect();
        let parameter_slots: HashMap<_, _> = doc
            .parameter_order()
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        // Identical axis grids sample the same normalized parameters. Intern
        // them once per document, including their axis order and float bits.
        let mut selections = Vec::new();
        let mut selection_slots = HashMap::new();
        let mut intern = |axes: &[BindingAxis]| {
            let key: Vec<_> = axes
                .iter()
                .map(|axis| {
                    (
                        parameter_slots[axis.parameter_id.as_str()],
                        axis.keys.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                    )
                })
                .collect();
            *selection_slots.entry(key).or_insert_with(|| {
                let slot = selections.len();
                selections.push(
                    axes.iter()
                        .map(|axis| SelectionAxis {
                            parameter: parameter_slots[axis.parameter_id.as_str()],
                            keys: axis.keys.clone(),
                            epsilon: 0.1f32.powi(
                                doc.get_parameter(&axis.parameter_id)
                                    .unwrap()
                                    .decimal_places,
                            ),
                        })
                        .collect(),
                );
                slot
            })
        };
        let mesh_selections = doc
            .mesh_order()
            .iter()
            .map(|id| {
                doc.binding_for_mesh(id)
                    .map(|binding| intern(&binding.axes))
            })
            .collect();
        let mut scene_selections = HashMap::new();
        for id in doc.scene_binding_order() {
            let binding = doc.get_scene_binding(id).unwrap();
            scene_selections.insert(binding.target_id().to_owned(), intern(&binding.axes));
        }
        for id in doc.glue_order() {
            if let Some(binding) = &doc.get_glue(id).unwrap().binding {
                scene_selections.insert(id.clone(), intern(&binding.axes));
            }
        }
        // Bindings sharing a key table and constraints have identical weights.
        // Resolve their parameter IDs once, retaining declaration order.
        let mut blend_selections = Vec::new();
        let mut blend_slots = HashMap::new();
        let mut blend_binding_selections = HashMap::new();
        for id in doc.blend_binding_order() {
            let binding = doc.get_blend_binding(id).unwrap();
            let key = (binding.key_table_id.clone(), binding.constraint_ids.clone());
            let slot = *blend_slots.entry(key).or_insert_with(|| {
                let table = doc.get_blend_key_table(&binding.key_table_id).unwrap();
                let slot = blend_selections.len();
                blend_selections.push(BlendSelectionInput {
                    parameter: parameter_slots[table.parameter_id.as_str()],
                    keys: table.keys.clone(),
                    base_key: table.base_key_idx,
                    constraints: binding
                        .constraint_ids
                        .iter()
                        .map(|id| {
                            let constraint = doc.get_blend_constraint(id).unwrap();
                            (
                                parameter_slots[constraint.parameter_id.as_str()],
                                constraint.clone(),
                            )
                        })
                        .collect(),
                });
                slot
            });
            blend_binding_selections.insert(id.clone(), slot);
        }
        Ok(Self {
            glue_vertices: doc
                .glue_order()
                .iter()
                .map(|id| {
                    let glue = doc.get_glue(id).unwrap();
                    glue.pairs
                        .iter()
                        .map(|pair| {
                            Some(GlueVertexSlots {
                                a: doc.vertex_slot(&glue.mesh_a_id, pair.vertex_a)?,
                                b: doc.vertex_slot(&glue.mesh_b_id, pair.vertex_b)?,
                            })
                        })
                        .collect()
                })
                .collect(),
            parts,
            part_slots,
            part_parents,
            transform_parents,
            mesh_parents,
            transforms,
            assets: doc
                .asset_order()
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i))
                .collect(),
            meshes,
            order_count: order_slots.len(),
            offscreen_slots,
            groups,
            geometry,
            selections,
            mesh_selections,
            scene_selections,
            blend_selections,
            blend_binding_selections,
        })
    }
}
