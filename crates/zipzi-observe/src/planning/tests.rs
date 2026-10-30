use super::*;

#[cfg(feature = "gpu")]
#[test]
fn mipmap_version_matches_the_renderer() {
    assert_eq!(
        MIPMAP_VERSION,
        zipzi_render::texture::STRAIGHT_RGBA_MIPMAP_VERSION
    );
}

fn source(id: &str, width: u32, height: u32) -> ObservationTextureSource {
    ObservationTextureSource {
        asset_id: id.into(),
        source_size: ObservationTextureSize { width, height },
        source_content_identity: None,
    }
}

fn ready(sources: Vec<ObservationTextureSource>, budget: u64) -> ObservationTexturePlan {
    match plan_textures(
        sources,
        ObservationTexturePolicy::FitBudget {
            max_rgba_bytes: budget,
        },
        u64::MAX,
    )
    .unwrap()
    {
        ObservationTexturePlanning::Ready { plan } => plan,
        other => panic!("Expected feasible plan: {other:?}"),
    }
}

#[test]
fn empty_set_validates_policy_before_returning_zero_bytes() {
    let plan = ready(vec![], 1);
    assert!(plan.textures.is_empty());
    assert_eq!(plan.longest_edge_limit, None);
    assert_eq!(
        (
            plan.source_rgba_bytes,
            plan.render_rgba_bytes,
            plan.estimated_mipmap_bytes
        ),
        (0, 0, 0)
    );
    for budget in [0, 101] {
        assert_eq!(
            plan_textures(
                [],
                ObservationTexturePolicy::FitBudget {
                    max_rgba_bytes: budget
                },
                100
            )
            .unwrap_err()
            .code,
            "INVALID_TEXTURE_POLICY"
        );
    }
}

#[test]
fn original_over_limit_is_a_diagnostic_with_source_table() {
    let result = plan_textures(
        [source("a", 3, 5)],
        ObservationTexturePolicy::Original {},
        59,
    )
    .unwrap();
    match result {
        ObservationTexturePlanning::InsufficientBudget {
            source_rgba_bytes,
            minimum_required_rgba_bytes,
            textures,
            ..
        } => {
            assert_eq!((source_rgba_bytes, minimum_required_rgba_bytes), (60, 60));
            assert_eq!(textures, [source("a", 3, 5)]);
        }
        _ => panic!("Original must not resize"),
    }
}

#[test]
fn exact_budget_preserves_dimensions_and_mips_use_each_actual_level() {
    let plan = ready(vec![source("a", 3, 5), source("b", 1, 9)], 96);
    assert_eq!(plan.source_rgba_bytes, 96);
    assert_eq!(plan.render_rgba_bytes, 96);
    assert_eq!(plan.resized_count, 0);
    assert_eq!(plan.longest_edge_limit, Some(9));
    // 3x5 -> 1x2 -> 1x1; 1x9 -> 1x4 -> 1x2 -> 1x1.
    assert_eq!(plan.estimated_mipmap_bytes, 12 + 28);
    assert_eq!(
        estimated_mipmap_bytes(ObservationTextureSize {
            width: 1,
            height: 1
        })
        .unwrap(),
        0
    );
    assert_eq!(
        estimated_mipmap_bytes(ObservationTextureSize {
            width: 8,
            height: 1
        })
        .unwrap(),
        28
    );
}

#[test]
fn minimum_budget_and_shortfall_do_not_invent_target_dimensions() {
    let sources = vec![source("a", 11, 7), source("b", 1, u32::MAX)];
    let plan = ready(sources.clone(), 8);
    assert_eq!(plan.render_rgba_bytes, 8);
    assert!(plan.textures.iter().all(|t| t.render_size
        == ObservationTextureSize {
            width: 1,
            height: 1
        }));
    match plan_textures(
        sources.clone(),
        ObservationTexturePolicy::FitBudget { max_rgba_bytes: 7 },
        100,
    )
    .unwrap()
    {
        ObservationTexturePlanning::InsufficientBudget {
            minimum_required_rgba_bytes,
            textures,
            ..
        } => {
            assert_eq!(minimum_required_rgba_bytes, 8);
            assert_eq!(textures, sources);
        }
        _ => panic!("Four bytes per distinct asset are required"),
    }
}

#[test]
fn integer_search_matches_exhaustive_largest_feasible_edge() {
    for width in 1..=13 {
        for height in 1..=13 {
            let sources = vec![source("odd", width, height), source("thin", 1, 17)];
            for budget in (8..=width as u64 * height as u64 * 4 + 68).step_by(7) {
                let plan = ready(sources.clone(), budget);
                let maximum = width.max(height).max(17);
                let expected_limit = (1..=maximum)
                    .filter(|&limit| {
                        // Independent integer definition, not the planner's scaling helper.
                        sources
                            .iter()
                            .map(|s| {
                                let w = s.source_size.width as u64;
                                let h = s.source_size.height as u64;
                                let m = w.max(h);
                                let l = u64::from(limit).min(m);
                                (w * l / m).max(1) * (h * l / m).max(1) * 4
                            })
                            .sum::<u64>()
                            <= budget
                    })
                    .max()
                    .unwrap();
                assert_eq!(plan.longest_edge_limit, Some(expected_limit));
                assert!(plan.render_rgba_bytes <= budget);
            }
        }
    }
}

#[test]
fn sources_are_sorted_deduplicated_and_conflicts_rejected() {
    let mut a = source("a", 3, 5);
    a.source_content_identity = Some(ObservationContentIdentity::PngSha256 {
        hash: "a".repeat(64),
    });
    let b = source("b", 9, 1);
    let first = ready(vec![b.clone(), a.clone(), a.clone()], 80);
    let second = ready(vec![a.clone(), b.clone()], 80);
    assert_eq!(first, second);
    assert_eq!(first.source_rgba_bytes, 96);
    assert_eq!(first.textures[0].source, a);
    assert_eq!(first.textures[1].source, b);
    let mut conflicting = a.clone();
    conflicting.source_content_identity = Some(ObservationContentIdentity::Rgba8Sha256 {
        hash: "a".repeat(64),
    });
    assert_eq!(
        plan_textures([a, conflicting], ObservationTexturePolicy::Original {}, 100)
            .unwrap_err()
            .code,
        "MIXED_DOCUMENT_SNAPSHOTS"
    );
}

#[test]
fn invalid_dimensions_and_byte_overflow_fail_before_search() {
    for (w, h) in [(0, 1), (1, 0), (u32::MAX, u32::MAX)] {
        let error = plan_textures(
            [source("bad", w, h)],
            ObservationTexturePolicy::Original {},
            u64::MAX,
        )
        .unwrap_err();
        assert_eq!(error.code, "RESOURCE_DIMENSIONS");
        assert_eq!(error.asset_id.as_deref(), Some("bad"));
    }
    // Each texture fits u64, their sum does not.
    let error = plan_textures(
        [
            source("a", u32::MAX, 1 << 30),
            source("b", u32::MAX, 1 << 30),
        ],
        ObservationTexturePolicy::Original {},
        u64::MAX,
    )
    .unwrap_err();
    assert_eq!(error.code, "RESOURCE_DIMENSIONS");
}

#[test]
fn large_thin_texture_uses_integer_dimensions_without_device_clamping() {
    let plan = ready(vec![source("thin", u32::MAX, 1)], 400);
    assert_eq!(plan.longest_edge_limit, Some(100));
    assert_eq!(
        plan.textures[0].render_size,
        ObservationTextureSize {
            width: 100,
            height: 1
        }
    );
    let original = ready(vec![source("thin", 16385, 1)], 16385 * 4);
    assert_eq!(original.textures[0].render_size.width, 16385);
}

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

#[test]
fn reference_collection_keeps_hidden_and_mask_meshes_and_ignores_unused_images() {
    use zipzi_core::{Canvas, Document, ImageContent, Mesh, Vec2};
    let mut document = Document::new();
    assert!(document
        .initialize(&id(1), Canvas::new(100., 100., Vec2::new(50., 50.), 10.))
        .is_ok());
    // These files deliberately do not exist: planning must never read/decode them.
    for (n, width) in [(2, 3), (3, 5), (4, u32::MAX)] {
        assert!(document
            .add_asset(ImageAsset {
                id: id(n),
                name: format!("asset {n}"),
                unresolved_source: None,
                content: Some(ImageContent {
                    source: format!("/missing/texture-{n}.png"),
                    width,
                    height: width,
                    sha256: String::new()
                }),
            })
            .status
            .is_ok());
    }
    for (n, asset, enabled, masks) in [
        (5, 3, false, vec![]),
        (6, 2, true, vec![id(5)]),
        (7, 2, true, vec![]),
    ] {
        assert!(document
            .create_mesh(Mesh {
                id: id(n),
                texture_asset_id: id(asset).into(),
                enabled,
                masks,
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let sources = texture_sources(&document).unwrap();
    assert_eq!(sources, [source(&id(2), 3, 3), source(&id(3), 5, 5)]);
    let plan = ready(sources, 136);
    assert_eq!(plan.source_rgba_bytes, 136);
}

#[test]
fn execution_reference_check_rejects_unplanned_mask_sources() {
    let frame = DrawableFrame {
        drawables: vec![zipzi_core::Drawable {
            texture_asset_id: "mask".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        validate_frame_references(&frame, &[]).unwrap_err().code,
        "UNPLANNED_TEXTURE"
    );
}
