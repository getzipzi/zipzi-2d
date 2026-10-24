use super::common::*;
use zipzi_core::Vec2;
use zipzi_sdk::AlphaMeshOptions;

#[test]
fn padding_can_merge_islands_and_close_holes_without_losing_foreground() {
    let alpha = pixels(32, 32, |x, y| {
        u8::from(
            (3..29).contains(&y)
                && ((3..14).contains(&x) || (17..29).contains(&x))
                && !(x == 8 && y == 8),
        ) * 255
    });
    let mesh = generate(
        32,
        32,
        &alpha,
        &AlphaMeshOptions {
            outside_margin: 3.0,
            minimum_margin: 1.0,
            inside_margin: 2.0,
            ..options()
        },
    );
    valid(&mesh, 32, 32);
    coverage(&mesh, 32, 32, &alpha, 0, false);
    assert!(covers(&mesh, 15.5, 16.5));
    assert!(covers(&mesh, 8.5, 23.5));
}

#[test]
fn minimum_margin_survives_simplification_at_sharp_corners() {
    let alpha = pixels(48, 48, |x, y| {
        u8::from((10..38).contains(&x) && (10..38).contains(&y) && x + y < 51) * 255
    });
    let opts = AlphaMeshOptions {
        outside_spacing: 40.0,
        outside_margin: 5.0,
        minimum_margin: 3.0,
        ..options()
    };
    let mesh = generate(48, 48, &alpha, &opts);
    valid(&mesh, 48, 48);
    for row in 0..48 {
        for x in 0..48 {
            if alpha[(row * 48 + x) as usize] == 0 {
                continue;
            }
            for (dx, dy) in [(2.8, 0.0), (-2.8, 0.0), (0.0, 2.8), (0.0, -2.8)] {
                assert!(covers(
                    &mesh,
                    x as f32 + 0.5 + dx,
                    48.0 - row as f32 - 0.5 + dy
                ));
            }
        }
    }
}

#[test]
fn representative_padded_fragmented_masks_do_not_create_degenerate_meshes() {
    use super::mask_cases::*;
    for (case, alpha) in fragmented_masks().filter(|(case, _)| FRAGMENTED_DAILY.contains(case)) {
        check_fragmented(case, &alpha);
    }
}

#[test]
fn generated_geometry_is_accepted_by_sdk_and_undo_redo_restores_it() {
    use zipzi_core::Canvas;
    use zipzi_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession, TopologyReplacement};
    const DOCUMENT: &str = "00000000-0000-4000-8000-000000000701";
    const ASSET: &str = "00000000-0000-4000-8000-000000000702";
    const MESH: &str = "00000000-0000-4000-8000-000000000703";
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET, "source", &path).unwrap();
    let original = rectangle_mesh(
        MESH,
        "source",
        ASSET,
        Vec2::new(0.0, 0.0),
        Vec2::new(2.0, 2.0),
    )
    .unwrap();
    let mut session =
        AuthoringSession::new(DOCUMENT, Canvas::new(2.0, 2.0, Vec2::new(1.0, 1.0), 1.0)).unwrap();
    session
        .edit("source", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(original.clone())
        })
        .unwrap();
    let original = session.mesh(MESH).unwrap();
    let snapshot = session.geometry(MESH).unwrap();
    let geometry = generate(2, 2, &[255, 0, 255, 255], &options());
    let mut replacement = original.clone();
    replacement.vertex_ids = geometry.vertex_ids;
    replacement.base_positions = geometry.positions;
    replacement.uvs = geometry.uvs;
    replacement.triangles = geometry.triangles;
    session
        .edit("alpha topology", Some(snapshot.version), |edit| {
            edit.replace_topology(
                &snapshot,
                TopologyReplacement {
                    mesh: replacement.clone(),
                    binding: None,
                    blend_bindings: vec![],
                    glues: vec![],
                    vertex_mapping: snapshot.vertex_ids.iter().map(|&id| (id, None)).collect(),
                },
            )
        })
        .unwrap();
    assert!(session.validate_structure().is_empty());
    assert_eq!(session.mesh(MESH).unwrap(), replacement);
    session.undo().unwrap();
    assert_eq!(session.mesh(MESH).unwrap(), original);
    session.redo().unwrap();
    assert_eq!(session.mesh(MESH).unwrap(), replacement);
}

#[test]
fn large_asymmetric_image_uses_local_pixel_geometry_without_atlas_scaling() {
    let alpha = pixels(4096, 1024, |x, y| {
        u8::from((3700..4050).contains(&x) && (27..977).contains(&y)) * 255
    });
    let opts = AlphaMeshOptions {
        inside_spacing: 60.0,
        outside_spacing: 40.0,
        outside_margin: 3.0,
        minimum_margin: 2.0,
        inside_margin: 2.0,
        ..options()
    };
    let mesh = generate(4096, 1024, &alpha, &opts);
    valid(&mesh, 4096, 1024);
    let min_x = mesh
        .positions
        .iter()
        .map(|p| p.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = mesh
        .positions
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x - 3697.0).abs() < 0.001);
    assert!((max_x - 4053.0).abs() < 0.001);
    assert!(mesh.positions.len() < 5000);
    assert!(covers(&mesh, 3700.1, 47.1));
    assert!(covers(&mesh, 4049.9, 996.9));
}

#[test]
fn unbounded_margins_cover_edge_artwork_without_flattening_the_ring() {
    let opts = AlphaMeshOptions {
        outside_margin: 8.,
        minimum_margin: 3.,
        inside_margin: 4.,
        clip_to_image: false,
        ..options()
    };
    let mask = vec![255; 40 * 60];
    let mesh = generate(40, 60, &mask, &opts);
    coverage(&mesh, 40, 60, &mask, 0, false);
    for predicate in [
        |p: &Vec2| p.x < -3.,
        |p: &Vec2| p.y < -3.,
        |p: &Vec2| p.x > 43.,
        |p: &Vec2| p.y > 63.,
    ] {
        assert!(mesh.positions.iter().any(predicate));
    }
    for (p, uv) in mesh.positions.iter().zip(&mesh.uvs) {
        assert_eq!(uv.x, p.x / 40.);
        assert_eq!(uv.y, p.y / 60.);
    }
    assert!(mesh.triangles.iter().all(|t| {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        cross(a, b, c) > 0.
    }));
}

#[test]
fn faint_island_filter_preserves_small_opaque_and_connected_faint_artwork() {
    let alpha = pixels(96, 64, |x, y| {
        if ((32..70).contains(&x) && (10..40).contains(&y)) || (x == 5 && y == 50) {
            255
        } else if (12..32).contains(&x) && y == 20 {
            1
        } else if (3..6).contains(&x) && (3..6).contains(&y) {
            40
        } else if (72..94).contains(&x) && (42..62).contains(&y) {
            4
        } else {
            0
        }
    });
    let filtered = generate(
        96,
        64,
        &alpha,
        &AlphaMeshOptions {
            remove_faint_islands: true,
            ..options()
        },
    );
    let exact = generate(96, 64, &alpha, &options());
    assert!(covers(&exact, 4.5, 64. - 4.5));
    assert!(!covers(&filtered, 4.5, 64. - 4.5));
    for (x, y) in [(5.5, 50.5), (12.5, 20.5), (72.5, 42.5), (33.5, 11.5)] {
        assert!(covers(&filtered, x, 64. - y));
    }
    let faint = pixels(12, 12, |x, y| if x == y { 1 } else { 0 });
    let all_faint = generate(
        12,
        12,
        &faint,
        &AlphaMeshOptions {
            remove_faint_islands: true,
            ..options()
        },
    );
    coverage(&all_faint, 12, 12, &faint, 0, true);
}
