use crate::document::Document;
use crate::types::{DeltaKeyforms, Status};

use super::evaluator::EvalContext;
use super::prepared::{DrawItemSource, PreparedEvaluation};
use super::types::OffscreenFrame;

pub(super) fn evaluate(
    doc: &Document,
    prepared: &PreparedEvaluation,
    state: &mut EvalContext<'_>,
) -> Status {
    let groups = &prepared.groups;
    state.orders.clear();
    state.orders.resize(prepared.order_count, 0);
    state.offscreen_orders.clear();
    state
        .offscreen_orders
        .resize(prepared.offscreen_slots.len(), 0);
    for group in groups {
        state.items.clear();
        state
            .items
            .extend(group.items.iter().enumerate().map(|(item_slot, item)| {
                let (order, enabled) = match item.source {
                    DrawItemSource::Mesh(slot) => {
                        let d = &state.frame.drawables[slot];
                        (d.draw_order, d.enabled)
                    }
                    DrawItemSource::Part(slot) => {
                        (state.part_orders[slot], state.enabled_parts[slot])
                    }
                };
                (
                    item_slot,
                    if enabled {
                        order.clamp(group.min_order, group.max_order)
                    } else {
                        group.min_order
                    },
                )
            }));
        state
            .items
            .sort_unstable_by_key(|&(slot, order)| (order, slot));
        let mut rank = state.orders[group.owner];
        for &(item_slot, _) in state.items.iter() {
            let item = &group.items[item_slot];
            if let Some(slot) = item.offscreen {
                state.offscreen_orders[slot] = rank;
                rank += 1;
            }
            state.orders[item.order_slot] = rank;
            rank += item.descendants;
        }
    }
    for (slot, d) in state.frame.drawables.iter_mut().enumerate() {
        d.render_order = state.orders[slot];
    }

    for os_id in doc.offscreen_order() {
        if let Some(os) = doc.get_offscreen(os_id) {
            let part_id = &os.part_id;
            let owner_enabled = os.enabled
                && prepared
                    .part_slots
                    .get(part_id)
                    .is_some_and(|&slot| state.enabled_parts[slot]);
            let mut opacity = 0.0f32;
            let mut mul_color = [1.0f32, 1.0, 1.0, 1.0];
            let mut scr_color = [0.0f32, 0.0, 0.0, 1.0];

            if owner_enabled {
                if let Some(b) = doc
                    .binding_for_scene(part_id)
                    .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                {
                    let s = &state.selections[prepared.scene_selections[part_id]];
                    let mut interp_opa = 0.0f32;
                    let mut interp_mul = [0.0f32; 3];
                    let mut interp_scr = [0.0f32; 3];
                    let mut has_color = false;
                    for k in 0..s.indices.len() {
                        let kf_idx = s.indices[k];
                        let w = s.weights[k];
                        let index = match os.keyform_index(kf_idx, Some(b.track.len())) {
                            Ok(index) => index,
                            Err(status) => return status,
                        };
                        if let Some(index) = index {
                            let kf = &os.keyforms[index];
                            interp_opa += kf.opacity * w;
                            if let Some(m) = kf.multiply {
                                interp_mul[0] += m[0] * w;
                                interp_mul[1] += m[1] * w;
                                interp_mul[2] += m[2] * w;
                                has_color = true;
                            } else {
                                interp_mul[0] += 1.0 * w;
                                interp_mul[1] += 1.0 * w;
                                interp_mul[2] += 1.0 * w;
                            }
                            if let Some(scr) = kf.screen {
                                interp_scr[0] += scr[0] * w;
                                interp_scr[1] += scr[1] * w;
                                interp_scr[2] += scr[2] * w;
                                has_color = true;
                            }
                        } else {
                            interp_opa += 1.0 * w;
                            interp_mul[0] += 1.0 * w;
                            interp_mul[1] += 1.0 * w;
                            interp_mul[2] += 1.0 * w;
                        }
                    }
                    opacity = interp_opa;
                    if has_color {
                        mul_color = [interp_mul[0], interp_mul[1], interp_mul[2], 1.0];
                        scr_color = [interp_scr[0], interp_scr[1], interp_scr[2], 1.0];
                    }
                } else {
                    let index = match os.keyform_index(0, None) {
                        Ok(index) => index,
                        Err(status) => return status,
                    };
                    opacity = 1.0;
                    if let Some(index) = index {
                        let kf = &os.keyforms[index];
                        opacity = kf.opacity;
                        if let Some(m) = kf.multiply {
                            mul_color = [m[0], m[1], m[2], 1.0];
                        }
                        if let Some(scr) = kf.screen {
                            scr_color = [scr[0], scr[1], scr[2], 1.0];
                        }
                    }
                }

                // Apply blendshapes
                for bs in doc.iter_active_blend_bindings_for_target(os_id) {
                    if let DeltaKeyforms::Offscreen(ref forms) = bs.keyforms {
                        for &(kf_idx, eff_w) in state.blend_selections
                            [prepared.blend_binding_selections[&bs.id]]
                            .as_slice()
                        {
                            if kf_idx < forms.len() {
                                let df = &forms[kf_idx];
                                opacity += df.opacity * eff_w;
                                if let Some(m) = df.multiply {
                                    mul_color[0] += (m[0] - 1.0) * eff_w;
                                    mul_color[1] += (m[1] - 1.0) * eff_w;
                                    mul_color[2] += (m[2] - 1.0) * eff_w;
                                }
                                if let Some(scr) = df.screen {
                                    scr_color[0] += scr[0] * eff_w;
                                    scr_color[1] += scr[1] * eff_w;
                                    scr_color[2] += scr[2] * eff_w;
                                }
                            }
                        }
                    }
                }
                opacity = opacity.clamp(0.0, 1.0);
            }

            let ro = state.offscreen_orders[prepared.offscreen_slots[&os.id]];
            let parent_os_id = doc
                .parent_offscreen_for_part(&os.part_id)
                .map(|p| p.id.clone());

            state.frame.offscreens.push(OffscreenFrame {
                renderable: prepared.scene.composite_slots.contains_key(os_id),
                id: os.id.clone(),
                runtime_id: os.runtime_id.clone(),
                owner_part_id: os.part_id.clone(),
                parent_offscreen_id: parent_os_id,
                render_order: ro,
                opacity,
                enabled: owner_enabled,
                blend_mode: os.blend_mode,
                flags: os.flags,
                masks: os.masks.clone(),
                multiply_color: mul_color,
                screen_color: scr_color,
            });
        }
    }

    // Preserve tie order explicitly while sorting in place without a sort workspace.
    state.plan_items.clear();
    let mesh_count = state.frame.drawables.len();
    state.plan_items.extend(
        state
            .frame
            .drawables
            .iter()
            .enumerate()
            .filter(|(_, d)| d.renderable)
            .map(|(i, d)| (d.render_order, i)),
    );
    state.plan_items.extend(
        state
            .frame
            .offscreens
            .iter()
            .enumerate()
            .filter(|(_, o)| o.renderable)
            .map(|(i, o)| (o.render_order, mesh_count + i)),
    );
    state.plan_items.sort_unstable();
    state.active_offscreens.clear();
    let mut cursor = 0;
    for &(_, slot) in state.plan_items.iter() {
        let (id, part_id, is_offscreen) = if slot < mesh_count {
            let mesh = &state.frame.drawables[slot];
            (mesh.id.as_str(), mesh.part_id.as_str(), false)
        } else {
            let os = &state.frame.offscreens[slot - mesh_count];
            (os.id.as_str(), os.owner_part_id.as_str(), true)
        };
        while let Some(&top) = state.active_offscreens.last() {
            let os = &state.frame.offscreens[top];
            if doc.is_part_ancestor(&os.owner_part_id, part_id) {
                break;
            }
            state.active_offscreens.pop();
            write_command(
                &mut state.frame.render_plan,
                &mut cursor,
                CommandKind::End,
                &os.id,
            );
        }
        if is_offscreen {
            state.active_offscreens.push(slot - mesh_count);
            write_command(
                &mut state.frame.render_plan,
                &mut cursor,
                CommandKind::Begin,
                id,
            );
        } else {
            write_command(
                &mut state.frame.render_plan,
                &mut cursor,
                CommandKind::Mesh,
                id,
            );
        }
    }
    while let Some(top) = state.active_offscreens.pop() {
        write_command(
            &mut state.frame.render_plan,
            &mut cursor,
            CommandKind::End,
            &state.frame.offscreens[top].id,
        );
    }
    state.frame.render_plan.truncate(cursor);

    Status::ok()
}

enum CommandKind {
    Mesh,
    Begin,
    End,
}

// Reuse command IDs in both transactional frame buffers.
fn write_command(
    plan: &mut Vec<super::types::RenderCommand>,
    cursor: &mut usize,
    kind: CommandKind,
    id: &str,
) {
    let existing = plan.get_mut(*cursor);
    match (&kind, existing) {
        (CommandKind::Mesh, Some(super::types::RenderCommand::DrawMesh { mesh_id })) => {
            id.clone_into(mesh_id)
        }
        (
            CommandKind::Begin,
            Some(super::types::RenderCommand::BeginOffscreen { offscreen_id }),
        )
        | (CommandKind::End, Some(super::types::RenderCommand::EndOffscreen { offscreen_id })) => {
            id.clone_into(offscreen_id)
        }
        _ => {
            let command = match kind {
                CommandKind::Mesh => super::types::RenderCommand::DrawMesh {
                    mesh_id: id.to_owned(),
                },
                CommandKind::Begin => super::types::RenderCommand::BeginOffscreen {
                    offscreen_id: id.to_owned(),
                },
                CommandKind::End => super::types::RenderCommand::EndOffscreen {
                    offscreen_id: id.to_owned(),
                },
            };
            if *cursor < plan.len() {
                plan[*cursor] = command;
            } else {
                plan.push(command);
            }
        }
    }
    *cursor += 1;
}
