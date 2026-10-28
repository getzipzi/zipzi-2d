use super::*;
fn mask(w: usize, h: usize, p: impl Fn(f64, f64) -> bool) -> Vec<u8> {
    (0..w * h)
        .map(|i| {
            if p((i % w) as f64 + 0.5, (i / w) as f64 + 0.5) {
                255
            } else {
                0
            }
        })
        .collect()
}
fn settings() -> MeshSettings {
    MeshSettings {
        edge_spacing: 12.,
        interior_spacing: 28.,
        edge_width: 8.,
        ..MeshSettings::default()
    }
}
fn validate(out: &GeneratedMesh) {
    let g = &out.geometry;
    assert!(!g.triangles.is_empty());
    for t in &g.triangles {
        let [a, b, c] = t.map(|i| {
            let p = g.positions[i as usize];
            [p.x as f64, p.y as f64]
        });
        assert!(cross(a, b, c) > 0.);
    }
    let mut uses = BTreeMap::new();
    for t in &g.triangles {
        for e in edges(t.map(|i| i as usize)) {
            *uses.entry(e).or_insert(0) += 1;
        }
    }
    assert!(uses.values().all(|n| *n == 1 || *n == 2));
    if g.meshing.is_some() {
        extract_manual_meshing(g).unwrap();
    }
}
#[test]
fn every_edge_and_fill_combination_is_deterministic_and_manifold() {
    let alpha = mask(192, 160, |x, y| {
        (x - 96.).powi(2) / 78_f64.powi(2) + (y - 80.).powi(2) / 62_f64.powi(2) < 1.
    });
    for edge_mode in [
        MeshEdgeMode::Single,
        MeshEdgeMode::Double,
        MeshEdgeMode::Triple,
    ] {
        for fill_algorithm in [
            MeshFillAlgorithm::GradedPoisson,
            MeshFillAlgorithm::SimpleTriangles,
            MeshFillAlgorithm::AdaptiveQuadtree,
            MeshFillAlgorithm::TriangleFractal,
            MeshFillAlgorithm::ContourPaving,
        ] {
            let s = MeshSettings {
                edge_mode,
                fill_algorithm,
                ..settings()
            };
            let a = generate_mesh(192, 160, &alpha, &s, MeshGenerationStage::Full)
                .unwrap_or_else(|e| panic!("{edge_mode:?} {fill_algorithm:?}: {e}"));
            validate(&a);
            assert!(a.geometry.meshing.as_ref().unwrap().constraints.is_empty());
            let b = generate_mesh(192, 160, &alpha, &s, MeshGenerationStage::Full).unwrap();
            assert_eq!(a.geometry, b.geometry);
            if edge_mode != MeshEdgeMode::Single {
                assert!(
                    !a.inner_loops.is_empty(),
                    "{edge_mode:?} {fill_algorithm:?} fell back"
                );
            }
            if edge_mode == MeshEdgeMode::Triple {
                assert!(!a.middle_loops.is_empty());
            }
        }
    }
}
#[test]
fn holes_and_disconnected_islands_keep_their_domains() {
    let alpha = mask(192, 128, |x, y| {
        let r = (x - 54.).hypot(y - 64.);
        (r < 42. && r > 18.) || (x > 126. && x < 175. && y > 20. && y < 105.)
    });
    let out = generate_mesh(192, 128, &alpha, &settings(), MeshGenerationStage::Full).unwrap();
    validate(&out);
    assert_eq!(out.boundary_loops.len(), 3);
    for t in &out.geometry.triangles {
        let p = t.map(|i| out.geometry.positions[i as usize]);
        let x = p.iter().map(|p| p.x).sum::<f32>() / 3.;
        let y = p.iter().map(|p| p.y).sum::<f32>() / 3.;
        assert!((x - 54.).hypot(y - 64.) > 15.);
        assert!(!(x > 102. && x < 122.));
    }
}
#[test]
fn narrow_ribbon_covers_every_source_pixel_center() {
    let alpha = mask(180, 40, |x, y| x > 10. && x < 170. && y > 18. && y < 21.);
    let out = generate_mesh(180, 40, &alpha, &settings(), MeshGenerationStage::Full).unwrap();
    validate(&out);
    assert_eq!(out.boundary_loops[0].len(), out.geometry.positions.len());
    for (i, &a) in alpha.iter().enumerate() {
        if a == 0 {
            continue;
        }
        let p = [(i % 180) as f64 + 0.5, 40. - ((i / 180) as f64 + 0.5)];
        assert!(out.geometry.triangles.iter().any(|t| {
            let [a, b, c] = t.map(|i| {
                let p = out.geometry.positions[i as usize];
                [p.x as f64, p.y as f64]
            });
            cross(a, b, p) >= -EPS && cross(b, c, p) >= -EPS && cross(c, a, p) >= -EPS
        }));
    }
}
#[test]
fn edges_are_unchanged_by_the_fill_strategy() {
    let alpha = mask(160, 160, |x, y| (x - 80.).hypot(y - 80.) < 60.);
    let s = MeshSettings {
        edge_mode: MeshEdgeMode::Triple,
        ..settings()
    };
    let a = generate_mesh(160, 160, &alpha, &s, MeshGenerationStage::Edges).unwrap();
    assert!(a.interior_uvs.is_empty());
    assert!(a.geometry.meshing.as_ref().unwrap().constraints.is_empty());
    for fill_algorithm in [
        MeshFillAlgorithm::GradedPoisson,
        MeshFillAlgorithm::AdaptiveQuadtree,
        MeshFillAlgorithm::ContourPaving,
    ] {
        let b = generate_mesh(
            160,
            160,
            &alpha,
            &MeshSettings {
                fill_algorithm,
                ..s
            },
            MeshGenerationStage::Full,
        )
        .unwrap();
        for (ra, rb) in a.boundary_loops.iter().zip(&b.boundary_loops) {
            assert_eq!(
                ra.iter()
                    .map(|i| a.geometry.positions[*i as usize])
                    .collect::<Vec<_>>(),
                rb.iter()
                    .map(|i| b.geometry.positions[*i as usize])
                    .collect::<Vec<_>>()
            );
        }
    }
}
#[test]
fn cropped_mesh_has_valid_editable_topology() {
    let alpha = vec![255; 64 * 64];
    let out = generate_mesh(
        64,
        64,
        &alpha,
        &MeshSettings {
            clip_to_image: true,
            edge_mode: MeshEdgeMode::Double,
            ..settings()
        },
        MeshGenerationStage::Full,
    )
    .unwrap();
    validate(&out);
    assert!(
        out.geometry
            .uvs
            .iter()
            .all(|p| (0. ..=1.).contains(&p.x) && (0. ..=1.).contains(&p.y))
    );
    assert!(out.geometry.meshing.is_some());
}
#[test]
fn invalid_inputs_and_limits_are_explicit() {
    assert!(matches!(
        generate_mesh(0, 1, &[], &settings(), MeshGenerationStage::Full),
        Err(MeshError::Input)
    ));
    assert!(matches!(
        generate_mesh(2, 2, &[0; 4], &settings(), MeshGenerationStage::Full),
        Err(MeshError::Empty)
    ));
    assert!(
        MeshSettings {
            edge_spacing: f64::NAN,
            ..settings()
        }
        .validate()
        .is_err()
    );
    assert!(matches!(
        generate_mesh(
            64,
            64,
            &[255; 4096],
            &MeshSettings {
                max_vertices: 3,
                ..settings()
            },
            MeshGenerationStage::Full
        ),
        Err(MeshError::Limit)
    ));
}
#[test]
fn wrap_adds_coverage_and_preserves_pixels() {
    let alpha = mask(96, 96, |x, y| {
        y > 20. && y < 80. && ((x > 20. && x < 35.) || (x > 41. && x < 56.))
    });
    let s = MeshSettings {
        wrap: 12.,
        ..settings()
    };
    let out = generate_mesh(96, 96, &alpha, &s, MeshGenerationStage::Full).unwrap();
    assert_eq!(out.boundary_loops.len(), 1);
    validate(&out);
}
#[test]
fn spatial_queries_match_all_segments() {
    let loops = vec![vec![[0., 0.], [100., 0.], [80., 50.], [20., 80.]]];
    let idx = SegmentIndex::new(&loops, 5.);
    for x in -20..120 {
        for y in (-20..100).step_by(7) {
            let p = [x as f64, y as f64];
            let expected = (0..4)
                .map(|i| segment_distance(p, loops[0][i], loops[0][(i + 1) % 4]))
                .fold(f64::INFINITY, f64::min);
            assert!((idx.distance(p) - expected).abs() < 1e-8);
        }
    }
}

fn reference_mask(name: &str, w: usize, h: usize) -> Vec<u8> {
    mask(w, h, |x, y| match name {
        "ellipse" => (x - 96.).powi(2) / 78_f64.powi(2) + (y - 80.).powi(2) / 62_f64.powi(2) < 1.,
        "hole" => (x - 96.).hypot(y - 80.) < 62. && (x - 96.).hypot(y - 80.) > 22.,
        "ribbon" => x > 10. && x < 180. && y > 78. && y < 81.,
        "concave" => {
            x > 20. && x < 170. && y > 20. && y < 140. && !(x > 65. && x < 125. && y < 95.)
        }
        _ => panic!("unknown fixture"),
    })
}
fn coverage(w: usize, h: usize, alpha: &[u8], points: &[Point], triangles: &[Triangle]) -> usize {
    let mut covered = vec![false; w * h];
    for &t in triangles {
        let [a, b, c] = t.map(|i| points[i]);
        let minx = a[0].min(b[0]).min(c[0]).floor().max(0.) as usize;
        let maxx = a[0].max(b[0]).max(c[0]).ceil().min(w as f64) as usize;
        let miny = a[1].min(b[1]).min(c[1]).floor().max(0.) as usize;
        let maxy = a[1].max(b[1]).max(c[1]).ceil().min(h as f64) as usize;
        for y in miny..maxy {
            for x in minx..maxx {
                let p = [x as f64 + 0.5, y as f64 + 0.5];
                let v = [cross(a, b, p), cross(b, c, p), cross(c, a, p)];
                if v.iter().all(|v| *v >= -EPS) || v.iter().all(|v| *v <= EPS) {
                    covered[y * w + x] = true
                }
            }
        }
    }
    covered
        .iter()
        .zip(alpha)
        .filter(|(c, a)| **c && **a != 0)
        .count()
}
#[test]
fn psd2live_reference_coverage_and_topology() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../fixtures/mesh-generation/psd2live.json"
    ))
    .unwrap();
    let mut reports = vec![];
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let edge = case["edge"].as_str().unwrap();
        let fill = case["fill"].as_str().unwrap();
        let edge_mode = match edge {
            "SINGLE" => MeshEdgeMode::Single,
            "DOUBLE" => MeshEdgeMode::Double,
            "TRIPLE" => MeshEdgeMode::Triple,
            _ => panic!(),
        };
        let fill_algorithm = match fill {
            "GRADED_POISSON" => MeshFillAlgorithm::GradedPoisson,
            "SIMPLE_TRIANGLES" => MeshFillAlgorithm::SimpleTriangles,
            "ADAPTIVE_QUADTREE" => MeshFillAlgorithm::AdaptiveQuadtree,
            "TRIANGLE_FRACTAL" => MeshFillAlgorithm::TriangleFractal,
            "CONTOUR_PAVING" => MeshFillAlgorithm::ContourPaving,
            _ => panic!(),
        };
        let alpha = reference_mask(name, 192, 160);
        let start = std::time::Instant::now();
        let out = generate_mesh(
            192,
            160,
            &alpha,
            &MeshSettings {
                edge_mode,
                fill_algorithm,
                ..settings()
            },
            MeshGenerationStage::Full,
        )
        .unwrap_or_else(|e| panic!("{name}/{edge}/{fill}: {e}"));
        let elapsed = start.elapsed().as_nanos();
        validate(&out);
        assert_ne!(case["failed"], true, "unexpected reference failure");
        let raw: Vec<f64> = case["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let reference: Vec<Point> = raw.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
        let raw: Vec<usize> = case["indices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as usize)
            .collect();
        let reference_triangles: Vec<Triangle> =
            raw.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();
        let points: Vec<Point> = out
            .geometry
            .positions
            .iter()
            .map(|p| [p.x as f64, 160. - p.y as f64])
            .collect();
        let triangles: Vec<Triangle> = out
            .geometry
            .triangles
            .iter()
            .map(|t| t.map(|i| i as usize))
            .collect();
        let expected = coverage(192, 160, &alpha, &reference, &reference_triangles);
        let actual = coverage(192, 160, &alpha, &points, &triangles);
        let painted = alpha.iter().filter(|a| **a > 0).count();
        println!(
            "{name}/{edge}/{fill}: vertices {} -> {}, covered {expected} -> {actual} / {painted}",
            reference.len(),
            points.len()
        );
        assert_eq!(
            out.boundary_loops.len(),
            case["boundary_loops"].as_array().unwrap().len(),
            "{name}/{edge}/{fill} topology"
        );
        assert_eq!(
            actual, painted,
            "{name}/{edge}/{fill} must cover every painted pixel center"
        );
        assert!(
            points.len() <= reference.len() * 2 + 10,
            "{name}/{edge}/{fill} unexpected density explosion"
        );
        reports.push(serde_json::json!({"name":name,"edge":edge,"fill":fill,"reference_vertices":reference.len(),"vertices":points.len(),"reference_covered":expected,"covered":actual,"painted":painted,"elapsed_ns":elapsed,"recovered_components":out.recovered_components,"points":points,"triangles":triangles}));
    }
    if let Ok(path) = std::env::var("ZIPZI_MESH_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).unwrap()).unwrap();
    }
}

#[test]
fn graded_structured_fills_keep_pattern_edges_and_handle_rotation() {
    let loops = vec![
        (0..32)
            .map(|i| {
                let side = i / 8;
                let k = (i % 8) as f64 * 32.;
                match side {
                    0 => [32. + k, 32.],
                    1 => [288., 32. + k],
                    2 => [288. - k, 288.],
                    _ => [32., 288. - k],
                }
            })
            .collect::<Vec<_>>(),
    ];
    for kind in [
        MeshFillAlgorithm::AdaptiveQuadtree,
        MeshFillAlgorithm::TriangleFractal,
        MeshFillAlgorithm::ContourPaving,
    ] {
        for angle in [0., 37.] {
            let p = FillParameters {
                edge_ratio: 1.,
                gradation: 1.,
                angle,
                ..FillParameters::default()
            };
            let a = structured::generate(&loops, kind, p, 96., false)
                .unwrap_or_else(|| panic!("{kind:?} angle {angle}"));
            assert!(valid_mesh(&a, &loop_ids(&loops)), "{kind:?} {angle}");
            assert!(a.points.len() > loops[0].len());
            let b = structured::generate(&loops, kind, p, 96., false).unwrap();
            assert_eq!(a.points, b.points);
            assert_eq!(a.triangles, b.triangles);
        }
    }
}
#[test]
fn patterned_layout_triangles_are_not_retriangulated() {
    let loops = vec![vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]]];
    let layout = Mesh {
        points: vec![
            [0., 0.],
            [100., 0.],
            [100., 100.],
            [0., 100.],
            [30., 30.],
            [70., 30.],
            [70., 70.],
            [30., 70.],
        ],
        triangles: vec![[4, 5, 6], [4, 6, 7]],
    };
    let out =
        triangulate::with_layout(&loops, &layout, &[[15., 50.], [85., 50.]], 30., false).unwrap();
    let mut faces: BTreeSet<_> = out
        .triangles
        .iter()
        .map(|t| {
            let mut t = *t;
            t.sort();
            t
        })
        .collect();
    for t in layout.triangles {
        assert!(faces.remove(&t));
    }
}
#[test]
fn scaling_preserves_ribbons_and_source_coordinate_extent() {
    let alpha = mask(768, 640, |x, y| x > 40. && x < 720. && y > 312. && y < 324.);
    let s = MeshSettings {
        unit_scale: 4.,
        detail: 1.,
        ..settings()
    };
    let out = generate_mesh(768, 640, &alpha, &s, MeshGenerationStage::Full).unwrap();
    assert_eq!(out.geometry.positions.len(), 60);
    assert!(
        out.geometry
            .positions
            .iter()
            .all(|p| p.x >= 40. && p.x <= 720.)
    );
    validate(&out);
}
#[test]
fn boundary_diagonal_suppression_adds_support_without_changing_outline() {
    let alpha = mask(192, 160, |x, y| (x - 96.).hypot(y - 80.) < 60.);
    let out = generate_mesh(
        192,
        160,
        &alpha,
        &MeshSettings {
            suppress_boundary_diagonals: true,
            interior_spacing: 10000.,
            ..settings()
        },
        MeshGenerationStage::Full,
    )
    .unwrap();
    validate(&out);
    let boundary = &out.boundary_loops[0];
    let ids: BTreeSet<_> = boundary.iter().copied().collect();
    let rim: BTreeSet<_> = (0..boundary.len())
        .map(|i| {
            edge(
                boundary[i] as usize,
                boundary[(i + 1) % boundary.len()] as usize,
            )
        })
        .collect();
    for t in &out.geometry.triangles {
        for e in edges(t.map(|i| i as usize)) {
            if ids.contains(&(e[0] as u32)) && ids.contains(&(e[1] as u32)) {
                assert!(rim.contains(&e));
            }
        }
    }
}

#[test]
fn coarse_poisson_spacing_uses_occupied_samples_instead_of_empty_buckets() {
    let loops = vec![vec![[0., 0.], [100., 0.], [100., 100.], [0., 100.]]];
    let sites = fill::poisson(&loops, FillParameters::default(), 1e6);
    assert!(sites.len() <= 1);
    assert!(
        fill::generate(
            &vec![vec![[0., 0.], [1e6, 0.], [1e6, 1e6], [0., 1e6]]],
            MeshFillAlgorithm::GradedPoisson,
            FillParameters::default(),
            40.,
            false,
            false
        )
        .is_none()
    );
}

#[test]
fn generated_band_edges_are_free_until_the_user_constrains_them() {
    use zipzi_sdk::{
        ManualMeshOperation, ManualVertexIdAllocator, add_manual_interior_sites, edit_manual_mesh,
    };
    let alpha = mask(192, 160, |x, y| {
        (x - 96.).powi(2) / 78_f64.powi(2) + (y - 80.).powi(2) / 62_f64.powi(2) < 1.
    });
    let generated = generate_mesh(
        192,
        160,
        &alpha,
        &MeshSettings {
            edge_mode: MeshEdgeMode::Triple,
            ..settings()
        },
        MeshGenerationStage::Full,
    )
    .unwrap()
    .geometry;
    assert!(generated.meshing.as_ref().unwrap().constraints.is_empty());
    let edges_of = |g: &zipzi_sdk::MeshGeometry| -> BTreeSet<[usize; 2]> {
        g.triangles
            .iter()
            .flat_map(|t| edges(t.map(|i| i as usize)))
            .collect()
    };
    let reconnected = edit_manual_mesh(&generated, ManualMeshOperation::Reconnect)
        .unwrap()
        .geometry;
    assert!(reconnected.meshing.as_ref().unwrap().constraints.is_empty());
    let original_edges = edges_of(&generated);
    let reconnected_edges = edges_of(&reconnected);
    // Choose an actual generated edge which unconstrained reconnection replaces.
    let [a, b] = *original_edges
        .difference(&reconnected_edges)
        .next()
        .expect("generated edges should be free to flip");
    let fixed = edit_manual_mesh(
        &generated,
        ManualMeshOperation::ConstrainEdge {
            a: a as u32,
            b: b as u32,
        },
    )
    .unwrap()
    .geometry;
    let constraints = &fixed.meshing.as_ref().unwrap().constraints;
    assert_eq!(constraints.len(), 1);
    assert_eq!(constraints[0].kind, zipzi_core::MeshConstraintKind::User);
    let reconnected = edit_manual_mesh(&fixed, ManualMeshOperation::Reconnect)
        .unwrap()
        .geometry;
    assert!(edges_of(&reconnected).contains(&[a, b]));
    assert_eq!(
        &reconnected.meshing.as_ref().unwrap().constraints,
        constraints
    );
    let face = fixed.triangles[0].map(|i| fixed.uvs[i as usize]);
    let site = Vec2::new(
        face.iter().map(|p| p.x).sum::<f32>() / 3.,
        face.iter().map(|p| p.y).sum::<f32>() / 3.,
    );
    let filled =
        add_manual_interior_sites(&fixed, &[site], &mut ManualVertexIdAllocator::new(&fixed))
            .unwrap();
    assert_eq!(filled.added.len(), 1);
    assert_eq!(
        &filled.geometry.meshing.as_ref().unwrap().constraints,
        constraints
    );
    assert!(edges_of(&filled.geometry).contains(&[a, b]));
}

#[test]
fn interior_generation_uses_authored_contours_and_keeps_vertex_identities() {
    use zipzi_sdk::{ManualMeshOperation, edit_manual_mesh};
    let alpha = mask(192, 160, |x, y| {
        (x - 96.).powi(2) / 78_f64.powi(2) + (y - 80.).powi(2) / 62_f64.powi(2) < 1.
    });
    let source = generate_mesh(
        192,
        160,
        &alpha,
        &MeshSettings {
            edge_mode: MeshEdgeMode::Double,
            ..settings()
        },
        MeshGenerationStage::Edges,
    )
    .unwrap()
    .geometry;
    let boundary = &source.meshing.as_ref().unwrap().boundary;
    let rim: BTreeSet<_> = (0..boundary.len())
        .map(|i| {
            edge(
                boundary[i] as usize,
                boundary[(i + 1) % boundary.len()] as usize,
            )
        })
        .collect();
    let e = source
        .triangles
        .iter()
        .flat_map(|t| edges(t.map(|i| i as usize)))
        .find(|e| !rim.contains(e))
        .unwrap();
    let source = edit_manual_mesh(
        &source,
        ManualMeshOperation::ConstrainEdge {
            a: e[0] as u32,
            b: e[1] as u32,
        },
    )
    .unwrap()
    .geometry;
    for fill_algorithm in [
        MeshFillAlgorithm::GradedPoisson,
        MeshFillAlgorithm::SimpleTriangles,
        MeshFillAlgorithm::AdaptiveQuadtree,
        MeshFillAlgorithm::TriangleFractal,
        MeshFillAlgorithm::ContourPaving,
    ] {
        let s = MeshSettings {
            fill_algorithm,
            interior_spacing: 24.,
            ..settings()
        };
        let filled = fill_existing_geometry(&source, 192, 160, &s)
            .unwrap_or_else(|e| panic!("{fill_algorithm:?}: {e}"));
        assert!(filled.vertex_ids.len() > source.vertex_ids.len());
        assert_eq!(
            &filled.vertex_ids[..source.vertex_ids.len()],
            &source.vertex_ids
        );
        assert_eq!(
            &filled.positions[..source.positions.len()],
            &source.positions
        );
        assert_eq!(&filled.uvs[..source.uvs.len()], &source.uvs);
        assert_eq!(filled.meshing, source.meshing);
        let other = fill_existing_geometry(
            &source,
            192,
            160,
            &MeshSettings {
                edge_spacing: 70.,
                alpha_threshold: 255,
                wrap: 64.,
                outer_margin: 40.,
                ..s
            },
        )
        .unwrap();
        assert_eq!(filled, other, "interior changes must never retrace alpha");
    }
}

#[test]
fn filling_a_coarse_authored_rectangle_does_not_crowd_its_fixed_sides() {
    let source = MeshGeometry {
        vertex_ids: vec![10, 20, 30, 40],
        positions: vec![
            Vec2::new(0., 0.),
            Vec2::new(96., 0.),
            Vec2::new(96., 64.),
            Vec2::new(0., 64.),
        ],
        uvs: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        triangles: vec![[10, 20, 30], [10, 30, 40]],
        meshing: None,
    };
    for fill_algorithm in [
        MeshFillAlgorithm::GradedPoisson,
        MeshFillAlgorithm::SimpleTriangles,
        MeshFillAlgorithm::AdaptiveQuadtree,
        MeshFillAlgorithm::TriangleFractal,
        MeshFillAlgorithm::ContourPaving,
    ] {
        let filled = fill_existing_geometry(
            &source,
            96,
            64,
            &MeshSettings {
                fill_algorithm,
                interior_spacing: 12.,
                ..settings()
            },
        )
        .unwrap_or_else(|e| panic!("{fill_algorithm:?}: {e}"));
        assert_eq!(&filled.vertex_ids[..4], &source.vertex_ids);
        assert_eq!(&filled.positions[..4], &source.positions);
        assert_eq!(&filled.uvs[..4], &source.uvs);
        assert!(zipzi_sdk::extract_manual_meshing(&filled).is_ok());
        // Sites may fill the interior, but must not form collapsing layers
        // along the four original sides, which stay fixed during generation.
        assert!(filled.positions.len() < 100);
        assert!(
            filled
                .positions
                .iter()
                .skip(4)
                .all(|p| { p.x.min(96. - p.x).min(p.y).min(64. - p.y) > 0.1 })
        );
    }
}

#[test]
fn filling_authored_islands_preserves_holes_and_winding() {
    let alpha = mask(192, 128, |x, y| {
        let r = (x - 54.).hypot(y - 64.);
        (r < 42. && r > 18.) || (x > 126. && x < 175. && y > 20. && y < 105.)
    });
    let source = generate_mesh(192, 128, &alpha, &settings(), MeshGenerationStage::Edges)
        .unwrap()
        .geometry;
    let filled = fill_existing_geometry(&source, 192, 128, &settings()).unwrap();
    let slots: BTreeMap<_, _> = filled
        .vertex_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    for t in &filled.triangles {
        let p = t.map(|id| {
            let p = filled.positions[slots[&id]];
            [p.x as f64, p.y as f64]
        });
        assert!(cross(p[0], p[1], p[2]) > 0.);
        let center = mul(add(add(p[0], p[1]), p[2]), 1. / 3.);
        assert!(distance(center, [54., 64.]) > 15.);
        assert!(!(center[0] > 102. && center[0] < 122.));
    }
    assert_eq!(
        &filled.positions[..source.positions.len()],
        &source.positions
    );
    assert!(filled.meshing.is_none());
}
