//! Bezier authoring controls are independent of the evaluated conversion lattice.
use crate::control_edit::ControlSpace;
use crate::{CanvasScene, Document, ModelPoint, ObjectRef};
use kasane_core::{SceneTrack, Vec2, WarpBezier};
use std::{collections::HashMap, sync::OnceLock};

pub(crate) const BEZIER_ID: u32 = 1 << 31;
pub(crate) fn component_id(node: usize, part: usize) -> u32 {
    BEZIER_ID + (node * 5 + part) as u32
}
pub(crate) fn component(id: u32) -> Option<(usize, usize)> {
    (id >= BEZIER_ID).then(|| {
        let n = (id - BEZIER_ID) as usize;
        (n / 5, n % 5)
    })
}
#[derive(Clone, Debug)]
pub(crate) struct BezierGeometry {
    pub anchors: Vec<ModelPoint>,
    pub handles: Vec<(u32, ModelPoint, ModelPoint)>,
    pub curves: Vec<(ModelPoint, ModelPoint)>,
}

#[derive(Debug)]
enum WarpAuthoring {
    Controller(WarpBezier),
    Lattice {
        rows: u32,
        columns: u32,
        quad: bool,
        points: Vec<Vec2>,
    },
}

/// Own only the authoring inputs at publication. Controller synthesis, parent
/// space preparation and curve sampling wait until this Warp is queried.
/// Parent positions come from the same immutable canvas scene, never the live document.
#[derive(Debug)]
pub(crate) struct LazyBezierGeometry {
    authoring: WarpAuthoring,
    parent_id: String,
    parent_shape: Option<(usize, usize, bool)>,
    geometry: OnceLock<Option<BezierGeometry>>,
}
impl LazyBezierGeometry {
    pub fn get(&self, scene: &CanvasScene) -> Option<&BezierGeometry> {
        self.geometry
            .get_or_init(|| {
                let space =
                    ControlSpace::from_scene(scene, &self.parent_id, self.parent_shape).ok()?;
                let controller;
                let controller = match &self.authoring {
                    WarpAuthoring::Controller(stored) => stored,
                    WarpAuthoring::Lattice {
                        rows,
                        columns,
                        quad,
                        points,
                    } => {
                        controller = WarpBezier::from_lattice(*rows, *columns, *quad, points);
                        &controller
                    }
                };
                Some(BezierGeometry::new(controller, &space))
            })
            .as_ref()
    }
}

pub(crate) fn authored(
    document: &Document,
    object: &ObjectRef,
    parameters: &HashMap<&str, f32>,
) -> Option<LazyBezierGeometry> {
    let model = document.model();
    let transform = model.get_transform(&object.id)?;
    let warp = transform.warp()?;
    let (stored, positions) = if let Some(b) = model
        .binding_for_scene(&object.id)
        .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
    {
        let SceneTrack::Warp { keyforms, .. } = &b.track else {
            return None;
        };
        let form = keyforms.iter().find(|f| {
            b.axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty())
                .zip(&f.keys)
                .all(|(a, k)| {
                    parameters
                        .get(a.parameter_id.as_str())
                        .is_some_and(|value| (value - k).abs() < 1e-6)
                })
        })?;
        (&form.bezier, &form.positions)
    } else {
        (&warp.bezier, &warp.points)
    };
    let authoring = if let Some(controller) = stored {
        WarpAuthoring::Controller(controller.clone())
    } else {
        WarpAuthoring::Lattice {
            rows: warp.rows,
            columns: warp.columns,
            quad: warp.quad,
            points: positions.clone(),
        }
    };
    Some(LazyBezierGeometry {
        authoring,
        parent_id: transform.parent().into(),
        parent_shape: model
            .get_transform(transform.parent())
            .and_then(|t| t.warp())
            .map(|w| (w.rows as usize, w.columns as usize, w.quad)),
        geometry: OnceLock::new(),
    })
}
impl BezierGeometry {
    pub fn new(controller: &WarpBezier, space: &ControlSpace) -> Self {
        let anchors = controller
            .nodes
            .iter()
            .map(|n| space.to_model(n.position))
            .collect();
        let mut handles = Vec::new();
        for (i, n) in controller.nodes.iter().enumerate() {
            for h in 0..4 {
                if controller.has_handle(i, h) {
                    handles.push((
                        component_id(i, h + 1),
                        space.to_model(n.position),
                        space.to_model(n.handles[h]),
                    ));
                }
            }
        }
        let steps = 16;
        let mut curves = Vec::with_capacity(
            ((controller.rows + 1) * controller.columns
                + (controller.columns + 1) * controller.rows) as usize
                * steps,
        );
        for y in 0..=controller.rows {
            let v = y as f32 / controller.rows as f32;
            let mut previous = space.to_model(controller.sample(0., v));
            for x in 0..controller.columns {
                for i in 1..=steps {
                    let u = (x as f32 + i as f32 / steps as f32) / controller.columns as f32;
                    let next = space.to_model(controller.sample(u, v));
                    curves.push((previous, next));
                    previous = next;
                }
            }
        }
        for x in 0..=controller.columns {
            let u = x as f32 / controller.columns as f32;
            let mut previous = space.to_model(controller.sample(u, 0.));
            for y in 0..controller.rows {
                for i in 1..=steps {
                    let v = (y as f32 + i as f32 / steps as f32) / controller.rows as f32;
                    let next = space.to_model(controller.sample(u, v));
                    curves.push((previous, next));
                    previous = next;
                }
            }
        }
        Self {
            anchors,
            handles,
            curves,
        }
    }
    pub fn points(&self) -> impl Iterator<Item = (u32, ModelPoint)> + '_ {
        self.anchors
            .iter()
            .enumerate()
            .map(|(i, p)| (component_id(i, 0), *p))
            .chain(self.handles.iter().map(|(id, _, p)| (*id, *p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_sampling_is_lazy_and_uses_an_owned_authoring_snapshot() -> anyhow::Result<()> {
        let mut doc = Document::new_blank()?;
        let snapshot = doc.snapshot()?;
        let scene = CanvasScene::new(&doc, snapshot);
        let mut controller = WarpBezier::from_lattice(
            1,
            1,
            true,
            &[
                kasane_core::Vec2::new(0., 0.),
                kasane_core::Vec2::new(100., 0.),
                kasane_core::Vec2::new(0., 100.),
                kasane_core::Vec2::new(100., 100.),
            ],
        );
        let space = ControlSpace::new(&doc, &scene, "")?;
        let expected = BezierGeometry::new(&controller, &space);
        let lattice = LazyBezierGeometry {
            authoring: WarpAuthoring::Lattice {
                rows: 1,
                columns: 1,
                quad: true,
                points: controller.nodes.iter().map(|n| n.position).collect(),
            },
            parent_id: String::new(),
            parent_shape: None,
            geometry: OnceLock::new(),
        };
        let stored = LazyBezierGeometry {
            authoring: WarpAuthoring::Controller(controller.clone()),
            parent_id: String::new(),
            parent_shape: None,
            geometry: OnceLock::new(),
        };
        controller.nodes[0].position.x += 50.;
        for cache in [lattice, stored] {
            assert!(cache.geometry.get().is_none());
            let actual = cache.get(&scene).unwrap();
            assert_eq!(actual.anchors, expected.anchors);
            assert_eq!(actual.handles, expected.handles);
            assert_eq!(actual.curves, expected.curves);
            assert!(std::ptr::eq(actual, cache.get(&scene).unwrap()));
        }
        Ok(())
    }
}
