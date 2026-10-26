//! Glue drag capabilities, coincident endpoint picking, and tool guidance.
use super::hints::*;
use crate::CommandId;

pub(super) const HINTS: &[InteractionHint] = &[
    InteractionHint::Primary(HintAction::GlueBind),
    InteractionHint::Command(CommandId::SelectTool),
];

use crate::{CanvasFrame, ComponentRef, Document, HitTarget, ObjectKind, ObjectRef, ViewportPoint};

impl super::ToolController {
    /// A single visible handle must hit the editable selected source at an
    /// overlapping seam. It never redirects writes into an unselected mesh.
    pub(super) fn canvas_hit(
        &self,
        document: &Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
    ) -> Option<HitTarget> {
        let scene = &frame.geometry;
        let selection: Vec<_> = document
            .context()
            .selections
            .iter()
            .filter(|r| scene.in_isolation_scope(r))
            .cloned()
            .collect();
        let locks = document.object_state_index();
        let hit = scene.hit_with_mode(
            point,
            frame.transform,
            &selection,
            self.active,
            self.deformer_visibility,
            self.warp_fine,
            &locks,
        );
        let Some(HitTarget::Component(component)) = &hit else {
            return hit;
        };
        if component.object.kind != ObjectKind::Mesh {
            return hit;
        }
        let position = scene
            .mesh(&component.object.id)?
            .vertices
            .iter()
            .find(|(id, _)| *id == component.id)?
            .1;
        let position = frame.transform.to_viewport(position);
        let mut candidates = Vec::new();
        for g in scene.topology.glues.iter() {
            if !scene.control(&g.object.id).is_some_and(|c| {
                scene.control_visible(c, &selection, self.active, self.deformer_visibility)
            }) {
                continue;
            }
            for &(a, b) in &g.pairs {
                for (mesh, slot) in [(&g.mesh_a, a), (&g.mesh_b, b)] {
                    let object = ObjectRef::new(ObjectKind::Mesh, mesh);
                    if !selection.contains(&object) || locks.is_locked(&object) {
                        continue;
                    }
                    let Some((id, p)) = scene.mesh(mesh).and_then(|m| m.vertices.get(slot)) else {
                        continue;
                    };
                    let p = frame.transform.to_viewport(*p);
                    let candidate = ComponentRef { object, id: *id };
                    if (p.x - position.x).hypot(p.y - position.y) < 0.5
                        && !candidates.contains(&candidate)
                    {
                        candidates.push(candidate);
                    }
                }
            }
        }
        if candidates.len() < 2 || !candidates.contains(component) {
            return hit;
        }
        let access = self.edit_access(document, scene, &selection);
        if matches!(access.probe(component), Some(Ok(()))) {
            return hit;
        }
        candidates.sort_by(|a, b| a.object.id.cmp(&b.object.id).then(a.id.cmp(&b.id)));
        candidates
            .into_iter()
            .find(|c| matches!(access.probe(c), Some(Ok(()))))
            .map(HitTarget::Component)
            .or(hit)
    }
}
