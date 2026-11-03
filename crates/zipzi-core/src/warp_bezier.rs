//! Bezier evaluation and lattice algorithms.
use crate::Vec2;
pub use zipzi_contract::warp_bezier::{
    WarpAuthoringSettings, WarpBezier, WarpBezierControlPatch, WarpBezierEditMode, WarpBezierGrid,
    WarpBezierNode, WarpShapeAction,
};

fn add(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Vec2, b: f32) -> Vec2 {
    Vec2::new(a.x * b, a.y * b)
}
pub trait WarpBezierEvaluation: Sized {
    fn from_lattice(rows: u32, columns: u32, quad: bool, points: &[Vec2]) -> Self;
    fn sample(&self, u: f32, v: f32) -> Vec2;
    fn apply_delta(&self, before: &Self, rows: u32, columns: u32, points: &[Vec2]) -> Vec<Vec2>;
    fn from_lattice_grid(
        rows: u32,
        columns: u32,
        quad: bool,
        points: &[Vec2],
        grid: WarpBezierGrid,
    ) -> Self;
    fn resized(&self, grid: WarpBezierGrid) -> Self;
    fn lattice(&self, rows: u32, columns: u32) -> Vec<Vec2>;
    fn edit_controls(
        &self,
        updates: &[WarpBezierControlPatch],
        mode: WarpBezierEditMode,
    ) -> Result<Self, crate::Status>;
}
impl WarpBezierEvaluation for WarpBezier {
    fn from_lattice(rows: u32, columns: u32, quad: bool, points: &[Vec2]) -> Self {
        Self::from_lattice_grid(
            rows,
            columns,
            quad,
            points,
            WarpBezierGrid {
                rows: 1,
                columns: 1,
            },
        )
    }
    fn from_lattice_grid(
        rows: u32,
        columns: u32,
        quad: bool,
        points: &[Vec2],
        grid: WarpBezierGrid,
    ) -> Self {
        let flat: Vec<_> = points.iter().flat_map(|p| [p.x, p.y]).collect();
        let sample = |u: f32, v: f32| {
            let x = u * columns as f32;
            let y = v * rows as f32;
            if (x - x.round()).abs() < 1e-6 && (y - y.round()).abs() < 1e-6 {
                return points[y.round() as usize * (columns as usize + 1) + x.round() as usize];
            }
            let mut out = [0.; 2];
            crate::deformers::warp_points(
                rows as i32,
                columns as i32,
                quad,
                &flat,
                &[u, v],
                &mut out,
                1,
            );
            Vec2::new(out[0], out[1])
        };
        let nr = grid.rows;
        let nc = grid.columns;
        let mut nodes = Vec::new();
        for y in 0..=nr {
            for x in 0..=nc {
                let u = x as f32 / nc as f32;
                let v = y as f32 / nr as f32;
                let position = sample(u, v);
                let du = 1. / nc as f32;
                let dv = 1. / nr as f32;
                let a = sample((u - du).max(0.), v);
                let b = sample((u + du).min(1.), v);
                let c = sample(u, (v - dv).max(0.));
                let d = sample(u, (v + dv).min(1.));
                let dx = mul(sub(b, a), 1. / if x > 0 && x < nc { 6. } else { 3. });
                let dy = mul(sub(d, c), 1. / if y > 0 && y < nr { 6. } else { 3. });
                nodes.push(WarpBezierNode {
                    position,
                    handles: [
                        sub(position, dx),
                        add(position, dx),
                        sub(position, dy),
                        add(position, dy),
                    ],
                });
            }
        }
        Self {
            rows: nr,
            columns: nc,
            nodes,
        }
    }
    fn sample(&self, u: f32, v: f32) -> Vec2 {
        let u = u.clamp(0., 1.) * self.columns as f32;
        let v = v.clamp(0., 1.) * self.rows as f32;
        let x = (u.floor() as u32).min(self.columns - 1);
        let y = (v.floor() as u32).min(self.rows - 1);
        let n = |x: u32, y: u32| &self.nodes[(y * (self.columns + 1) + x) as usize];
        let a = n(x, y);
        let b = n(x + 1, y);
        let c = n(x, y + 1);
        let d = n(x + 1, y + 1);
        let interior = |n: &WarpBezierNode, h: usize, k: usize| {
            sub(add(n.handles[h], n.handles[k]), n.position)
        };
        let net = [
            [a.position, a.handles[1], b.handles[0], b.position],
            [
                a.handles[3],
                interior(a, 1, 3),
                interior(b, 0, 3),
                b.handles[3],
            ],
            [
                c.handles[2],
                interior(c, 1, 2),
                interior(d, 0, 2),
                d.handles[2],
            ],
            [c.position, c.handles[1], d.handles[0], d.position],
        ];
        let basis = |t: f32| {
            [
                (1. - t).powi(3),
                3. * t * (1. - t).powi(2),
                3. * t * t * (1. - t),
                t.powi(3),
            ]
        };
        let bu = basis(u - x as f32);
        let bv = basis(v - y as f32);
        let mut p = Vec2::new(0., 0.);
        for j in 0..4 {
            for i in 0..4 {
                p = add(p, mul(net[j][i], bu[i] * bv[j]));
            }
        }
        p
    }
    fn apply_delta(&self, before: &Self, rows: u32, columns: u32, points: &[Vec2]) -> Vec<Vec2> {
        points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let u = (i % (columns as usize + 1)) as f32 / columns as f32;
                let v = (i / (columns as usize + 1)) as f32 / rows as f32;
                add(*p, sub(self.sample(u, v), before.sample(u, v)))
            })
            .collect()
    }
    fn resized(&self, grid: WarpBezierGrid) -> Self {
        if (grid.rows, grid.columns) == (self.rows, self.columns) {
            return self.clone();
        }
        let mut nodes = Vec::new();
        for y in 0..=grid.rows {
            for x in 0..=grid.columns {
                let u = x as f32 / grid.columns as f32;
                let v = y as f32 / grid.rows as f32;
                let position = self.sample(u, v);
                // Symmetric differences at interior knots, one-sided at the boundary.
                let step = 1e-3;
                let (u0, u1) = ((u - step).max(0.), (u + step).min(1.));
                let (v0, v1) = ((v - step).max(0.), (v + step).min(1.));
                let dx = mul(
                    sub(self.sample(u1, v), self.sample(u0, v)),
                    1. / ((u1 - u0) * 3. * grid.columns as f32),
                );
                let dy = mul(
                    sub(self.sample(u, v1), self.sample(u, v0)),
                    1. / ((v1 - v0) * 3. * grid.rows as f32),
                );
                nodes.push(WarpBezierNode {
                    position,
                    handles: [
                        sub(position, dx),
                        add(position, dx),
                        sub(position, dy),
                        add(position, dy),
                    ],
                });
            }
        }
        Self {
            rows: grid.rows,
            columns: grid.columns,
            nodes,
        }
    }
    fn lattice(&self, rows: u32, columns: u32) -> Vec<Vec2> {
        (0..=rows)
            .flat_map(|y| {
                (0..=columns)
                    .map(move |x| self.sample(x as f32 / columns as f32, y as f32 / rows as f32))
            })
            .collect()
    }
    fn edit_controls(
        &self,
        updates: &[WarpBezierControlPatch],
        mode: WarpBezierEditMode,
    ) -> Result<Self, crate::Status> {
        use std::collections::BTreeSet;
        let mut selected = BTreeSet::new();
        let mut next = self.clone();
        for update in updates {
            let i = update.node as usize;
            let part = update.component as usize;
            if i >= self.nodes.len()
                || part > 4
                || (part > 0 && !self.has_handle(i, part - 1))
                || !selected.insert((i, part))
                || !update.point.x.is_finite()
                || !update.point.y.is_finite()
            {
                return Err(crate::Status::error(
                    "INVALID_WARP_BEZIER_CONTROL",
                    "Invalid or repeated controller address",
                ));
            }
            let node = &mut next.nodes[i];
            if part == 0 {
                let delta = sub(update.point, node.position);
                node.position = update.point;
                for p in &mut node.handles {
                    *p = add(*p, delta);
                }
            } else {
                node.handles[part - 1] = update.point;
            }
        }
        // Explicit handles win over anchor-carried handles, independent of patch ordering.
        for update in updates.iter().filter(|u| u.component > 0) {
            next.nodes[update.node as usize].handles[update.component as usize - 1] = update.point;
        }
        match mode {
            WarpBezierEditMode::Local => {}
            WarpBezierEditMode::PreserveStructure => {
                for &(i, part) in &selected {
                    if part == 0 || selected.contains(&(i, 0)) {
                        continue;
                    }
                    let opposite = if part % 2 == 1 { part + 1 } else { part - 1 };
                    if selected.contains(&(i, opposite)) || !self.has_handle(i, opposite - 1) {
                        continue;
                    }
                    let old = sub(self.nodes[i].handles[part - 1], self.nodes[i].position);
                    let old_opposite =
                        sub(self.nodes[i].handles[opposite - 1], self.nodes[i].position);
                    let length = old.x.hypot(old.y);
                    if length < 1e-8 {
                        return Err(crate::Status::error(
                            "DEGENERATE_WARP_TANGENT",
                            "Cannot preserve a zero-length tangent ratio",
                        ));
                    }
                    let changed = sub(next.nodes[i].handles[part - 1], next.nodes[i].position);
                    next.nodes[i].handles[opposite - 1] = sub(
                        next.nodes[i].position,
                        mul(changed, old_opposite.x.hypot(old_opposite.y) / length),
                    );
                }
            }
            WarpBezierEditMode::SmoothLocal | WarpBezierEditMode::SmoothGlobal => {
                let explicit = next.clone();
                for i in 0..self.nodes.len() {
                    for part in 0..=4 {
                        if selected.contains(&(i, part)) || selected.contains(&(i, 0)) {
                            continue;
                        }
                        let mut delta = Vec2::default();
                        let mut weights = 0.;
                        for &(j, source_part) in &selected {
                            if i == j
                                || (source_part != 0 && source_part != part)
                                || (source_part > 0 && selected.contains(&(j, 0)))
                            {
                                continue;
                            }
                            let dx = (i % (self.columns as usize + 1)) as f32
                                - (j % (self.columns as usize + 1)) as f32;
                            let dy = (i / (self.columns as usize + 1)) as f32
                                - (j / (self.columns as usize + 1)) as f32;
                            let weight = if mode == WarpBezierEditMode::SmoothLocal {
                                let distance = dx.hypot(dy);
                                if distance >= 2. {
                                    continue;
                                }
                                0.5 * (1. + (std::f32::consts::PI * distance / 2.).cos())
                            } else {
                                (-2. * ((dx / self.columns as f32).powi(2)
                                    + (dy / self.rows as f32).powi(2)))
                                .exp()
                            };
                            let change = if source_part == 0 {
                                sub(explicit.nodes[j].position, self.nodes[j].position)
                            } else {
                                sub(
                                    explicit.nodes[j].handles[source_part - 1],
                                    self.nodes[j].handles[source_part - 1],
                                )
                            };
                            delta = add(delta, mul(change, weight));
                            weights += weight;
                        }
                        delta = mul(delta, 1. / weights.max(1.));
                        if part == 0 {
                            next.nodes[i].position = add(self.nodes[i].position, delta);
                        } else {
                            next.nodes[i].handles[part - 1] =
                                add(self.nodes[i].handles[part - 1], delta);
                        }
                    }
                }
            }
        }
        let valid = next.validate();
        if valid.is_ok() {
            Ok(next)
        } else {
            Err(valid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn controller() -> WarpBezier {
        let points = (0..=3)
            .flat_map(|y| (0..=3).map(move |x| Vec2::new(x as f32, y as f32)))
            .collect::<Vec<_>>();
        WarpBezier::from_lattice_grid(
            3,
            3,
            false,
            &points,
            WarpBezierGrid {
                rows: 3,
                columns: 3,
            },
        )
    }
    #[test]
    fn neighborhood_modes_have_distinct_symmetric_and_bounded_influence() {
        let before = controller();
        let patch = [WarpBezierControlPatch {
            node: 5,
            component: 0,
            point: Vec2::new(2., 1.),
        }];
        let local = before
            .edit_controls(&patch, WarpBezierEditMode::Local)
            .unwrap();
        let near = before
            .edit_controls(&patch, WarpBezierEditMode::SmoothLocal)
            .unwrap();
        let global = before
            .edit_controls(&patch, WarpBezierEditMode::SmoothGlobal)
            .unwrap();
        assert_eq!(local.nodes[6].position, before.nodes[6].position);
        for index in [4, 6, 1, 9] {
            assert!(
                (near.nodes[index].position.x - before.nodes[index].position.x - 0.5).abs() < 1e-6
            );
        }
        assert_eq!(near.nodes[15].position, before.nodes[15].position);
        assert!(global.nodes[15].position.x > before.nodes[15].position.x + 0.1);
        assert!(global.nodes[6].position.x > near.nodes[6].position.x);
        for next in [&local, &near, &global] {
            assert_eq!(next.nodes[5].position, patch[0].point);
            for (a, b) in before.nodes.iter().zip(&next.nodes) {
                assert!(b.position.x >= a.position.x && b.position.x <= a.position.x + 1.);
                assert_eq!(a.position.y, b.position.y);
                for i in 0..4 {
                    assert!(
                        ((b.handles[i].x - a.handles[i].x) - (b.position.x - a.position.x)).abs()
                            < 1e-6
                    );
                }
            }
        }
    }
    #[test]
    fn structure_mode_preserves_opposite_tangent_ratio_without_moving_anchors() {
        let before = controller();
        let patch = [WarpBezierControlPatch {
            node: 5,
            component: 2,
            point: Vec2::new(1.5, 1.4),
        }];
        let changed = before
            .edit_controls(&patch, WarpBezierEditMode::PreserveStructure)
            .unwrap();
        assert!((changed.nodes[5].handles[0].x - 0.5).abs() < 1e-6);
        assert!((changed.nodes[5].handles[0].y - 0.6).abs() < 1e-6);
        for (i, (a, b)) in before.nodes.iter().zip(&changed.nodes).enumerate() {
            assert_eq!(a.position, b.position);
            if i != 5 {
                assert_eq!(a, b);
            }
        }
        let mut zero = before.clone();
        zero.nodes[5].handles[1] = zero.nodes[5].position;
        assert!(zero
            .edit_controls(&patch, WarpBezierEditMode::PreserveStructure)
            .is_err());
        assert_eq!(before.nodes[5].position, Vec2::new(1., 1.));
    }
    #[test]
    fn explicit_controls_win_over_influence_and_patch_order() {
        let before = controller();
        let patches = [
            WarpBezierControlPatch {
                node: 5,
                component: 2,
                point: Vec2::new(1.5, 1.4),
            },
            WarpBezierControlPatch {
                node: 5,
                component: 0,
                point: Vec2::new(1.2, 1.1),
            },
            WarpBezierControlPatch {
                node: 6,
                component: 0,
                point: Vec2::new(2.5, 1.),
            },
        ];
        for mode in [
            WarpBezierEditMode::Local,
            WarpBezierEditMode::SmoothLocal,
            WarpBezierEditMode::SmoothGlobal,
            WarpBezierEditMode::PreserveStructure,
        ] {
            let a = before.edit_controls(&patches, mode).unwrap();
            let mut reversed = patches.to_vec();
            reversed.reverse();
            let b = before.edit_controls(&reversed, mode).unwrap();
            assert_eq!(a, b);
            assert_eq!(a.nodes[5].handles[1], patches[0].point);
            assert_eq!(a.nodes[6].position, patches[2].point);
        }
    }
    #[test]
    fn explicit_controller_density_rebuilds_and_refits_affine_samples() {
        let points = vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(0., 1.),
            Vec2::new(1., 1.),
        ];
        let controller = WarpBezier::from_lattice_grid(
            1,
            1,
            false,
            &points,
            WarpBezierGrid {
                rows: 3,
                columns: 2,
            },
        );
        assert_eq!(controller.nodes.len(), 12);
        let refit = controller.resized(WarpBezierGrid {
            rows: 2,
            columns: 4,
        });
        assert_eq!(refit.nodes.len(), 15);
        for y in 0..=20 {
            for x in 0..=20 {
                let (u, v) = (x as f32 / 20., y as f32 / 20.);
                let p = refit.sample(u, v);
                assert!((p.x - u).hypot(p.y - v) < 1e-4);
            }
        }
        assert!(refit.validate().is_ok());
    }
}
