use zipzi_contract::{
    api_shape::ApiShape,
    binary_wire::{Reader, WireRead, WireWrite},
    runtime::{ObservationContentIdentity, ObservationTexturePolicy},
};

#[test]
fn policy_requires_tagged_values_and_integer_budgets() {
    assert_eq!(
        ObservationTexturePolicy::default(),
        ObservationTexturePolicy::Original {}
    );
    let policy: ObservationTexturePolicy = serde_json::from_value(serde_json::json!({
        "kind": "fit_budget", "max_rgba_bytes": 134217728
    }))
    .unwrap();
    assert_eq!(
        policy,
        ObservationTexturePolicy::FitBudget {
            max_rgba_bytes: 128 << 20
        }
    );
    for invalid in [
        serde_json::json!("original"),
        serde_json::json!({"kind": "fit_budget", "max_rgba_bytes": true}),
        serde_json::json!({"kind": "fit_budget", "max_rgba_bytes": -1}),
        serde_json::json!({"kind": "fit_budget", "max_rgba_bytes": 1.5}),
        serde_json::json!({"kind": "original", "max_rgba_bytes": 1}),
        serde_json::json!({"kind": "unknown"}),
    ] {
        assert!(serde_json::from_value::<ObservationTexturePolicy>(invalid).is_err());
    }
    let shape = serde_json::to_value(ObservationTexturePolicy::api_shape()).unwrap();
    assert_eq!(shape["tag"], "kind");
}

fn roundtrip<T: WireWrite + WireRead + PartialEq + std::fmt::Debug>(value: T) {
    let mut bytes = Vec::new();
    value.encode(&mut bytes).unwrap();
    let mut admission = Reader::new(&bytes).unwrap();
    T::input_cost(&mut admission).unwrap();
    admission.finish().unwrap();
    let mut reader = Reader::new(&bytes).unwrap();
    assert_eq!(T::decode(&mut reader).unwrap(), value);
    reader.finish().unwrap();
}

#[test]
fn binary_policies_and_typed_content_identity_remain_distinct() {
    roundtrip(ObservationTexturePolicy::Original {});
    roundtrip(ObservationTexturePolicy::FitBudget {
        max_rgba_bytes: 128 << 20,
    });
    let hash = "a".repeat(64);
    let png = ObservationContentIdentity::PngSha256 { hash: hash.clone() };
    let rgba = ObservationContentIdentity::Rgba8Sha256 { hash };
    assert_ne!(png, rgba);
    assert_ne!(
        serde_json::to_value(&png).unwrap(),
        serde_json::to_value(&rgba).unwrap()
    );
    let mut png_bytes = Vec::new();
    let mut rgba_bytes = Vec::new();
    png.encode(&mut png_bytes).unwrap();
    rgba.encode(&mut rgba_bytes).unwrap();
    assert_ne!(png_bytes, rgba_bytes);
    roundtrip(png);
    roundtrip(rgba);
}

#[test]
fn plans_and_budget_diagnostics_roundtrip_through_json_and_binary() {
    use zipzi_contract::runtime::{
        ObservationTexturePlan, ObservationTexturePlanning, ObservationTextureSize,
        ObservationTextureSource, ObservationTextureTarget,
    };
    let source = ObservationTextureSource {
        asset_id: "asset".into(),
        source_size: ObservationTextureSize {
            width: 5,
            height: 3,
        },
        source_content_identity: None,
    };
    let policy = ObservationTexturePolicy::FitBudget { max_rgba_bytes: 8 };
    let ready = ObservationTexturePlanning::Ready {
        plan: ObservationTexturePlan {
            policy: policy.clone(),
            effective_max_rgba_bytes: 8,
            longest_edge_limit: Some(2),
            source_rgba_bytes: 60,
            render_rgba_bytes: 8,
            estimated_mipmap_bytes: 4,
            resized_count: 1,
            planning_version: 1,
            mipmap_version: 2,
            textures: vec![ObservationTextureTarget {
                source: source.clone(),
                render_size: ObservationTextureSize {
                    width: 2,
                    height: 1,
                },
            }],
        },
    };
    let diagnostic = ObservationTexturePlanning::InsufficientBudget {
        policy: ObservationTexturePolicy::FitBudget { max_rgba_bytes: 3 },
        effective_max_rgba_bytes: 3,
        source_rgba_bytes: 60,
        minimum_required_rgba_bytes: 4,
        textures: vec![source],
    };
    for value in [ready, diagnostic] {
        let json = serde_json::to_value(&value).unwrap();
        assert_eq!(
            serde_json::from_value::<ObservationTexturePlanning>(json).unwrap(),
            value
        );
        roundtrip(value);
    }
}
