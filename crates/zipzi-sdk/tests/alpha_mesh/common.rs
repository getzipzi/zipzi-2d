use std::collections::{BTreeMap, BTreeSet};

use zipzi_core::Vec2;
use zipzi_sdk::{alpha_mesh_geometry, AlphaMask, AlphaMeshOptions, MeshGeometry};

pub fn pixels(width: u32, height: u32, f: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| f(x, y))
        .collect()
}

pub fn options() -> AlphaMeshOptions {
    AlphaMeshOptions {
        outside_spacing: 5.0,
        inside_spacing: 6.0,
        outside_margin: 0.0,
        inside_margin: 0.0,
        minimum_margin: 0.0,
        minimum_boundary_points: 4,
        preserve_holes: true,
        ..AlphaMeshOptions::default()
    }
}

pub fn generate(width: u32, height: u32, alpha: &[u8], options: &AlphaMeshOptions) -> MeshGeometry {
    alpha_mesh_geometry(
        AlphaMask {
            width,
            height,
            alpha,
        },
        options,
    )
    .unwrap()
}

pub fn cross(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
        - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64)
}

pub fn covers(mesh: &MeshGeometry, x: f32, y: f32) -> bool {
    let p = Vec2::new(x, y);
    mesh.triangles.iter().any(|t| {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        cross(a, b, p) >= -1e-5 && cross(b, c, p) >= -1e-5 && cross(c, a, p) >= -1e-5
    })
}

pub fn area(mesh: &MeshGeometry) -> f64 {
    mesh.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
            cross(a, b, c) * 0.5
        })
        .sum()
}

/// Independent checks on exported geometry, including manifold boundary cycles.
pub fn valid(mesh: &MeshGeometry, width: u32, height: u32) {
    assert!(!mesh.triangles.is_empty());
    assert_eq!(mesh.positions.len(), mesh.uvs.len());
    assert_eq!(
        mesh.vertex_ids,
        (0..mesh.positions.len() as u32).collect::<Vec<_>>()
    );
    let mut edges = BTreeMap::<_, Vec<_>>::new();
    let mut used = BTreeSet::new();
    for t in &mesh.triangles {
        assert!(t.iter().all(|&i| (i as usize) < mesh.positions.len()));
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        assert!(cross(a, b, c) > 0.0, "degenerate or reversed triangle");
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push((a, b));
            used.insert(a);
        }
    }
    assert_eq!(used.len(), mesh.positions.len(), "unused vertices");
    let mut degree = BTreeMap::<_, (usize, usize)>::new();
    for directed in edges.values() {
        assert!(directed.len() <= 2, "non-manifold edge");
        if directed.len() == 2 {
            assert_eq!(directed[0], (directed[1].1, directed[1].0));
        } else {
            let (a, b) = directed[0];
            degree.entry(a).or_default().0 += 1;
            degree.entry(b).or_default().1 += 1;
        }
    }
    // Point-touching components can share a vertex, but every incoming boundary
    // edge must still have an outgoing edge (no open boundary).
    assert!(degree.values().all(|&(a, b)| a > 0 && a == b));
    for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
        assert!(p.x.is_finite() && p.y.is_finite());
        assert!((0.0..=width as f32).contains(&p.x));
        assert!((0.0..=height as f32).contains(&p.y));
        assert!((0.0..=1.0).contains(&uv.x) && (0.0..=1.0).contains(&uv.y));
        assert_eq!(uv.x, p.x / width as f32);
        assert_eq!(uv.y, p.y / height as f32);
    }
}

pub fn coverage(
    mesh: &MeshGeometry,
    width: u32,
    height: u32,
    alpha: &[u8],
    threshold: u8,
    exact: bool,
) {
    for row in 0..height {
        for x in 0..width {
            let opaque = alpha[(row * width + x) as usize] > threshold;
            if opaque || exact {
                // Probe multiple positions within pixel squares, not just centers.
                for (dx, dy) in [(0.05, 0.05), (0.5, 0.5), (0.95, 0.95)] {
                    assert_eq!(
                        covers(mesh, x as f32 + dx, height as f32 - row as f32 - dy),
                        opaque,
                        "pixel ({x},{row}) probe ({dx},{dy})"
                    );
                }
            }
        }
    }
}

pub fn mesh_edges(mesh: &MeshGeometry) -> BTreeMap<(u32, u32), usize> {
    let mut edges = BTreeMap::new();
    for t in &mesh.triangles {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    edges
}
