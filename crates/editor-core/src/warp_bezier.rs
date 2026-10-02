//! Bezier authoring controls are independent of the evaluated conversion lattice.
use crate::control_edit::ControlSpace;
use crate::{CanvasScene, Document, ModelPoint, ObjectRef};
use kasane_core::{SceneTrack, WarpBezier};

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

pub(crate) fn authored(
    document: &Document,
    scene: &CanvasScene,
    object: &ObjectRef,
) -> Option<(WarpBezier, ControlSpace)> {
    let transform = document.model().get_transform(&object.id)?;
    let warp = transform.warp()?;
    let (stored, positions) = if let Some(b) = document.model().binding_for_scene(&object.id) {
        let SceneTrack::Warp { keyforms, .. } = &b.track else {
            return None;
        };
        let form = keyforms.iter().find(|f| {
            b.axes.iter().zip(&f.keys).all(|(a, k)| {
                scene
                    .source
                    .frame
                    .parameters
                    .iter()
                    .any(|p| p.id == a.parameter_id && (p.value - k).abs() < 1e-6)
            })
        })?;
        (&form.bezier, &form.positions)
    } else {
        (&warp.bezier, &warp.points)
    };
    let controller = stored
        .clone()
        .unwrap_or_else(|| WarpBezier::from_lattice(warp.rows, warp.columns, warp.quad, positions));
    let space = ControlSpace::new(document, scene, transform.parent()).ok()?;
    Some((controller, space))
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
        let mut curves = Vec::new();
        let steps = 16;
        for y in 0..=controller.rows {
            for x in 0..controller.columns {
                for i in 0..steps {
                    let v = y as f32 / controller.rows as f32;
                    let u = |i| (x as f32 + i as f32 / steps as f32) / controller.columns as f32;
                    curves.push((
                        space.to_model(controller.sample(u(i), v)),
                        space.to_model(controller.sample(u(i + 1), v)),
                    ));
                }
            }
        }
        for x in 0..=controller.columns {
            for y in 0..controller.rows {
                for i in 0..steps {
                    let u = x as f32 / controller.columns as f32;
                    let v = |i| (y as f32 + i as f32 / steps as f32) / controller.rows as f32;
                    curves.push((
                        space.to_model(controller.sample(u, v(i))),
                        space.to_model(controller.sample(u, v(i + 1))),
                    ));
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
