//! Ordered object/component hits and candidate selection, including editor locks.
use super::{
    CONTROL_RADIUS, CanvasScene, ComponentRef, ControlKind, HitTarget, PickCandidate, distance2,
    segment_distance2,
};
use crate::{
    DeformerVisibility, ObjectKind as K, ObjectRef, ToolId, ViewportPoint, ViewportTransform,
};
use std::collections::HashSet;

impl CanvasScene {
    pub fn hit(
        &self,
        point: ViewportPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
    ) -> Option<HitTarget> {
        self.hit_with_mode(
            point,
            t,
            selection,
            tool,
            visibility,
            false,
            &crate::ObjectStateIndex::default(),
        )
    }
    pub(crate) fn hit_with_mode(
        &self,
        point: ViewportPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
        fine: bool,
        locks: &crate::ObjectStateIndex,
    ) -> Option<HitTarget> {
        if tool == ToolId::Hand {
            return None;
        }
        {
            let mut hits = Vec::new();
            for r in selection {
                if r.kind == K::Mesh && !locks.is_locked(r) {
                    if let Some(mesh) = self.mesh(&r.id).filter(|_| self.mesh_visible(&r.id)) {
                        for (id, p) in &mesh.vertices {
                            let distance = distance2(point, t.to_viewport(*p));
                            if distance <= CONTROL_RADIUS * CONTROL_RADIUS {
                                hits.push((distance, r.clone(), *id));
                            }
                        }
                    }
                }
            }
            hits.sort_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then(a.1.id.cmp(&b.1.id))
                    .then(a.2.cmp(&b.2))
            });
            if let Some((_, object, id)) = hits.into_iter().next() {
                return Some(HitTarget::Component(ComponentRef { object, id }));
            }
        }
        let mut controls: Vec<_> = self
            .controls
            .iter()
            .filter(|c| {
                !locks.is_locked(&c.object) && self.control_visible(c, selection, tool, visibility)
            })
            .collect();
        controls.sort_by_key(|c| !selection.contains(&c.object));
        let mut object_hit = None;
        for c in controls {
            let points = self.control_points(c, t);
            let editing = c.kind != ControlKind::Glue && selection.contains(&c.object);
            if editing && c.kind == ControlKind::Warp {
                let bezier = (!fine).then(|| self.bezier(&c.object.id)).flatten();
                let candidates: Vec<_> = if let Some(b) = bezier {
                    b.points().map(|(id, p)| (id, t.to_viewport(p))).collect()
                } else {
                    points
                        .iter()
                        .enumerate()
                        .map(|(i, p)| (i as u32, *p))
                        .collect()
                };
                let nearest = candidates
                    .into_iter()
                    .map(|(id, p)| (id, distance2(point, p)))
                    .filter(|(_, d)| *d <= CONTROL_RADIUS * CONTROL_RADIUS)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let handle = self.deformer_gizmo(c, t).and_then(|g| {
                    g.handles
                        .into_iter()
                        .map(|(h, p)| (h, distance2(point, p)))
                        .filter(|(_, d)| *d <= CONTROL_RADIUS * CONTROL_RADIUS)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                });
                if let Some((h, d)) = handle {
                    if nearest.is_none_or(|(_, n)| d < n) {
                        return Some(HitTarget::DeformerHandle(c.object.clone(), h));
                    }
                }
                if let Some((id, _)) = nearest {
                    return Some(HitTarget::Component(ComponentRef {
                        object: c.object.clone(),
                        id,
                    }));
                }
                if let Some(bezier) = bezier {
                    if bezier.curves.iter().any(|(a, b)| {
                        segment_distance2(point, t.to_viewport(*a), t.to_viewport(*b)) <= 25.
                    }) {
                        object_hit.get_or_insert_with(|| HitTarget::Object(c.object.clone()));
                    }
                    continue;
                }
            }
            for (i, p) in points.iter().enumerate() {
                if distance2(point, *p) <= CONTROL_RADIUS * CONTROL_RADIUS {
                    if editing {
                        return Some(if c.kind == ControlKind::Rotation && i == 1 {
                            HitTarget::RotationHandle(c.object.clone())
                        } else {
                            HitTarget::Component(ComponentRef {
                                object: c.object.clone(),
                                id: i as u32,
                            })
                        });
                    }
                    object_hit.get_or_insert_with(|| HitTarget::Object(c.object.clone()));
                }
            }
            if c.edges.iter().any(|&(a, b)| {
                points
                    .get(a)
                    .zip(points.get(b))
                    .is_some_and(|(a, b)| segment_distance2(point, *a, *b) <= 25.)
            }) {
                object_hit.get_or_insert_with(|| HitTarget::Object(c.object.clone()));
            }
        }
        if tool != ToolId::Hand {
            for c in self
                .controls
                .iter()
                .filter(|c| selection.contains(&c.object) && !locks.is_locked(&c.object))
            {
                let Some(gizmo) = self.deformer_gizmo(c, t) else {
                    continue;
                };
                for (handle, p) in &gizmo.handles {
                    if distance2(point, *p) <= CONTROL_RADIUS * CONTROL_RADIUS {
                        return Some(HitTarget::DeformerHandle(c.object.clone(), *handle));
                    }
                }
                if gizmo
                    .outline
                    .iter()
                    .any(|&(a, b)| segment_distance2(point, a, b) <= 16.)
                {
                    return Some(HitTarget::DeformerHandle(
                        c.object.clone(),
                        if c.kind == ControlKind::Rotation {
                            crate::DeformerHandle::Rotate
                        } else {
                            crate::DeformerHandle::Move
                        },
                    ));
                }
            }
        }
        if object_hit.is_some() {
            return object_hit;
        }
        self.mesh_hits(t.to_model(point))
            .into_iter()
            .find(|id| !locks.is_locked(&ObjectRef::new(K::Mesh, id)))
            .map(|id| HitTarget::Object(ObjectRef::new(K::Mesh, id)))
    }
    pub fn candidates(
        &self,
        point: ViewportPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
    ) -> Vec<PickCandidate> {
        self.candidates_with_locks(
            point,
            t,
            selection,
            tool,
            visibility,
            &crate::ObjectStateIndex::default(),
        )
    }
    pub fn candidates_with_locks(
        &self,
        point: ViewportPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
        locks: &crate::ObjectStateIndex,
    ) -> Vec<PickCandidate> {
        let mut objects = Vec::new();
        if let Some(hit) = self.hit_with_mode(point, t, selection, tool, visibility, false, locks) {
            objects.push(hit.object().clone());
        }
        for id in self.mesh_hits(t.to_model(point)) {
            let mesh = ObjectRef::new(K::Mesh, id);
            objects.push(mesh.clone());
            let ancestors = self.ancestors(&mesh);
            objects.extend(ancestors.iter().cloned());
            for info in &self.index.objects {
                if info.reference.kind == K::Offscreen
                    && ancestors.contains(&ObjectRef::new(K::Part, &info.part_id))
                {
                    objects.push(info.reference.clone());
                }
            }
        }
        let mut seen = HashSet::new();
        objects
            .into_iter()
            .filter(|r| {
                r.kind != K::Deformer || self.control(&r.id).is_some_and(|c| c.enabled && c.visible)
            })
            .filter(|r| self.in_isolation_scope(r) && !locks.is_locked(r) && seen.insert(r.clone()))
            .map(|object| PickCandidate {
                label: format!("{} · {:?}", self.index.name(&object), object.kind),
                object,
            })
            .collect()
    }
}
