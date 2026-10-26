use super::common::*;
use std::collections::{BTreeMap, BTreeSet};
use zipzi_sdk::{
    build_edge_layout, sample_interior, triangulate_constrained, AlphaMask, AlphaMeshOptions,
};

#[test]
fn staged_edge_and_interior_generation_matches_the_composed_output() {
    let alpha = pixels(160, 160, |x, y| {
        u8::from((20..140).contains(&x) && (20..140).contains(&y)) * 255
    });
    let mask = AlphaMask {
        width: 160,
        height: 160,
        alpha: &alpha,
    };
    let options = AlphaMeshOptions {
        outside_spacing: 20.,
        inside_spacing: 20.,
        ..AlphaMeshOptions::standard()
    };
    let layout = build_edge_layout(mask, &options).unwrap();
    let edges = layout.preview_geometry().unwrap();
    assert!(edges
        .meshing
        .as_ref()
        .is_some_and(|m| !m.constraints.is_empty()));
    let interior = sample_interior(layout).unwrap();
    assert!(!interior.added_uvs().is_empty());
    let staged = triangulate_constrained(interior).unwrap();
    assert!(edges.vertex_ids.len() < staged.vertex_ids.len());
    assert_eq!(staged, generate(160, 160, &alpha, &options));
}

#[test]
fn continuous_inner_ring_separates_boundary_from_deep_vertices() {
    let alpha = pixels(256, 256, |x, y| {
        u8::from((32..224).contains(&x) && (32..224).contains(&y)) * 255
    });
    let mesh = generate(256, 256, &alpha, &AlphaMeshOptions::deformation_small());
    valid(&mesh, 256, 256);
    let meshing = mesh
        .meshing
        .as_ref()
        .expect("single region preserves structure");
    assert!(!meshing.constraints.is_empty());
    assert!(meshing
        .constraints
        .iter()
        .all(|constraint| { constraint.kind == zipzi_core::MeshConstraintKind::EdgeSupport }));
    let edges = mesh_edges(&mesh);
    let boundary: BTreeSet<_> = edges
        .iter()
        .filter(|(_, n)| **n == 1)
        .flat_map(|(&(a, b), _)| [a, b])
        .collect();
    // The inset rectangle is [46,210]^2. No boundary edge may skip its ring
    // and reach a vertex strictly inside that rectangle.
    for &(a, b) in edges.keys() {
        for (outer, other) in [(a, b), (b, a)] {
            let p = mesh.positions[other as usize];
            if boundary.contains(&outer) {
                assert!(
                    !(p.x > 46.01 && p.x < 209.99 && p.y > 46.01 && p.y < 209.99),
                    "boundary bypassed the inner ring: {p:?}"
                );
            }
        }
    }
    // Every inset side is a continuous chain of mesh edges (with shared corners).
    for axis in 0..2 {
        for side in [46.0, 210.0] {
            let mut length = 0.0;
            for &(a, b) in edges.keys() {
                let a = mesh.positions[a as usize];
                let b = mesh.positions[b as usize];
                let (u, v) = if axis == 0 {
                    ([a.x, a.y], [b.x, b.y])
                } else {
                    ([a.y, a.x], [b.y, b.x])
                };
                if (u[0] - side).abs() < 0.001
                    && (v[0] - side).abs() < 0.001
                    && (46.0..=210.0).contains(&u[1])
                    && (46.0..=210.0).contains(&v[1])
                {
                    length += (u[1] - v[1]).abs();
                }
            }
            assert!((length - 164.0).abs() < 0.01, "broken inset side: {length}");
        }
    }
    assert!(covers(&mesh, 128.0, 128.0), "inner ring became a hole");
}

#[test]
fn collapsed_insets_have_distributed_support_in_isolated_and_attached_strips() {
    for attached in [false, true] {
        let alpha = pixels(256, 200, |x, y| {
            u8::from(
                ((20..236).contains(&x) && (96..104).contains(&y))
                    || (attached && (20..120).contains(&x) && (20..180).contains(&y)),
            ) * 255
        });
        let mesh = generate(256, 200, &alpha, &AlphaMeshOptions::deformation_small());
        valid(&mesh, 256, 200);
        for (left, right) in [(130.0, 190.0), (190.0, 236.0)] {
            assert!(
                mesh.positions
                    .iter()
                    .any(|p| p.x >= left && p.x <= right && p.y > 96.0 && p.y < 104.0),
                "unsupported strip span {left}..{right}, attached={attached}"
            );
        }
        coverage(&mesh, 256, 200, &alpha, 0, false);
        assert_eq!(
            mesh,
            generate(256, 200, &alpha, &AlphaMeshOptions::deformation_small())
        );
    }
}

#[test]
fn internal_constraints_preserve_real_holes_and_nested_islands() {
    let alpha = pixels(80, 80, |x, y| {
        u8::from(
            (4..76).contains(&x)
                && (4..76).contains(&y)
                && (!((24..56).contains(&x) && (24..56).contains(&y))
                    || ((36..44).contains(&x) && (36..44).contains(&y))),
        ) * 255
    });
    let mesh = generate(
        80,
        80,
        &alpha,
        &AlphaMeshOptions {
            inside_margin: 6.0,
            inside_spacing: 12.0,
            outside_spacing: 12.0,
            ..options()
        },
    );
    valid(&mesh, 80, 80);
    coverage(&mesh, 80, 80, &alpha, 0, true);
}

#[test]
fn dense_interior_retains_equilateral_lattice() {
    let alpha = pixels(512, 512, |x, y| {
        u8::from((20..492).contains(&x) && (20..492).contains(&y)) * 255
    });
    // Exercise an explicit dense lattice independently of UI preset tuning.
    let mesh = generate(
        512,
        512,
        &alpha,
        &AlphaMeshOptions {
            outside_spacing: 25.0,
            inside_spacing: 25.0,
            ..AlphaMeshOptions::deformation_large()
        },
    );
    valid(&mesh, 512, 512);
    let mut count = 0;
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        if p.iter()
            .all(|p| (80.0..432.0).contains(&p.x) && (80.0..432.0).contains(&p.y))
        {
            for i in 0..3 {
                let a = p[i];
                let b = p[(i + 1) % 3];
                assert!(((a.x - b.x).hypot(a.y - b.y) - 25.0).abs() < 0.001);
            }
            count += 1;
        }
    }
    assert!(
        count > 300,
        "dense core has only {count} equilateral triangles"
    );
}

#[test]
fn changing_component_bounds_keeps_the_common_interior_lattice() {
    let core = |left| {
        let alpha = pixels(256, 256, |x, y| {
            u8::from((left..240).contains(&x) && (left..240).contains(&y)) * 255
        });
        let mesh = generate(
            256,
            256,
            &alpha,
            &AlphaMeshOptions {
                inside_spacing: 20.0,
                preserve_holes: true,
                ..AlphaMeshOptions::standard()
            },
        );
        valid(&mesh, 256, 256);
        mesh.positions
            .iter()
            .filter(|p| (64.0..192.0).contains(&p.x) && (64.0..192.0).contains(&p.y))
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .collect::<BTreeSet<_>>()
    };
    let original = core(16);
    assert!(original.len() > 30);
    assert_eq!(original, core(19));
}

#[test]
fn shared_clearance_preserves_support_on_every_thin_branch() {
    let alpha = pixels(256, 256, |x, y| {
        u8::from(
            ((20..236).contains(&x) && (124..132).contains(&y))
                || ((124..132).contains(&x) && (20..236).contains(&y)),
        ) * 255
    });
    let mesh = generate(
        256,
        256,
        &alpha,
        &AlphaMeshOptions {
            inside_spacing: 40.0,
            preserve_holes: true,
            ..AlphaMeshOptions::standard()
        },
    );
    valid(&mesh, 256, 256);
    coverage(&mesh, 256, 256, &alpha, 0, false);
    let boundary: BTreeSet<_> = mesh_edges(&mesh)
        .into_iter()
        .filter(|(_, n)| *n == 1)
        .flat_map(|((a, b), _)| [a, b])
        .collect();
    let support: Vec<_> = mesh
        .vertex_ids
        .iter()
        .filter(|id| !boundary.contains(id))
        .collect();
    assert!(support.len() >= 8, "thin branches lost their support");
    // Required endpoints may be closer than the optional-site clearance.
    // Verify that filtering still leaves local support on all four branches.
    for (horizontal, low, high) in [
        (true, 20., 100.),
        (true, 156., 236.),
        (false, 20., 100.),
        (false, 156., 236.),
    ] {
        assert!(
            support.iter().any(|&&id| {
                let p = mesh.positions[id as usize];
                let (along, across) = if horizontal { (p.x, p.y) } else { (p.y, p.x) };
                along > low && along < high && across > 124. && across < 132.
            }),
            "unsupported branch: horizontal={horizontal}, {low}..{high}"
        );
    }
}

#[test]
fn spanned_large_holes_keep_island_support_without_a_void_lattice() {
    let alpha = pixels(128, 128, |x, y| {
        u8::from(
            (12..116).contains(&x)
                && (12..116).contains(&y)
                && (!((32..96).contains(&x) && (32..96).contains(&y))
                    || ((60..68).contains(&x) && (60..68).contains(&y))),
        ) * 255
    });
    let opts = AlphaMeshOptions::deformation_large();
    assert!(!opts.preserve_holes);
    let mesh = generate(128, 128, &alpha, &opts);
    valid(&mesh, 128, 128);
    assert!(
        covers(&mesh, 48.0, 64.0),
        "transparent hole was not spanned"
    );
    assert!(
        mesh.positions
            .iter()
            .any(|p| (60.0..68.0).contains(&p.x) && (60.0..68.0).contains(&p.y)),
        "foreground island inside the hole lost all support"
    );
    for p in &mesh.positions {
        if (40.0..88.0).contains(&p.x) && (40.0..88.0).contains(&p.y) {
            assert!(
                (58.0..70.0).contains(&p.x) && (58.0..70.0).contains(&p.y),
                "unnecessary lattice vertex inside large transparent hole: {p:?}"
            );
        }
    }
    coverage(&mesh, 128, 128, &alpha, 0, false);
    let preserved = generate(
        128,
        128,
        &alpha,
        &AlphaMeshOptions {
            preserve_holes: true,
            ..opts
        },
    );
    assert!(!covers(&preserved, 48.0, 64.0));
    assert!(covers(&preserved, 64.0, 64.0));
}

#[test]
fn sub_spacing_holes_do_not_add_dense_internal_rings() {
    let filled = pixels(160, 160, |x, y| {
        u8::from((12..148).contains(&x) && (12..148).contains(&y)) * 255
    });
    let perforated = pixels(160, 160, |x, y| {
        if (72..80).contains(&x) && (60..100).contains(&y) {
            0
        } else {
            filled[(y * 160 + x) as usize]
        }
    });
    let opts = AlphaMeshOptions::deformation_large();
    let expected = generate(160, 160, &filled, &opts);
    let mesh = generate(160, 160, &perforated, &opts);
    valid(&mesh, 160, 160);
    assert_eq!(
        mesh, expected,
        "a sub-spacing hole changed the interior support layout"
    );
}

#[test]
fn tiny_island_in_a_spanned_hole_gets_a_local_support_vertex() {
    let alpha = pixels(128, 128, |x, y| {
        u8::from(
            ((12..116).contains(&x)
                && (12..116).contains(&y)
                && !((32..96).contains(&x) && (32..96).contains(&y)))
                || (x == 64 && y == 64),
        ) * 255
    });
    let mesh = generate(128, 128, &alpha, &AlphaMeshOptions::deformation_large());
    valid(&mesh, 128, 128);
    assert!(
        mesh.positions
            .iter()
            .any(|p| p.x > 64.0 && p.x < 65.0 && p.y > 63.0 && p.y < 64.0),
        "covered island has no local deformation support"
    );
}

#[test]
fn cropped_diagonal_artwork_does_not_densify_the_whole_boundary() {
    let (width, height) = (512, 256);
    let alpha = pixels(width, height, |x, y| {
        let taper = 0.38 * height as f64 * (1. - ((x as f64 + 0.5) / width as f64 * 2. - 1.).abs());
        if y as f64 + 0.5 >= taper && (y as f64 + 0.5) < height as f64 - taper {
            255
        } else {
            0
        }
    });
    let opts = AlphaMeshOptions {
        minimum_boundary_points: 15,
        ..AlphaMeshOptions::deformation_small()
    };
    let mesh = generate(width, height, &alpha, &opts);
    valid(&mesh, width, height);
    let boundary = mesh_edges(&mesh).values().filter(|&&n| n == 1).count();
    assert!(
        boundary < 60,
        "clipping forced {boundary} boundary samples at spacing 80"
    );
    for y in 0..height {
        for x in 0..width {
            if alpha[(y * width + x) as usize] != 0 {
                assert!(covers(
                    &mesh,
                    x as f32 + 0.5,
                    height as f32 - y as f32 - 0.5
                ));
            }
        }
    }
}

#[test]
fn target_spacing_does_not_bisect_every_slightly_longer_edge() {
    let alpha = pixels(128, 128, |x, y| {
        u8::from((20..101).contains(&x) && (20..101).contains(&y)) * 255
    });
    let mesh = generate(
        128,
        128,
        &alpha,
        &AlphaMeshOptions {
            outside_spacing: 80.,
            inside_spacing: 1000.,
            ..options()
        },
    );
    valid(&mesh, 128, 128);
    coverage(&mesh, 128, 128, &alpha, 0, true);
    let lengths: Vec<_> = mesh_edges(&mesh)
        .iter()
        .filter(|(_, n)| **n == 1)
        .map(|(&(a, b), _)| {
            let (a, b) = (mesh.positions[a as usize], mesh.positions[b as usize]);
            (a.x - b.x).hypot(a.y - b.y)
        })
        .collect();
    assert!(
        lengths.iter().all(|&length| length > 60. && length < 100.),
        "oversampled ring: {lengths:?}"
    );
}

#[test]
fn smooth_circle_samples_follow_spacing_without_losing_padding() {
    let alpha = pixels(256, 256, |x, y| {
        if (x as f64 + 0.5 - 128.0).hypot(y as f64 + 0.5 - 128.0) <= 115.0 {
            255
        } else {
            0
        }
    });
    let options = AlphaMeshOptions {
        outside_spacing: 40.0,
        inside_spacing: 40.0,
        outside_margin: 7.0,
        inside_margin: 7.0,
        minimum_margin: 2.5,
        minimum_boundary_points: 15,
        ..AlphaMeshOptions::deformation_small()
    };
    let mesh = generate(256, 256, &alpha, &options);
    valid(&mesh, 256, 256);
    coverage(&mesh, 256, 256, &alpha, 0, false);
    let mut edges = BTreeMap::new();
    for t in &mesh.triangles {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            *edges.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
    }
    let boundary: Vec<_> = edges.iter().filter(|(_, n)| **n == 1).collect();
    // Circumference / target spacing is about 20. RDP used to leave 16
    // slightly-long edges, each split again, producing 32 boundary vertices.
    assert!(
        (19..=23).contains(&boundary.len()),
        "boundary has {} edges",
        boundary.len()
    );
    let mean_radius = boundary
        .iter()
        .map(|((a, _), _)| {
            let p = mesh.positions[*a as usize];
            (p.x - 128.0).hypot(p.y - 128.0)
        })
        .sum::<f32>()
        / boundary.len() as f32;
    assert!(
        (121.4..=123.0).contains(&mean_radius),
        "resampling shrank the requested offset: radius {mean_radius}"
    );
    for (&(a, b), _) in boundary {
        let a = mesh.positions[a as usize];
        let b = mesh.positions[b as usize];
        let length = (a.x - b.x).hypot(a.y - b.y);
        assert!(
            (24.0..=44.0).contains(&length),
            "uneven smooth contour edge {length}"
        );
    }
    for degree in 0..360 {
        let angle = (degree as f32).to_radians();
        assert!(
            covers(
                &mesh,
                128.0 + 116.5 * angle.cos(),
                128.0 + 116.5 * angle.sin()
            ),
            "minimum padding cut at {degree} degrees"
        );
    }
    let repeated = generate(256, 256, &alpha, &options);
    assert_eq!(mesh, repeated);
    let dense = generate(
        256,
        256,
        &alpha,
        &AlphaMeshOptions {
            minimum_boundary_points: 48,
            ..options
        },
    );
    valid(&dense, 256, 256);
    assert!(dense.positions.len() > mesh.positions.len());
}
