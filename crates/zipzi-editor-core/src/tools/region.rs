//! Region selection shared by the rectangle and freehand tools.
use super::{ToolController, access::EditAccess, gesture::Press};
use crate::{
    CanvasFrame, CanvasScene, ComponentRef, ControlKind, Document, HitTarget, ObjectKind,
    ObjectRef, ObjectStateIndex, ViewportPoint, ViewportTransform,
};
use anyhow::Result;
use std::{collections::HashSet, hash::Hash};

#[derive(Clone, Copy, PartialEq, Eq)]
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
    pub fn click<T: Eq + Hash + Clone>(self, selected: &mut Vec<T>, hit: T) {
        if self == Self::Add && selected.contains(&hit) {
            selected.retain(|item| item != &hit);
        } else {
            self.apply(selected, vec![hit]);
        }
    }
}

pub(super) fn select_click(
    components: &mut Vec<ComponentRef>,
    document: &mut Document,
    press: &Press,
    target: &Option<HitTarget>,
    component_surface: bool,
) -> Result<()> {
    if target.is_none() && !component_surface {
        // A true miss clears selection even with modifiers.
        document.select_objects(Vec::new())?;
        components.clear();
    } else if let Some(HitTarget::Object(object)) = target {
        let mut selection = press.selection.clone();
        press.operation.click(&mut selection, object.clone());
        document.select_objects(selection)?;
        components.clear();
    } else if press.operation == SelectionOperation::Replace {
        components.clear();
    }
    Ok(())
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

pub(super) fn select_region(
    components: &mut Vec<ComponentRef>,
    document: &mut Document,
    press: &Press,
    region: &SelectionRegion<'_>,
) -> Result<()> {
    RegionSelection {
        scene: &press.scene,
        selection: &press.selection,
        transform: press.transform,
        warp_fine: press.warp_fine,
        access: &press.access,
    }
    .apply(components, document, Some(region), press.operation)
}

impl ToolController {
    pub(crate) fn select_all(
        &mut self,
        document: &mut Document,
        frame: &CanvasFrame,
    ) -> Result<()> {
        let selection: Vec<_> = document
            .context()
            .selections
            .iter()
            .filter(|object| frame.geometry.in_isolation_scope(object))
            .cloned()
            .collect();
        let access = self.edit_access(document, &frame.geometry, &selection);
        RegionSelection {
            scene: &frame.geometry,
            selection: &selection,
            transform: frame.transform,
            warp_fine: self.warp_fine,
            access: &access,
        }
        .apply(
            &mut self.components,
            document,
            None,
            SelectionOperation::Replace,
        )
    }
}

struct RegionSelection<'a> {
    scene: &'a CanvasScene,
    selection: &'a [ObjectRef],
    transform: ViewportTransform,
    warp_fine: bool,
    access: &'a EditAccess,
}
impl RegionSelection<'_> {
    fn apply(
        &self,
        components: &mut Vec<ComponentRef>,
        document: &mut Document,
        region: Option<&SelectionRegion<'_>>,
        operation: SelectionOperation,
    ) -> Result<()> {
        let contains = |point| region.is_none_or(|region| region.contains(point));
        let locks = document.object_state_index();
        if has_region_components(self.scene, self.selection, &locks) {
            let mut hits = Vec::new();
            for c in &self.scene.controls {
                if c.kind != ControlKind::Warp
                    || locks.is_locked(&c.object)
                    || !c.enabled
                    || !c.visible
                    || !self.selection.contains(&c.object)
                {
                    continue;
                }
                let mut add = |id, point| {
                    let component = ComponentRef {
                        object: c.object.clone(),
                        id,
                    };
                    if (operation == SelectionOperation::Remove
                        || matches!(self.access.probe(&component), Some(Ok(()))))
                        && contains(self.transform.to_viewport(point))
                    {
                        hits.push(component);
                    }
                };
                if let Some(bezier) = (!self.warp_fine)
                    .then(|| self.scene.bezier(&c.object.id))
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
            for r in self.selection {
                if let Some(mesh) = self.scene.mesh(&r.id).filter(|_| {
                    r.kind == ObjectKind::Mesh
                        && !locks.is_locked(r)
                        && self.scene.mesh_visible(&r.id)
                }) {
                    for &(id, point) in &mesh.vertices {
                        let component = ComponentRef {
                            object: r.clone(),
                            id,
                        };
                        if (operation == SelectionOperation::Remove
                            || matches!(self.access.probe(&component), Some(Ok(()))))
                            && contains(self.transform.to_viewport(point))
                        {
                            hits.push(component);
                        }
                    }
                }
            }
            operation.apply(components, hits);
        } else {
            let hits = self
                .scene
                .meshes
                .iter()
                .filter(|mesh| {
                    !locks.is_locked(&ObjectRef::new(ObjectKind::Mesh, &mesh.id))
                        && self.scene.mesh_visible(&mesh.id)
                        && !mesh.vertices.is_empty()
                        && mesh
                            .vertices
                            .iter()
                            .all(|(_, p)| contains(self.transform.to_viewport(*p)))
                })
                .map(|mesh| ObjectRef::new(ObjectKind::Mesh, &mesh.id))
                .collect();
            let mut selection = document.context().selections.clone();
            operation.apply(&mut selection, hits);
            document.select_objects(selection)?;
            components.clear();
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
