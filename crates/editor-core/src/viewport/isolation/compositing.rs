//! Flatten context at its original opacity before fading the completed image.
use crate::{ObjectKind, ObjectRef};
use kasane_core::{
    DrawableFrame,
    evaluation::{OffscreenFrame, RenderCommand},
};
use std::collections::{HashMap, HashSet};

pub(super) fn dim_context(frame: &mut DrawableFrame, scope: &HashSet<ObjectRef>) {
    let mut used_ids: HashSet<_> = frame
        .drawables
        .iter()
        .map(|mesh| mesh.id.clone())
        .chain(frame.offscreens.iter().map(|group| group.id.clone()))
        .collect();
    let mut unique_id = |base: &str| {
        let mut id = base.to_owned();
        let mut suffix = 0;
        while !used_ids.insert(id.clone()) {
            suffix += 1;
            id = format!("{base}:{suffix}");
        }
        id
    };
    let context_id = unique_id("kasane-editor:isolation:context");
    let context_groups: HashMap<_, _> = frame
        .offscreens
        .iter()
        .map(|group| {
            (
                group.id.clone(),
                unique_id(&format!("{context_id}:{}", group.id)),
            )
        })
        .collect();
    // The same authored composite may contain both isolated and context meshes.
    // Give each layer its own copy, retaining nesting, masks and blend settings.
    let mut background_groups: Vec<_> = frame
        .offscreens
        .iter()
        .map(|group| {
            let mut background = group.clone();
            background.id = context_groups[&group.id].clone();
            background.runtime_id = background.id.clone();
            background.parent_offscreen_id = Some(group.parent_offscreen_id.as_ref().map_or_else(
                || context_id.clone(),
                |parent| context_groups[parent].clone(),
            ));
            background
        })
        .collect();
    let original_plan = if frame.render_plan.is_empty() {
        let mut ordered: Vec<_> = frame.drawables.iter().enumerate().collect();
        ordered.sort_by_key(|(slot, mesh)| (mesh.render_order, *slot));
        ordered
            .into_iter()
            .map(|(_, mesh)| RenderCommand::DrawMesh {
                mesh_id: mesh.id.clone(),
            })
            .collect()
    } else {
        std::mem::take(&mut frame.render_plan)
    };
    let isolated = |id: &str| scope.contains(&ObjectRef::new(ObjectKind::Mesh, id));
    let mut plan = Vec::with_capacity(original_plan.len() * 2 + 2);
    plan.push(RenderCommand::BeginOffscreen {
        offscreen_id: context_id.clone(),
    });
    for command in &original_plan {
        let command = match command {
            RenderCommand::BeginOffscreen { offscreen_id } => RenderCommand::BeginOffscreen {
                offscreen_id: context_groups[offscreen_id].clone(),
            },
            RenderCommand::EndOffscreen { offscreen_id } => RenderCommand::EndOffscreen {
                offscreen_id: context_groups[offscreen_id].clone(),
            },
            RenderCommand::DrawMesh { mesh_id } if !isolated(mesh_id) => command.clone(),
            _ => continue,
        };
        plan.push(command);
    }
    plan.push(RenderCommand::EndOffscreen {
        offscreen_id: context_id.clone(),
    });
    plan.extend(original_plan.into_iter().filter(|command| match command {
        RenderCommand::DrawMesh { mesh_id } => isolated(mesh_id),
        _ => true,
    }));
    background_groups.push(OffscreenFrame {
        id: context_id.clone(),
        runtime_id: context_id,
        enabled: true,
        opacity: 0.15,
        multiply_color: [1.; 4],
        screen_color: [0., 0., 0., 1.],
        ..Default::default()
    });
    frame.offscreens.extend(background_groups);
    frame.render_plan = plan;
}
