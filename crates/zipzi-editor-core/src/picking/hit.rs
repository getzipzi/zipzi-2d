//! Shared point, line and surface picking policy, paired with the displayed frame.
use super::{
    CanvasScene, ComponentRef, ControlKind, HitTarget, PickCandidate, distance2, segment_distance2,
};
use crate::canvas_style::{LINE_HIT_RADIUS, POINT_HIT_RADIUS};
use crate::{
    DeformerVisibility, ObjectKind as K, ObjectRef, ToolId, ViewportPoint, ViewportTransform,
};
use std::collections::HashSet;

impl CanvasScene {
    fn warp_outline_hits(
        &self,
        point: ViewportPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
        locks: &crate::ObjectStateIndex,
    ) -> Vec<ObjectRef> {
        if tool == ToolId::Hand {
            return Vec::new();
        }
        let mut hits: Vec<_> = self
            .controls
            .iter()
            .filter(|c| {
                c.kind == ControlKind::Warp
                    && !locks.is_locked(&c.object)
                    && self.control_visible(c, selection, tool, visibility)
            })
            .filter_map(|c| {
                let d = self
                    .warp_outline(c, t)
                    .into_iter()
                    .map(|(a, b)| segment_distance2(point, a, b))
                    .min_by(f32::total_cmp)?;
                (d <= LINE_HIT_RADIUS.powi(2)).then_some((d, c.object.clone()))
            })
            .collect();
        hits.sort_by(|a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| selection.contains(&b.1).cmp(&selection.contains(&a.1)))
                .then_with(|| self.ancestors(&b.1).len().cmp(&self.ancestors(&a.1).len()))
                .then(a.1.id.cmp(&b.1.id))
        });
        hits.into_iter().map(|(_, o)| o).collect()
    }
    pub fn hit(
        &self,
        point: ViewportPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
    ) -> Option<HitTarget> {
        self.hit_with_state(
            point,
            t,
            selection,
            tool,
            visibility,
            &crate::ObjectStateIndex::default(),
            None,
        )
    }
    pub(crate) fn hit_with_state(
        &self,
        point: ViewportPoint,
        t: ViewportTransform,
        selection: &[ObjectRef],
        tool: ToolId,
        visibility: DeformerVisibility,
        locks: &crate::ObjectStateIndex,
        warp_selection: Option<&crate::deformer::WarpGizmos>,
    ) -> Option<HitTarget> {
        if tool == ToolId::Hand {
            return None;
        }
        // Within each selection tier, rank points, lines, then visible surfaces.
        let mut hits = Vec::new();
        let mut add = |class: u8, target: HitTarget, distance: f32| {
            let limit = match class {
                0 => POINT_HIT_RADIUS.powi(2),
                1 => LINE_HIT_RADIUS.powi(2),
                _ => 0.,
            };
            if distance <= limit {
                hits.push((selection.contains(target.object()), class, distance, target));
            }
        };
        for object in selection
            .iter()
            .filter(|o| o.kind == K::Mesh && !locks.is_locked(o))
        {
            if let Some(mesh) = self
                .mesh(&object.id)
                .filter(|_| self.mesh_visible(&object.id))
            {
                for (id, p) in &mesh.vertices {
                    add(
                        0,
                        HitTarget::Component(ComponentRef {
                            object: object.clone(),
                            id: *id,
                        }),
                        distance2(point, t.to_viewport(*p)),
                    );
                }
            }
        }
        if let Some(frames) = warp_selection
            .filter(|s| !s.objects.is_empty() && s.objects.iter().all(|o| !locks.is_locked(o)))
        {
            for (object, gizmo) in &frames.items {
                for (handle, p) in &gizmo.handles {
                    add(
                        0,
                        HitTarget::DeformerHandle(object.clone(), *handle),
                        distance2(point, *p),
                    );
                }
                for &(a, b) in gizmo.outline.iter().take(8) {
                    add(
                        1,
                        HitTarget::DeformerHandle(object.clone(), crate::DeformerHandle::Move),
                        segment_distance2(point, a, b),
                    );
                }
            }
        }
        for c in self.controls.iter().filter(|c| {
            !locks.is_locked(&c.object) && self.control_visible(c, selection, tool, visibility)
        }) {
            let selected = selection.contains(&c.object);
            if c.kind == ControlKind::Rotation {
                if let Some(gizmo) = self.deformer_gizmo(c, t) {
                    if selected {
                        if let Some(p) = self.control_points(c, t).first() {
                            add(
                                0,
                                HitTarget::Component(ComponentRef {
                                    object: c.object.clone(),
                                    id: 0,
                                }),
                                distance2(point, *p),
                            );
                        }
                        for (handle, p) in &gizmo.handles {
                            add(
                                0,
                                HitTarget::DeformerHandle(c.object.clone(), *handle),
                                distance2(point, *p),
                            );
                        }
                    }
                    if gizmo.bone_hit(point) {
                        add(
                            2,
                            if selected {
                                HitTarget::RotationHandle(c.object.clone())
                            } else {
                                HitTarget::Object(c.object.clone())
                            },
                            0.,
                        );
                    }
                }
                continue;
            }
            let adjusting = warp_selection.is_some_and(|s| s.adjusting.contains(&c.object));
            let detail = selected
                || visibility.all
                || visibility.ancestors
                    && selection
                        .iter()
                        .any(|r| self.ancestors(r).contains(&c.object));
            if c.kind == ControlKind::Glue || detail {
                let points = self.control_points(c, t);
                if !adjusting && (selected || c.kind == ControlKind::Glue) {
                    for (id, p) in points.iter().enumerate() {
                        add(
                            0,
                            if c.kind == ControlKind::Glue {
                                HitTarget::Object(c.object.clone())
                            } else {
                                HitTarget::Component(ComponentRef {
                                    object: c.object.clone(),
                                    id: id as u32,
                                })
                            },
                            distance2(point, *p),
                        );
                    }
                }
                for &(a, b) in c.edges.iter() {
                    if let Some((a, b)) = points.get(a).zip(points.get(b)) {
                        add(
                            1,
                            HitTarget::Object(c.object.clone()),
                            segment_distance2(point, *a, *b),
                        );
                    }
                }
            } else {
                for (a, b) in self.warp_outline(c, t) {
                    add(
                        1,
                        HitTarget::Object(c.object.clone()),
                        segment_distance2(point, a, b),
                    );
                }
            }
            if selected && c.kind == ControlKind::Warp {
                if let Some(bezier) = self.bezier(&c.object.id) {
                    if !adjusting {
                        for (id, p) in bezier.points() {
                            add(
                                0,
                                HitTarget::Component(ComponentRef {
                                    object: c.object.clone(),
                                    id,
                                }),
                                distance2(point, t.to_viewport(p)),
                            );
                        }
                    }
                    for &(a, b) in &bezier.curves {
                        add(
                            1,
                            HitTarget::Object(c.object.clone()),
                            segment_distance2(point, t.to_viewport(a), t.to_viewport(b)),
                        );
                    }
                    for &(_, a, b) in &bezier.handles {
                        add(
                            1,
                            HitTarget::Object(c.object.clone()),
                            segment_distance2(point, t.to_viewport(a), t.to_viewport(b)),
                        );
                    }
                }
                if !warp_selection.is_some_and(|s| !s.objects.is_empty()) {
                    if let Some(gizmo) = self.gizmo_for_selection(c, t, warp_selection) {
                        for (handle, p) in gizmo.handles {
                            add(
                                0,
                                HitTarget::DeformerHandle(c.object.clone(), handle),
                                distance2(point, p),
                            );
                        }
                        for (a, b) in gizmo.outline {
                            add(
                                1,
                                HitTarget::DeformerHandle(
                                    c.object.clone(),
                                    crate::DeformerHandle::Move,
                                ),
                                segment_distance2(point, a, b),
                            );
                        }
                    }
                }
            }
        }
        // On a coincident point, an explicit component wins over its frame.
        // Bezier component IDs rank before coincident lattice IDs.
        let component_rank = |hit: &HitTarget| match hit {
            HitTarget::Component(c) => (0, u32::MAX - c.id),
            _ => (1, 0),
        };
        let control_hit = hits.into_iter().min_by(|a, b| {
            b.0.cmp(&a.0)
                .then(a.1.cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
                .then_with(|| component_rank(&a.3).cmp(&component_rank(&b.3)))
        });
        let control_hit = match control_hit {
            Some((selected, _, _, target))
                if selected
                    || !selection.iter().any(|object| {
                        object.kind == K::Mesh
                            && self.mesh_visible(&object.id)
                            && !locks.is_locked(object)
                    }) =>
            {
                return Some(target);
            }
            hit => hit.map(|(_, _, _, target)| target),
        };
        // The first Mesh in each selection tier keeps its compositing order.
        let mesh_hit = self
            .mesh_hits(t.to_model(point))
            .into_iter()
            .map(|id| ObjectRef::new(K::Mesh, id))
            .filter(|object| !locks.is_locked(object))
            .min_by_key(|object| !selection.contains(object));
        match mesh_hit {
            Some(object) if selection.contains(&object) => Some(HitTarget::Object(object)),
            mesh_hit => control_hit.or_else(|| mesh_hit.map(HitTarget::Object)),
        }
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
        if let Some(hit) = self.hit_with_state(point, t, selection, tool, visibility, locks, None) {
            objects.push(hit.object().clone());
        }
        // Include every overlapping bone, even with no drawable beneath it.
        if tool != ToolId::Hand {
            for control in &self.controls {
                if control.kind == ControlKind::Rotation
                    && !locks.is_locked(&control.object)
                    && self.control_visible(control, selection, tool, visibility)
                    && self
                        .deformer_gizmo(control, t)
                        .is_some_and(|g| g.bone_hit(point))
                {
                    objects.push(control.object.clone());
                }
            }
        }
        objects.extend(self.warp_outline_hits(point, t, selection, tool, visibility, locks));
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
