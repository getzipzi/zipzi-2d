//! The ordered linear operations used by Glue, for authoring inverses.
use super::{selection, PreviewValues};
use crate::{DeltaKeyforms, Document, Vec2};

#[derive(Clone, Debug)]
pub struct GlueOperation {
    pub mesh_a: String,
    pub vertex_a: u32,
    pub mesh_b: String,
    pub vertex_b: u32,
    pub amount_a: f32,
    pub amount_b: f32,
}

impl GlueOperation {
    pub fn apply(&self, a: Vec2, b: Vec2) -> (Vec2, Vec2) {
        let d = Vec2::new(b.x - a.x, b.y - a.y);
        (
            Vec2::new(a.x + d.x * self.amount_a, a.y + d.y * self.amount_a),
            Vec2::new(b.x - d.x * self.amount_b, b.y - d.y * self.amount_b),
        )
    }
}

/// Values are the evaluated parameters of the displayed frame. Disabled scene
/// relationships and incomplete pairs are excluded, just as in frame evaluation.
pub fn evaluated_glue_operations(doc: &Document, values: &PreviewValues) -> Vec<GlueOperation> {
    let projection = doc.scene_projection();
    let mut result = Vec::new();
    for id in doc.glue_order() {
        if !projection.glue_slots.contains_key(id) {
            continue;
        }
        let glue = doc.get_glue(id).unwrap();
        let mut intensity = glue.intensity;
        if let Some(b) = glue
            .binding
            .as_ref()
            .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
        {
            let mut scratch = selection::Selection::default();
            let sel = selection::select(doc, values, &b.axes, &mut scratch);
            intensity = sel
                .indices
                .iter()
                .zip(&sel.weights)
                .map(|(&i, &w)| b.keyforms[i].intensity * w)
                .sum();
        }
        let mut blends = doc.iter_active_blend_bindings_for_target(id).peekable();
        if blends.peek().is_some() {
            for b in blends {
                if let DeltaKeyforms::Glue(forms) = &b.keyforms {
                    for (i, w) in selection::evaluate_blend_binding(doc, values, b) {
                        intensity += forms[i].intensity * w;
                    }
                }
            }
            intensity = intensity.clamp(0., 1.);
        }
        if intensity <= 0. {
            continue;
        }
        for pair in &glue.pairs {
            let (Some(a), Some(b)) = (pair.vertex_a, pair.vertex_b) else {
                continue;
            };
            if !projection.pair_slots.contains_key(&pair.id) {
                continue;
            }
            result.push(GlueOperation {
                mesh_a: glue.mesh_a_id.as_str().into(),
                vertex_a: a,
                mesh_b: glue.mesh_b_id.as_str().into(),
                vertex_b: b,
                amount_a: intensity * pair.weight_a,
                amount_b: intensity * pair.weight_b,
            });
        }
    }
    result
}
