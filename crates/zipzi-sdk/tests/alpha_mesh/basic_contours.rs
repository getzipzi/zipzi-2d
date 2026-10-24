use super::common::*;
use std::collections::BTreeMap;
use zipzi_sdk::AlphaMeshOptions;

#[test]
fn opaque_rectangle_has_correct_area_uvs_and_closed_topology() {
    let alpha = vec![255; 32 * 20];
    let mesh = generate(32, 20, &alpha, &options());
    valid(&mesh, 32, 20);
    assert!((area(&mesh) - 640.0).abs() < 1e-4);
    coverage(&mesh, 32, 20, &alpha, 0, true);
}

#[test]
fn half_pixel_padding_repairs_self_touching_offset_contours() {
    let mut alpha = vec![0; 32 * 32];
    for (x, y) in [
        (9, 20),
        (11, 20),
        (14, 21),
        (11, 22),
        (12, 22),
        (30, 22),
        (14, 23),
        (16, 23),
        (28, 23),
        (11, 24),
        (29, 24),
        (11, 25),
        (13, 25),
        (14, 25),
        (8, 27),
        (10, 27),
        (11, 27),
    ] {
        alpha[y * 32 + x] = 255;
    }
    for preserve_holes in [false, true] {
        for minimum_margin in [0.0, 0.5] {
            let opts = AlphaMeshOptions {
                outside_spacing: 8.0,
                inside_spacing: 8.0,
                outside_margin: 0.5,
                minimum_margin,
                inside_margin: 0.0,
                clip_to_image: false,
                preserve_holes,
                ..AlphaMeshOptions::standard()
            };
            let mesh = generate(32, 32, &alpha, &opts);
            valid(&mesh, 32, 32);
            coverage(&mesh, 32, 32, &alpha, 0, false);
            assert_eq!(mesh, generate(32, 32, &alpha, &opts));
        }
    }
}

#[test]
fn separated_islands_at_vertex_budget_keep_exact_pixel_coverage() {
    let alpha = pixels(
        512,
        512,
        |x, y| if x % 4 == 0 && y % 4 == 0 { 255 } else { 0 },
    );
    let opts = AlphaMeshOptions {
        outside_spacing: 85.0,
        inside_spacing: 85.0,
        ..options()
    };
    let start = std::time::Instant::now();
    let mesh = generate(512, 512, &alpha, &opts);
    eprintln!("16384 islands: {:?}", start.elapsed());
    assert_eq!(mesh.positions.len(), 65_536);
    assert_eq!(mesh.triangles.len(), 32_768);
    valid(&mesh, 512, 512);
    assert_eq!(area(&mesh), 16_384.0);
    // Each unit square gets exactly two triangles, with no bridges across gaps.
    let mut faces_per_pixel = BTreeMap::new();
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        let x = ((p[0].x + p[1].x + p[2].x) / 3.0).floor() as u32;
        let y = ((p[0].y + p[1].y + p[2].y) / 3.0).floor() as u32;
        assert_eq!(alpha[((511 - y) * 512 + x) as usize], 255);
        assert!(p.iter().all(|p| p.x >= x as f32
            && p.x <= (x + 1) as f32
            && p.y >= y as f32
            && p.y <= (y + 1) as f32));
        *faces_per_pixel.entry((x, y)).or_insert(0) += 1;
    }
    assert_eq!(faces_per_pixel.len(), 16_384);
    assert!(faces_per_pixel.values().all(|&count| count == 2));
}

#[test]
fn single_pixel_thin_lines_concavity_and_separate_islands_survive() {
    let alpha = pixels(24, 24, |x, y| {
        u8::from(
            (x == 2 && y == 3)
                || (x == 5 && (2..21).contains(&y))
                || ((10..20).contains(&x)
                    && ((3..6).contains(&y) || (x < 13 && (3..15).contains(&y))))
                || (x == 21 && y == 20),
        ) * 255
    });
    let mesh = generate(24, 24, &alpha, &options());
    valid(&mesh, 24, 24);
    coverage(&mesh, 24, 24, &alpha, 0, true);
    assert!((area(&mesh) - alpha.iter().filter(|&&v| v > 0).count() as f64).abs() < 1e-4);
    for t in &mesh.triangles {
        let p = t.map(|i| mesh.positions[i as usize]);
        assert!((p[0].x - p[1].x).abs() <= 10.0, "bridged separate islands");
    }
}

#[test]
fn holes_and_nested_islands_are_preserved_or_explicitly_filled() {
    let alpha = pixels(40, 40, |x, y| {
        u8::from(
            (3..37).contains(&x)
                && (3..37).contains(&y)
                && (!((10..30).contains(&x) && (10..30).contains(&y))
                    || ((17..23).contains(&x) && (17..23).contains(&y))),
        ) * 255
    });
    let preserved = generate(40, 40, &alpha, &options());
    valid(&preserved, 40, 40);
    coverage(&preserved, 40, 40, &alpha, 0, true);
    let filled = generate(
        40,
        40,
        &alpha,
        &AlphaMeshOptions {
            preserve_holes: false,
            ..options()
        },
    );
    assert!((area(&filled) - 34.0 * 34.0).abs() < 1e-3);
    assert!(covers(&filled, 12.0, 12.0));
}

#[test]
fn threshold_is_inclusive_and_filters_dust_without_removing_opaque_specks() {
    let alpha = pixels(16, 16, |x, y| match (x, y) {
        (1, 1) => 10,
        (4, 4) => 11,
        (8, 8) => 255,
        _ => 0,
    });
    let mesh = generate(
        16,
        16,
        &alpha,
        &AlphaMeshOptions {
            alpha_threshold: 10,
            ..options()
        },
    );
    valid(&mesh, 16, 16);
    coverage(&mesh, 16, 16, &alpha, 10, true);
    assert!((area(&mesh) - 2.0).abs() < 1e-5);
}

#[test]
fn pixel_rows_are_flipped_once_and_rectangular_images_keep_pixel_aspect() {
    let alpha = pixels(40, 12, |x, y| u8::from(x < 8 && y < 3) * 255);
    let mesh = generate(40, 12, &alpha, &options());
    assert!(mesh.positions.iter().all(|p| p.x <= 8.0 && p.y >= 9.0));
    valid(&mesh, 40, 12);
    coverage(&mesh, 40, 12, &alpha, 0, true);
}

#[test]
fn margin_expands_coverage_and_clips_at_image_edges() {
    let alpha = pixels(40, 40, |x, y| {
        u8::from((10..30).contains(&x) && (10..30).contains(&y)) * 255
    });
    let opts = AlphaMeshOptions {
        outside_margin: 4.0,
        minimum_margin: 2.0,
        inside_margin: 3.0,
        ..options()
    };
    let mesh = generate(40, 40, &alpha, &opts);
    valid(&mesh, 40, 40);
    coverage(&mesh, 40, 40, &alpha, 0, false);
    for x in 8..32 {
        for y in 10..30 {
            assert!(covers(&mesh, x as f32 + 0.01, y as f32 + 0.01));
        }
    }
    assert!(area(&mesh) > 600.0);
    let full = vec![255; 40 * 40];
    let clipped = generate(40, 40, &full, &opts);
    valid(&clipped, 40, 40);
    assert!((area(&clipped) - 1600.0).abs() < 1e-3);
}
