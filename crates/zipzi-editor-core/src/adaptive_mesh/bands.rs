// SPDX-License-Identifier: GPL-3.0-only
// Rust port of PSD2Live EdgeBandBuilder.kt.
use super::{MeshEdgeMode, geometry::*};
pub(super) fn normals(p: &[Point], passes: usize) -> Vec<Point> {
    let n = p.len();
    let mut t: Vec<_> = (0..n)
        .map(|i| normal(sub(p[(i + 1) % n], p[(i + n - 1) % n])))
        .collect();
    for _ in 0..passes {
        t = (0..n)
            .map(|i| {
                let v = add(add(t[(i + n - 1) % n], mul(t[i], 2.)), t[(i + 1) % n]);
                if distance(v, [0., 0.]) < 1e-9 {
                    t[i]
                } else {
                    normal(v)
                }
            })
            .collect()
    }
    t.into_iter().map(|p| [-p[1], p[0]]).collect()
}
pub(super) fn min_smooth(v: &mut [f64], arcs: &[f64], slope: f64) {
    let n = v.len();
    for _ in 0..2 {
        for k in 1..=n {
            v[k % n] = v[k % n].min(v[k - 1] + slope * arcs[k - 1])
        }
        for k in (0..n).rev() {
            v[k] = v[k].min(v[(k + 1) % n] + slope * arcs[k])
        }
    }
}
fn max_smooth(v: &mut [f64], arcs: &[f64], slope: f64) {
    let n = v.len();
    for _ in 0..2 {
        for k in 1..=n {
            v[k % n] = v[k % n].max(v[k - 1] - slope * arcs[k - 1])
        }
        for k in (0..n).rev() {
            v[k] = v[k].max(v[(k + 1) % n] - slope * arcs[k])
        }
    }
}
pub(super) fn zip_rows(outer: &[usize], inner: &[usize], anchors: &[usize]) -> Vec<Triangle> {
    let n = outer.len();
    let m = inner.len();
    let mut tris = vec![];
    for j in 0..m {
        let start = anchors[j];
        let mut span = (anchors[(j + 1) % m] + n - start) % n;
        if span == 0 {
            span = n
        }
        let split = start + span.div_ceil(2);
        for t in start..start + span {
            let apex = if t < split {
                inner[j]
            } else {
                inner[(j + 1) % m]
            };
            tris.push([outer[t % n], outer[(t + 1) % n], apex]);
        }
        tris.push([outer[split % n], inner[(j + 1) % m], inner[j]]);
    }
    tris
}
pub(super) fn strip(rows: &[Vec<usize>], staggered: &[bool]) -> Vec<Triangle> {
    let mut tris = vec![];
    for r in 0..rows.len() - 1 {
        let n = rows[r + 1].len();
        let anchors: Vec<_> = (0..n)
            .map(|j| if staggered[r] { (j + n - 1) % n } else { j })
            .collect();
        tris.extend(zip_rows(&rows[r], &rows[r + 1], &anchors));
    }
    tris
}
fn bad_samples(rows: &[Vec<Point>], staggered: &[bool], ratio: f64) -> Vec<usize> {
    let pts: Vec<_> = rows.iter().flatten().copied().collect();
    let ids = loop_ids(rows);
    let samples: Vec<_> = rows
        .iter()
        .enumerate()
        .flat_map(|(r, row)| (0..row.len()).map(move |i| i * 2 + usize::from(staggered[r])))
        .collect();
    let mut bad = vec![];
    for t in strip(&ids, staggered) {
        let [a, b, c] = t.map(|i| pts[i]);
        let longest = distance(a, b)
            .max(distance(b, c))
            .max(distance(c, a))
            .powi(2);
        if cross(a, b, c) <= EPS.max(longest * ratio) {
            bad.extend(t.map(|i| samples[i]));
        }
    }
    bad
}
pub(super) fn staggered(mode: MeshEdgeMode) -> Vec<bool> {
    match mode {
        MeshEdgeMode::Single => vec![false],
        MeshEdgeMode::Double => vec![false, true],
        MeshEdgeMode::Triple => vec![false, true, false],
    }
}
pub(super) fn build(
    loops: &[Vec<Point>],
    mode: MeshEdgeMode,
    width: f64,
    neighbors: &[Vec<Point>],
) -> Option<Vec<Loops>> {
    if mode == MeshEdgeMode::Single
        || width <= 1e-3
        || loops.iter().any(|r| r.len() < 6 || r.len() % 2 != 0)
    {
        return None;
    }
    let offsets = if mode == MeshEdgeMode::Double {
        vec![-width * 0.5, width * 0.5]
    } else {
        vec![-width, 0., width]
    };
    let parity = staggered(mode);
    let outward = -offsets[0];
    let inward = *offsets.last()?;
    let own = SegmentIndex::new(loops, width);
    let neighbor = (!neighbors.is_empty()).then(|| SegmentIndex::new(neighbors, width));
    let mut result = vec![];
    for r in loops {
        let n = r.len();
        let mut normals = normals(r, 2);
        let arcs: Vec<_> = (0..n).map(|i| distance(r[i], r[(i + 1) % n])).collect();
        let mut sorted = arcs.clone();
        sorted.sort_by(f64::total_cmp);
        let min_out = outward.min(2_f64.max(0.3 * sorted[n / 2] * 2.));
        let mut shrink = vec![1.; n];
        let mut rows = None;
        let mut unfolded = None;
        let mut scale = vec![1.; n];
        let reach = (outward + inward) / 0.45 * 1.01 + 1e-6;
        for attempt in 0..=16 {
            let room_in: Vec<_> = (0..n)
                .map(|k| 0.45 * own.ray(r[k], normals[k], reach))
                .collect();
            let room_out: Vec<_> = (0..n)
                .map(|k| {
                    let d = mul(normals[k], -1.);
                    0.45 * own.ray(r[k], d, reach).min(
                        neighbor
                            .as_ref()
                            .map_or(f64::INFINITY, |idx| idx.ray(r[k], d, reach)),
                    )
                })
                .collect();
            let mut out: Vec<_> = (0..n)
                .map(|i| outward.min(room_out[i]).max(min_out) * shrink[i])
                .collect();
            min_smooth(&mut out, &arcs, 0.4);
            let mut shift: Vec<_> = (0..n).map(|i| (out[i] - room_out[i]).max(0.)).collect();
            let required = shift.clone();
            max_smooth(&mut shift, &arcs, 0.4);
            for k in 0..n {
                shift[k] = required[k].max(shift[k].min((room_in[k] - inward).max(0.)));
            }
            scale = (0..n)
                .map(|k| ((room_in[k] - shift[k]) / inward).clamp(0., 1.) * shrink[k])
                .collect();
            min_smooth(&mut scale, &arcs, 0.4 / inward);
            let candidate: Loops = offsets
                .iter()
                .enumerate()
                .map(|(row, &offset)| {
                    (0..n / 2)
                        .map(|i| {
                            let k = i * 2 + usize::from(parity[row]);
                            let amount = shift[k]
                                + if offset < 0. {
                                    offset / outward * out[k]
                                } else {
                                    offset * scale[k]
                                };
                            snap(add(r[k], mul(normals[k], amount)))
                        })
                        .collect()
                })
                .collect();
            let folded = bad_samples(&candidate, &parity, 0.2);
            if folded.is_empty() {
                rows = Some(candidate);
                break;
            }
            let inverted = bad_samples(&candidate, &parity, 1e-4);
            if inverted.is_empty() {
                unfolded = Some(candidate)
            }
            let region = |samples: &[usize]| {
                let mut region = vec![false; n];
                for &k in samples {
                    for d in -3..=3 {
                        region[(k as isize + d).rem_euclid(n as isize) as usize] = true;
                    }
                }
                region
            };
            let smooth = region(&folded);
            for _ in 0..2 {
                normals = (0..n)
                    .map(|k| {
                        if smooth[k] {
                            normal(add(
                                add(normals[(k + n - 1) % n], mul(normals[k], 2.)),
                                normals[(k + 1) % n],
                            ))
                        } else {
                            normals[k]
                        }
                    })
                    .collect();
            }
            if attempt % 2 == 1 && !inverted.is_empty() {
                let narrow = region(&inverted);
                for k in 0..n {
                    if narrow[k] {
                        shrink[k] *= 0.75
                    }
                }
            }
        }
        let rows = rows.or(unfolded)?;
        if scale.iter().filter(|v| **v < 0.35).count() as f64 > n as f64 * 0.3 {
            return None;
        }
        result.push(rows);
    }
    for row in 0..offsets.len() {
        if !valid_domain(&result.iter().map(|r| r[row].clone()).collect::<Loops>()) {
            return None;
        }
    }
    Some(result)
}
pub(super) fn offset(r: &[Point], amount: f64, barriers: &[Vec<Point>]) -> Vec<Point> {
    (0..r.len())
        .map(|i| {
            let p = r[(i + r.len() - 1) % r.len()];
            let q = r[i];
            let next = r[(i + 1) % r.len()];
            let a = normal(sub(q, p));
            let b = normal(sub(next, q));
            let n = normal([-a[1] - b[1], a[0] + b[0]]);
            let clearance = barriers
                .iter()
                .map(|ring| {
                    (0..ring.len())
                        .map(|j| segment_distance(q, ring[j], ring[(j + 1) % ring.len()]))
                        .fold(f64::INFINITY, f64::min)
                })
                .fold(f64::INFINITY, f64::min);
            let limit = distance(p, q).min(distance(q, next)).min(clearance) * 0.3;
            snap(add(q, mul(n, amount.clamp(-limit, limit))))
        })
        .collect()
}
