// SPDX-License-Identifier: GPL-3.0-only
// PSD2Live AlphaEdgePreprocessor, MeshWrap and AdaptiveMeshGenerator contour port.
use super::geometry::*;
use super::{MeshError, MeshSettings};
use geo::{Area, Coord, Polygon, Rect, Simplify, unary_union};
use std::collections::BTreeMap;

pub(super) fn domains(
    width: usize,
    height: usize,
    alpha: &[u8],
    s: &MeshSettings,
) -> Result<Vec<Loops>, MeshError> {
    let threshold = s.alpha_threshold.max(1);
    let mut horizontal = vec![0u8; alpha.len()];
    let mut blur = horizontal.clone();
    for y in 0..height {
        for x in 0..width {
            let at = |x| alpha[y * width + x] as u32;
            horizontal[y * width + x] =
                ((at(x.saturating_sub(1)) + 2 * at(x) + at((x + 1).min(width - 1)) + 2) / 4) as u8;
        }
    }
    let mut hist = [0usize; 256];
    let mut count = 0;
    for y in 0..height {
        for x in 0..width {
            let at = |y| horizontal[y * width + x] as u32;
            let v =
                ((at(y.saturating_sub(1)) + 2 * at(y) + at((y + 1).min(height - 1)) + 2) / 4) as u8;
            blur[y * width + x] = v;
            if v > 0 {
                hist[v as usize] += 1;
                count += 1;
            }
        }
    }
    if count == 0 {
        return Err(MeshError::Empty);
    }
    let rank = (count as f64 * 0.95).ceil() as usize;
    let mut sum = 0;
    let mut p95 = 255;
    for (i, n) in hist.iter().enumerate().skip(1) {
        sum += n;
        if sum >= rank {
            p95 = i;
            break;
        }
    }
    let hard = threshold.max((p95 as f64 * 0.45 + 0.5) as u8);
    let mut mask: Vec<bool> = alpha
        .iter()
        .zip(blur)
        .map(|(&a, b)| a >= threshold && (a >= hard || b >= hard))
        .collect();
    if s.wrap >= 1. {
        close(width, height, &mut mask, s.wrap * 0.5)?;
    }
    // Union pixel squares with merged runs. This replaces the Kotlin raster edge tracer;
    // coordinates remain pixel edges, and the same 0.35/1.0 simplification is applied.
    let mut active = BTreeMap::new();
    let mut rects = vec![];
    for y in 0..=height {
        let mut next = BTreeMap::new();
        if y < height {
            let mut x = 0;
            while x < width {
                if !mask[y * width + x] {
                    x += 1;
                    continue;
                }
                let left = x;
                while x < width && mask[y * width + x] {
                    x += 1
                }
                let start = active.remove(&(left, x)).unwrap_or(y);
                next.insert((left, x), start);
            }
        }
        for ((l, r), start) in active {
            rects.push(
                Rect::new(
                    Coord {
                        x: l as f64,
                        y: start as f64,
                    },
                    Coord {
                        x: r as f64,
                        y: y as f64,
                    },
                )
                .to_polygon(),
            );
        }
        active = next;
        if rects.len() + active.len() > 262144 {
            return Err(MeshError::Limit);
        }
    }
    if rects.is_empty() {
        return Err(MeshError::Empty);
    }
    let polygons = unary_union(&rects);
    let largest = polygons
        .0
        .iter()
        .map(Area::unsigned_area)
        .fold(0., f64::max);
    let minimum = 6_f64.max(24_f64.min(largest * 0.0002));
    let mut result = vec![];
    for p in &polygons.0 {
        if p.unsigned_area() < minimum && p.unsigned_area() < largest {
            continue;
        }
        let p = p.simplify(0.35);
        let mut loops = vec![];
        for (i, r) in std::iter::once(p.exterior())
            .chain(p.interiors())
            .enumerate()
        {
            let pts: Vec<_> = r.0.iter().take(r.0.len() - 1).map(|p| [p.x, p.y]).collect();
            loops.push(orient(pts, i == 0));
        }
        if valid_domain(&loops) {
            result.push(loops)
        } else {
            return Err(MeshError::Geometry("invalid raster contour"));
        }
    }
    Ok(result)
}

fn squared_distance(w: usize, h: usize, mask: &[bool]) -> Vec<f64> {
    let inf = 1e30;
    let mut out = vec![inf; w * h];
    for x in 0..w {
        let mut last = None;
        for y in 0..h {
            if mask[y * w + x] {
                last = Some(y)
            }
            if let Some(k) = last {
                out[y * w + x] = ((y - k) * (y - k)) as f64;
            }
        }
        last = None;
        for y in (0..h).rev() {
            if mask[y * w + x] {
                last = Some(y)
            }
            if let Some(k) = last {
                out[y * w + x] = out[y * w + x].min(((k - y) * (k - y)) as f64);
            }
        }
    }
    let mut v = vec![0usize; w];
    let mut z = vec![0.; w + 1];
    for y in 0..h {
        let f = out[y * w..(y + 1) * w].to_vec();
        let mut k = 0;
        v[0] = 0;
        z[0] = f64::NEG_INFINITY;
        z[1] = f64::INFINITY;
        for q in 1..w {
            let intersection =
                |p: usize| (f[q] + (q * q) as f64 - f[p] - (p * p) as f64) / (2. * (q - p) as f64);
            let mut s = intersection(v[k]);
            while s <= z[k] && k > 0 {
                k -= 1;
                s = intersection(v[k]);
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
        }
        let mut j = 0;
        for q in 0..w {
            while z[j + 1] < (q as f64) {
                j += 1
            }
            let d = q as f64 - v[j] as f64;
            out[y * w + q] = d * d + f[v[j]];
        }
    }
    out
}
fn close(w: usize, h: usize, mask: &mut [bool], radius: f64) -> Result<(), MeshError> {
    let pad = radius.ceil() as usize + 1;
    let pw = w + pad * 2;
    let ph = h + pad * 2;
    if pw * ph > 64 * 1024 * 1024 {
        return Err(MeshError::Limit);
    }
    let mut padded = vec![false; pw * ph];
    for y in 0..h {
        padded[(y + pad) * pw + pad..(y + pad) * pw + pad + w]
            .copy_from_slice(&mask[y * w..(y + 1) * w]);
    }
    let d = squared_distance(pw, ph, &padded);
    let outside: Vec<_> = d.iter().map(|d| *d > radius * radius).collect();
    let d = squared_distance(pw, ph, &outside);
    for y in 0..h {
        for x in 0..w {
            mask[y * w + x] |= d[(y + pad) * pw + x + pad] > radius * radius;
        }
    }
    Ok(())
}

fn turn(a: Point, b: Point, c: Point) -> f64 {
    dot(normal(sub(b, a)), normal(sub(c, b)))
        .clamp(-1., 1.)
        .acos()
}
fn supported_turn(p: &[Point], i: usize, support: f64) -> f64 {
    let n = p.len();
    let mut before = i;
    let mut after = i;
    let mut walked = 0.;
    for _ in 0..n - 1 {
        let next = (before + n - 1) % n;
        walked += distance(p[before], p[next]);
        before = next;
        if walked >= support {
            break;
        }
    }
    walked = 0.;
    for _ in 0..n - 1 {
        let next = (after + 1) % n;
        walked += distance(p[after], p[next]);
        after = next;
        if walked >= support {
            break;
        }
    }
    turn(p[before], p[i], p[after])
}
pub(super) fn interleave(p: &[Point]) -> Vec<Point> {
    (0..p.len())
        .flat_map(|i| [p[i], snap(lerp(p[i], p[(i + 1) % p.len()], 0.5))])
        .collect()
}
pub(super) fn guide(raw: &[Point], spacing: f64) -> Vec<Point> {
    // Coarser controls suppress staircase curvature; exact rings remain for recovery.
    let line: geo::LineString<f64> = raw
        .iter()
        .chain(std::iter::once(&raw[0]))
        .map(|p| (p[0], p[1]))
        .collect();
    let simplified = Polygon::new(line, vec![]).simplify(1.);
    let r = simplified.exterior();
    let mut controls: Vec<_> = r.0[..r.0.len() - 1].iter().map(|p| [p.x, p.y]).collect();
    if controls.len() < 3 {
        controls = raw.to_vec()
    }
    for _ in 0..2 {
        controls = (0..controls.len())
            .map(|i| {
                let a = controls[(i + controls.len() - 1) % controls.len()];
                let b = controls[i];
                let c = controls[(i + 1) % controls.len()];
                let delta = sub(mul(add(add(a, mul(b, 2.)), c), 0.25), b);
                let max_move = 0.55 * (1. - turn(a, b, c) / std::f64::consts::PI * 0.62);
                add(
                    b,
                    mul(
                        delta,
                        (max_move / distance(delta, [0., 0.]).max(EPS)).min(1.),
                    ),
                )
            })
            .collect();
    }
    for strength in [1., 0.55, 0.25, 0.] {
        let sampled = sample_bezier(&controls, spacing, strength);
        if sampled.len() >= 6
            && simple(&sampled)
            && (0..sampled.len())
                .all(|i| distance(sampled[i], sampled[(i + 1) % sampled.len()]) >= 0.075)
        {
            return sampled;
        }
    }
    interleave(raw)
}
fn sample_bezier(p: &[Point], spacing: f64, strength: f64) -> Vec<Point> {
    let n = p.len();
    let support = 10_f64.min(3.5_f64.max(spacing * 0.14));
    let turns: Vec<_> = (0..n).map(|i| supported_turn(p, i, support)).collect();
    let tangents: Vec<_> = (0..n)
        .map(|i| normal(sub(p[(i + 1) % n], p[(i + n - 1) % n])))
        .collect();
    let handles: Vec<_> = (0..n)
        .map(|i| {
            distance(p[(i + n - 1) % n], p[i]).min(distance(p[i], p[(i + 1) % n]))
                * 0.24
                * (1. - turn(p[(i + n - 1) % n], p[i], p[(i + 1) % n]) / std::f64::consts::PI * 0.8)
                * strength
        })
        .collect();
    let mut dense = vec![];
    let mut mandatory = vec![0];
    for i in 0..n {
        if turns[i] >= 0.7 && i != 0 {
            mandatory.push(dense.len())
        }
        let j = (i + 1) % n;
        let chord = distance(p[i], p[j]);
        let out = if turns[i] >= 1.2 {
            normal(sub(p[j], p[i]))
        } else {
            tangents[i]
        };
        let inc = if turns[j] >= 1.2 {
            normal(sub(p[j], p[i]))
        } else {
            tangents[j]
        };
        let c1 = add(p[i], mul(out, handles[i].min(chord / 3.)));
        let c2 = sub(p[j], mul(inc, handles[j].min(chord / 3.)));
        let length = distance(p[i], c1) + distance(c1, c2) + distance(c2, p[j]);
        let count = (length / (spacing * 0.08).clamp(1.25, 3.))
            .ceil()
            .clamp(6., 128.) as usize;
        for k in 0..count {
            let t = k as f64 / count as f64;
            let u = 1. - t;
            dense.push(add(
                add(mul(p[i], u * u * u), mul(c1, 3. * u * u * t)),
                add(mul(c2, 3. * u * t * t), mul(p[j], t * t * t)),
            ));
        }
    }
    let density: Vec<_> = (0..dense.len())
        .map(|i| {
            let a = dense[(i + dense.len() - 1) % dense.len()];
            let b = dense[i];
            let c = dense[(i + 1) % dense.len()];
            let curvature = turn(a, b, c) / ((distance(a, b) + distance(b, c)) * 0.5).max(EPS);
            let target = if curvature <= 1e-7 {
                spacing
            } else {
                (8. * 0.85 / curvature).sqrt()
            };
            spacing / target.clamp(3.5_f64.max(spacing / 12.).min(spacing), spacing)
        })
        .collect();
    let mut cumulative = vec![0.];
    for i in 0..dense.len() {
        cumulative.push(
            cumulative[i]
                + distance(dense[i], dense[(i + 1) % dense.len()])
                    * (density[i] + density[(i + 1) % dense.len()])
                    * 0.5,
        );
    }
    let total = *cumulative.last().unwrap();
    if total <= EPS {
        return vec![];
    }
    let mut step = spacing;
    loop {
        let mut result = vec![];
        for (k, &i) in mandatory.iter().enumerate() {
            let j = mandatory[(k + 1) % mandatory.len()];
            let start = cumulative[i];
            let end = if j > i {
                cumulative[j]
            } else {
                total + cumulative[j]
            };
            let count =
                ((end - start) / step)
                    .ceil()
                    .max(if mandatory.len() == 1 { 3. } else { 1. }) as usize;
            for h in 0..count * 2 {
                let t = (start + (end - start) * h as f64 / (count * 2) as f64) % total;
                let index = cumulative
                    .partition_point(|v| *v <= t)
                    .saturating_sub(1)
                    .min(dense.len() - 1);
                result.push(snap(lerp(
                    dense[index],
                    dense[(index + 1) % dense.len()],
                    (t - cumulative[index]) / (cumulative[index + 1] - cumulative[index]).max(EPS),
                )));
            }
        }
        if result.len() <= 960 || mandatory.len() >= 480 {
            return result;
        }
        step *= 1.1_f64.max(result.len() as f64 / 960.);
    }
}
