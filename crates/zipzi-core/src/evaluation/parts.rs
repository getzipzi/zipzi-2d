use crate::document::Document;
use zipzi_core::SceneTrackEvaluation;

use super::evaluator::EvalContext;
use super::prepared::PreparedEvaluation;
use super::transforms::f32_to_i32;
use crate::types::DeltaKeyforms;

pub(super) fn evaluate(doc: &Document, prepared: &PreparedEvaluation, state: &mut EvalContext<'_>) {
    state.enabled_parts.resize(prepared.parts.len(), false);
    state.part_orders.resize(prepared.parts.len(), 0);
    for (slot, id) in prepared.parts.iter().enumerate() {
        let p = doc.get_part(id).unwrap();
        let mut enabled = p.enabled
            && prepared.part_parents[slot].is_none_or(|parent| state.enabled_parts[parent]);
        let mut order = p.draw_order;
        if let Some(b) = doc
            .binding_for_scene(id)
            .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
        {
            let s = &state.selections[prepared.scene_selections[id]];
            enabled &= s.enabled;
            if s.enabled {
                order = 0.0;
                for k in 0..s.indices.len() {
                    order += b.track.sample(s.indices[k]).draw_order * s.weights[k];
                }
            }
        }
        let mut bs_list = doc.iter_active_blend_bindings_for_target(id).peekable();
        if bs_list.peek().is_some() {
            order = f32_to_i32(order + 0.001) as f32;
            for bs in bs_list {
                if let DeltaKeyforms::Part(ref forms) = bs.keyforms {
                    for &(kf_idx, eff_w) in
                        state.blend_selections[prepared.blend_binding_selections[&bs.id]].as_slice()
                    {
                        if kf_idx < forms.len() {
                            order += forms[kf_idx].draw_order * eff_w;
                        }
                    }
                }
            }
            order = f32_to_i32((order + 0.001).clamp(0.0, 1000.0)) as f32;
        }
        state.enabled_parts[slot] = enabled;
        state.part_orders[slot] = f32_to_i32(order + 0.001);
    }
}
