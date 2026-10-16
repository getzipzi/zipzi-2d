use super::common::*;
use kasane_sdk::{alpha_mesh_geometry, AlphaMask, AlphaMeshOptions};

#[test]
fn inner_support_rings_do_not_cut_out_the_middle() {
    let alpha = pixels(64, 64, |x, y| {
        u8::from((8..56).contains(&x) && (8..56).contains(&y)) * 255
    });
    let mesh = generate(
        64,
        64,
        &alpha,
        &AlphaMeshOptions {
            inside_margin: 8.0,
            inside_spacing: 12.0,
            ..options()
        },
    );
    valid(&mesh, 64, 64);
    coverage(&mesh, 64, 64, &alpha, 0, true);
    assert!(mesh
        .positions
        .iter()
        .any(|p| p.x == 16.0 && p.y > 16.0 && p.y < 48.0));
    assert!((area(&mesh) - 48.0 * 48.0).abs() < 1e-3);
}

#[test]
fn collapsed_inner_offset_does_not_delete_thin_artwork() {
    let alpha = pixels(32, 32, |x, y| {
        u8::from(x == 15 && (2..30).contains(&y)) * 255
    });
    let mesh = generate(
        32,
        32,
        &alpha,
        &AlphaMeshOptions {
            inside_margin: 14.0,
            ..options()
        },
    );
    coverage(&mesh, 32, 32, &alpha, 0, true);
}

#[test]
fn denser_settings_add_vertices_and_generation_is_repeatable() {
    let alpha = pixels(96, 64, |x, y| {
        u8::from((5..91).contains(&x) && (5..59).contains(&y)) * 255
    });
    let sparse = AlphaMeshOptions {
        outside_spacing: 24.0,
        inside_spacing: 24.0,
        ..options()
    };
    let dense = AlphaMeshOptions {
        outside_spacing: 8.0,
        inside_spacing: 8.0,
        ..options()
    };
    let a = generate(96, 64, &alpha, &sparse);
    let b = generate(96, 64, &alpha, &dense);
    assert!(b.positions.len() > a.positions.len() * 2);
    assert_eq!(b, generate(96, 64, &alpha, &dense));
    assert_eq!(a, generate(96, 64, &alpha, &sparse));
}

#[test]
fn minimum_points_applies_even_to_tiny_boundaries() {
    let mesh = generate(
        1,
        1,
        &[255],
        &AlphaMeshOptions {
            minimum_boundary_points: 10,
            ..options()
        },
    );
    let boundary_points = mesh
        .positions
        .iter()
        .filter(|p| p.x == 0.0 || p.y == 0.0 || p.x == 1.0 || p.y == 1.0)
        .count();
    assert!(boundary_points >= 10);
    valid(&mesh, 1, 1);
}

#[test]
fn diagonal_contacts_and_checkerboards_keep_their_pixel_area() {
    for size in [2, 3, 8] {
        let alpha = pixels(size, size, |x, y| u8::from((x + y) % 2 == 0) * 255);
        let mesh = generate(size, size, &alpha, &options());
        valid(&mesh, size, size);
        coverage(&mesh, size, size, &alpha, 0, true);
        assert!((area(&mesh) - alpha.iter().filter(|&&v| v > 0).count() as f64).abs() < 1e-4);
    }
}

#[test]
fn curved_and_sharp_shapes_keep_coverage_with_all_presets() {
    let alpha = pixels(128, 128, |x, y| {
        let dx = x as f64 - 64.0;
        let dy = y as f64 - 64.0;
        u8::from(dx * dx + dy * dy < 44.0 * 44.0 || (x > 63 && x < 110 && y > 63 && y < x)) * 255
    });
    for opts in [
        AlphaMeshOptions::standard(),
        AlphaMeshOptions::deformation_small(),
        AlphaMeshOptions::deformation_large(),
    ] {
        let mesh = generate(128, 128, &alpha, &opts);
        valid(&mesh, 128, 128);
        coverage(&mesh, 128, 128, &alpha, 0, false);
    }
}

#[test]
fn malformed_empty_and_excessive_inputs_return_errors() {
    let call = |w, h, a: &[u8], o: &AlphaMeshOptions| {
        alpha_mesh_geometry(
            AlphaMask {
                width: w,
                height: h,
                alpha: a,
            },
            o,
        )
        .unwrap_err()
        .code
        .to_string()
    };
    assert_eq!(call(0, 0, &[], &options()), "INVALID_ALPHA_MASK");
    assert_eq!(call(2, 2, &[255], &options()), "INVALID_ALPHA_MASK");
    assert_eq!(
        call(u32::MAX, u32::MAX, &[], &options()),
        "INVALID_ALPHA_MASK"
    );
    assert_eq!(call(1, 1, &[0], &options()), "EMPTY_ALPHA_MASK");
    assert_eq!(
        call(
            1,
            1,
            &[255],
            &AlphaMeshOptions {
                alpha_threshold: 255,
                ..options()
            }
        ),
        "EMPTY_ALPHA_MASK"
    );
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e100] {
        assert_eq!(
            call(
                1,
                1,
                &[255],
                &AlphaMeshOptions {
                    inside_spacing: value,
                    ..options()
                }
            ),
            "INVALID_ALPHA_MESH_OPTIONS"
        );
    }
    assert_eq!(
        call(
            1,
            1,
            &[255],
            &AlphaMeshOptions {
                minimum_margin: 1.0,
                ..options()
            }
        ),
        "INVALID_ALPHA_MESH_OPTIONS"
    );
    assert_eq!(
        call(
            1,
            1,
            &[255],
            &AlphaMeshOptions {
                max_vertices: 3,
                minimum_boundary_points: 3,
                ..options()
            }
        ),
        "ALPHA_MESH_LIMIT"
    );
    for inside_spacing in [1e-10, 1e-30, 1e-300] {
        assert_eq!(
            call(
                1,
                1,
                &[255],
                &AlphaMeshOptions {
                    inside_spacing,
                    ..options()
                }
            ),
            "ALPHA_MESH_LIMIT"
        );
    }
}

#[test]
fn many_small_masks_preserve_area_and_foreground_without_panics() {
    // Fixed PRNG seed; exercise arbitrary holes, islands and point contacts.
    let mut state = 0x12ab34cdu32;
    for _ in 0..64 {
        let alpha: Vec<_> = (0..64)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if state & 1 == 0 {
                    0
                } else {
                    255
                }
            })
            .collect();
        let mesh = generate(8, 8, &alpha, &options());
        valid(&mesh, 8, 8);
        coverage(&mesh, 8, 8, &alpha, 0, true);
        assert!((area(&mesh) - alpha.iter().filter(|&&v| v > 0).count() as f64).abs() < 1e-4);
    }
}
