use zipzi_core::{ModelSetup, Parameter, PreviewValues, Vec2};

use zipzi_sdk::{
    AuthoringSession, MeshBindingAxisSpec, MeshBindingCreateSpec, MeshCreateSpec, MeshGeometry,
    MeshKeyformSpec,
};
fn id(n: u32) -> String {
    format!("79700000-0000-4000-8000-{n:012x}")
}
fn keyform(value: f32, shift: f32) -> MeshKeyformSpec {
    MeshKeyformSpec {
        keys: vec![value],
        positions: vec![
            Vec2::new(shift, 0.),
            Vec2::new(shift + 1., 0.),
            Vec2::new(shift, 1.),
        ],
        opacity: None,
        multiply: None,
        screen: None,
        draw_order: None,
    }
}
fn spec() -> MeshBindingCreateSpec {
    MeshBindingCreateSpec {
        mesh_id: id(2),
        axes: vec![MeshBindingAxisSpec {
            parameter_id: id(3),
            keys: vec![0., 1.],
        }],
        keyforms: vec![keyform(0., 0.), keyform(1., 10.)],
        id: None,
    }
}
fn setup() -> (AuthoringSession, zipzi_sdk::Draft) {
    let sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(id(1), ModelSetup::default()).unwrap(),
            Default::default(),
        )
        .unwrap(),
    );
    let mut draft = sdk.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_parameter(Parameter {
                id: id(3),
                runtime_id: "Param".into(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            c.create_mesh_spec(MeshCreateSpec {
                id: Some(id(2)),
                name: "Face".into(),
                texture_asset_id: String::new(),
                geometry: MeshGeometry {
                    vertex_ids: vec![42, 7, 99],
                    positions: keyform(0., 0.).positions,
                    uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    triangles: vec![[42, 7, 99]],
                    meshing: None,
                },
            })
        })
        .unwrap();
    (sdk, draft)
}
#[test]
fn numeric_specs_allocate_canonical_identities_and_interpolate_without_preview() {
    let (mut sdk, mut e) = setup();
    let input = spec();
    let encoded = serde_json::to_value(&input).unwrap();
    assert_eq!(encoded["axes"][0]["keys"], serde_json::json!([0., 1.]));
    let id = e
        .batch(e.source(), |c| c.create_mesh_binding_spec(input))
        .unwrap();
    uuid::Uuid::parse_str(&id).unwrap();
    let binding = e.document().unwrap().binding(&id).unwrap().clone();
    assert_eq!(binding.keyforms[0].appearance, Default::default());
    assert_eq!(
        binding.keyforms[0].samples,
        [binding.axes[0].samples[0].id.clone()]
    );
    e.batch(e.source(), |c| {
        c.set_mesh_keyform_spec(&id, keyform(1., 20.))
    })
    .unwrap();
    assert_eq!(
        e.document().unwrap().binding(&id).unwrap().axes,
        binding.axes
    );
    sdk.publish(e.prepare("binding").unwrap()).unwrap();
    let before = sdk.version();
    let frame =
        crate::support::frame(sdk.snapshot(), &PreviewValues::from([(self::id(3), 0.5)])).unwrap();
    assert_eq!(frame.drawables[0].positions[0].x, 10.);
    assert_eq!(sdk.version(), before);
    assert_eq!(
        {
            let history = sdk.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (1, 0)
    );
}
#[test]
fn caught_invalid_grid_or_keyform_failure_cannot_publish_partial_creation() {
    for invalid_grid in [false, true] {
        let (sdk, mut e) = setup();
        let mut input = spec();
        if invalid_grid {
            input.axes[0].keys = vec![0., 0.];
        }
        let created = e.batch(e.source(), |c| c.create_mesh_binding_spec(input));
        if invalid_grid {
            assert!(created.is_err());
        } else {
            let mut bad = keyform(1., 10.);
            bad.positions.pop();
            assert!(e
                .batch(e.source(), |c| c
                    .set_mesh_keyform_spec(&created.unwrap(), bad))
                .is_err());
        }
        assert!(e.prepare("failed binding").is_err());
        assert!(sdk.snapshot().document().mesh_order().is_empty());
        assert_eq!(
            {
                let history = sdk.history_state();
                (history.undo_steps, history.redo_steps)
            },
            (0, 0)
        );
    }
}

#[test]
fn frozen_sampling_keeps_numeric_results_and_requested_order_after_publication() {
    use zipzi_sdk::{ObjectTarget, SpatialQuery, SpatialResult};
    let (mut sdk, mut draft) = setup();
    draft
        .batch(draft.source(), |c| c.create_mesh_binding_spec(spec()))
        .unwrap();
    sdk.publish(draft.prepare("binding").unwrap()).unwrap();
    let snapshot = sdk.read_snapshot();
    let (_, p) = crate::support::prepared(&sdk, |c| {
        let mut m = c.read_document()?.mesh(&id(2)).unwrap().clone();
        m.name = "changed".into();
        c.replace_mesh(m)
    })
    .unwrap();
    sdk.publish(p).unwrap();
    assert_ne!(sdk.read_snapshot().source, snapshot.source);
    for x in [5., 0., 5.] {
        let frame =
            crate::support::frame(&snapshot.snapshot, &PreviewValues::from([(id(3), x / 10.)]))
                .unwrap();
        assert_eq!(frame.drawables[0].positions[0].x, x);
        let result = zipzi_sdk::query_spatial_frame(
            snapshot.snapshot.document(),
            &frame,
            &[SpatialQuery::Bounds {
                targets: vec![ObjectTarget::Mesh(id(2))],
                include_hidden: false,
            }],
            &Default::default(),
        )
        .unwrap();
        let SpatialResult::Bounds(bounds) = &result[0] else {
            panic!()
        };
        assert_eq!(bounds.model_bounds, Some([x as f64, 0., x as f64 + 1., 1.]));
    }
}
#[test]
fn sampling_rejects_invalid_queries_and_cancelled_evaluation() {
    let (mut sdk, draft) = setup();
    sdk.publish(draft.prepare("mesh").unwrap()).unwrap();
    assert!(zipzi_sdk::validate_spatial_queries(
        sdk.snapshot().document(),
        &[zipzi_sdk::SpatialQuery::Bounds {
            targets: vec![zipzi_sdk::ObjectTarget::Mesh(id(999))],
            include_hidden: false
        }],
        &Default::default()
    )
    .is_err());
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let control = zipzi_sdk::ConversionControl::from_cancel_flag(cancel);
    let frame = crate::support::frame(sdk.snapshot(), &Default::default()).unwrap();
    assert!(
        zipzi_sdk::query_spatial_frame(sdk.snapshot().document(), &frame, &[], &control).is_err()
    );
}
