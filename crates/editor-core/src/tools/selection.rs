//! Region selection shared by the rectangle and freehand tools.
use super::{Press, ToolController};
use crate::{
    CanvasScene, ComponentRef, ControlKind, Document, ObjectKind, ObjectRef, ObjectStateIndex,
    ViewportPoint,
};
use anyhow::Result;
use std::{collections::HashSet, hash::Hash};

#[derive(Clone, Copy)]
pub(super) enum SelectionOperation {
    Replace,
    Add,
    Remove,
}
impl SelectionOperation {
    pub fn from_modifiers(shift: bool, alt: bool) -> Self {
        if alt {
            Self::Remove
        } else if shift {
            Self::Add
        } else {
            Self::Replace
        }
    }
    fn apply<T: Eq + Hash + Clone>(self, selected: &mut Vec<T>, hits: Vec<T>) {
        match self {
            Self::Replace => *selected = hits,
            Self::Add => {
                let mut seen: HashSet<_> = selected.iter().cloned().collect();
                for hit in hits {
                    if seen.insert(hit.clone()) {
                        selected.push(hit);
                    }
                }
            }
            Self::Remove => {
                let hits: HashSet<_> = hits.into_iter().collect();
                selected.retain(|item| !hits.contains(item));
            }
        }
    }
}

pub(super) struct LassoGesture {
    pub points: Vec<ViewportPoint>,
    pub operation: SelectionOperation,
}
impl LassoGesture {
    pub fn new(point: ViewportPoint, shift: bool, alt: bool) -> Self {
        Self {
            points: vec![point],
            operation: SelectionOperation::from_modifiers(shift, alt),
        }
    }
    pub fn sample(&mut self, point: ViewportPoint) {
        if crate::picking::distance2(*self.points.last().unwrap(), point) >= 4. {
            self.points.push(point);
        }
    }
    pub fn finish(&mut self, point: ViewportPoint) {
        if self.points.last() != Some(&point) {
            self.points.push(point);
        }
    }
}

pub(super) struct SelectionRegion<'a> {
    bounds: [f32; 4],
    polygon: Option<&'a [ViewportPoint]>,
}
impl<'a> SelectionRegion<'a> {
    pub fn rectangle(a: ViewportPoint, b: ViewportPoint) -> Self {
        Self {
            bounds: [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)],
            polygon: None,
        }
    }
    pub fn lasso(points: &'a [ViewportPoint]) -> Option<Self> {
        let &first = points.first()?;
        if points.len() < 3 {
            return None;
        }
        let second = points.iter().find(|p| **p != first)?;
        // Signed polygon area can cancel out for a valid self-intersecting lasso.
        // Reject only collinear paths; the even-odd rule handles intersections.
        if !points.iter().any(|p| {
            ((second.x as f64 - first.x as f64) * (p.y as f64 - first.y as f64)
                - (second.y as f64 - first.y as f64) * (p.x as f64 - first.x as f64))
                .abs()
                > 0.01
        }) {
            return None;
        }
        let bounds = points.iter().fold(
            [first.x, first.y, first.x, first.y],
            |[x0, y0, x1, y1], p| [x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)],
        );
        Some(Self {
            bounds,
            polygon: Some(points),
        })
    }
    pub fn contains(&self, p: ViewportPoint) -> bool {
        let [x0, y0, x1, y1] = self.bounds;
        if !p.x.is_finite() || !p.y.is_finite() || p.x < x0 || p.x > x1 || p.y < y0 || p.y > y1 {
            return false;
        }
        let Some(points) = self.polygon else {
            return true;
        };
        let mut inside = false;
        let mut a = *points.last().unwrap();
        for &b in points {
            // Include boundary points without expanding the selection by a pixel.
            if crate::picking::segment_distance2(p, a, b) <= 1e-8 {
                return true;
            }
            if (a.y > p.y) != (b.y > p.y) {
                let x = a.x as f64
                    + (p.y as f64 - a.y as f64) * (b.x as f64 - a.x as f64)
                        / (b.y as f64 - a.y as f64);
                if (p.x as f64) < x {
                    inside = !inside;
                }
            }
            a = b;
        }
        inside
    }
}

pub(super) fn has_region_components(
    scene: &CanvasScene,
    selection: &[ObjectRef],
    locks: &ObjectStateIndex,
) -> bool {
    selection.iter().any(|r| {
        !locks.is_locked(r)
            && scene.in_isolation_scope(r)
            && match r.kind {
                ObjectKind::Mesh => scene.mesh_visible(&r.id),
                ObjectKind::Deformer => scene.controls.iter().any(|c| {
                    c.object == *r && c.kind == ControlKind::Warp && c.enabled && c.visible
                }),
                _ => false,
            }
    })
}

impl ToolController {
    pub(super) fn select_region(
        &mut self,
        document: &mut Document,
        press: &Press,
        region: &SelectionRegion<'_>,
        operation: SelectionOperation,
    ) -> Result<()> {
        let locks = document.object_state_index();
        if has_region_components(&press.scene, &press.selection, &locks) {
            let mut hits = Vec::new();
            for c in &press.scene.controls {
                if c.kind != ControlKind::Warp
                    || locks.is_locked(&c.object)
                    || !c.enabled
                    || !c.visible
                    || !press.selection.contains(&c.object)
                {
                    continue;
                }
                let mut add = |id, point| {
                    if region.contains(press.transform.to_viewport(point)) {
                        hits.push(ComponentRef {
                            object: c.object.clone(),
                            id,
                        });
                    }
                };
                if let Some(bezier) = (!press.warp_fine)
                    .then(|| press.scene.bezier(&c.object.id))
                    .flatten()
                {
                    for (i, &p) in bezier.anchors.iter().enumerate() {
                        add(crate::warp_bezier::component_id(i, 0), p);
                    }
                } else {
                    for (i, &p) in c.points.iter().enumerate() {
                        add(i as u32, p);
                    }
                }
            }
            for r in &press.selection {
                if let Some(mesh) = press.scene.mesh(&r.id).filter(|_| {
                    r.kind == ObjectKind::Mesh
                        && !locks.is_locked(r)
                        && press.scene.mesh_visible(&r.id)
                }) {
                    for &(id, point) in &mesh.vertices {
                        if region.contains(press.transform.to_viewport(point)) {
                            hits.push(ComponentRef {
                                object: r.clone(),
                                id,
                            });
                        }
                    }
                }
            }
            operation.apply(&mut self.components, hits);
        } else {
            let hits = press
                .scene
                .meshes
                .iter()
                .filter(|mesh| {
                    !locks.is_locked(&ObjectRef::new(ObjectKind::Mesh, &mesh.id))
                        && press.scene.mesh_visible(&mesh.id)
                        && !mesh.vertices.is_empty()
                        && mesh
                            .vertices
                            .iter()
                            .all(|(_, p)| region.contains(press.transform.to_viewport(*p)))
                })
                .map(|mesh| ObjectRef::new(ObjectKind::Mesh, &mesh.id))
                .collect();
            let mut selection = document.context().selections.clone();
            operation.apply(&mut selection, hits);
            document.select_objects(selection)?;
            self.components.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(x: f32, y: f32) -> ViewportPoint {
        ViewportPoint { x, y }
    }

    #[test]
    fn concave_regions_include_boundaries_and_work_in_both_directions() {
        let mut path = vec![
            p(0., 0.),
            p(10., 0.),
            p(10., 4.),
            p(4., 4.),
            p(4., 10.),
            p(0., 10.),
        ];
        for _ in 0..2 {
            let region = SelectionRegion::lasso(&path).unwrap();
            assert!(region.contains(p(2., 8.)));
            assert!(region.contains(p(4., 8.)));
            assert!(region.contains(p(0., 0.)));
            assert!(!region.contains(p(8., 8.)));
            assert!(!region.contains(p(-1., 0.)));
            path.reverse();
        }
    }
    #[test]
    fn self_intersection_uses_even_odd_and_degenerate_paths_are_rejected() {
        let path = [p(0., 0.), p(10., 10.), p(0., 10.), p(10., 0.), p(0., 0.)];
        let region = SelectionRegion::lasso(&path).unwrap();
        assert!(region.contains(p(5., 2.)));
        assert!(region.contains(p(5., 8.)));
        assert!(region.contains(p(5., 5.)));
        assert!(!region.contains(p(1., 5.)));
        for path in [
            vec![],
            vec![p(0., 0.)],
            vec![p(0., 0.), p(1., 1.), p(2., 2.)],
        ] {
            assert!(SelectionRegion::lasso(&path).is_none());
        }
    }
}
