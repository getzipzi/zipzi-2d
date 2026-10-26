use zipzi_core::{ModelSetup, Parameter, PreviewValues, Vec2};
use zipzi_sdk::{
    AuthoringSession, MeshBindingAxisSpec, MeshBindingCreateSpec, MeshBindingOperation,
    MeshCreateSpec, MeshGeometry, MeshKeyformSpec,
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
fn setup() -> (AuthoringSession, zipzi_sdk::EditSession<'static>) {
    let sdk = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
    let mut e = sdk.begin_owned_edit("bound mesh", None).unwrap();
    e.create_parameter(Parameter {
        id: id(3),
        minimum: 0.,
        maximum: 1.,
        default_value: 0.,
        ..Default::default()
    })
    .unwrap();
    e.create_mesh_spec(MeshCreateSpec {
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
    .unwrap();
    (sdk, e)
}
#[test]
fn numeric_specs_allocate_canonical_identities_and_interpolate_without_preview() {
    let (mut sdk, mut e) = setup();
    let input = spec();
    let encoded = serde_json::to_value(&input).unwrap();
    assert_eq!(encoded["axes"][0]["keys"], serde_json::json!([0., 1.]));
    let id = e.create_mesh_binding_spec(input).unwrap();
    uuid::Uuid::parse_str(&id).unwrap();
    let binding = e.candidate_document().get_binding(&id).unwrap().clone();
    assert_eq!(binding.keyforms[0].appearance, Default::default());
    assert_eq!(
        binding.keyforms[0].samples,
        [binding.axes[0].samples[0].id.clone()]
    );
    MeshBindingOperation::SetKeyform {
        id: id.clone(),
        keyform: keyform(1., 20.),
    }
    .apply(&mut e)
    .unwrap();
    assert_eq!(
        e.candidate_document().get_binding(&id).unwrap().axes,
        binding.axes
    );
    e.commit_to(&mut sdk).unwrap();
    let before = sdk.version();
    let frame = sdk
        .read_snapshot()
        .sample_modeling(
            vec![PreviewValues::from([(self::id(3), 0.5)])],
            Default::default(),
        )
        .unwrap();
    assert_eq!(frame.frames[0].drawables[0].positions[0].x, 10.);
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.history_lengths(), (1, 0));
}
#[test]
fn caught_invalid_grid_or_keyform_failure_cannot_publish_partial_creation() {
    for invalid_grid in [false, true] {
        let (mut sdk, mut e) = setup();
        let mut input = spec();
        if invalid_grid {
            input.axes[0].keys = vec![0., 0.];
        }
        let created = e.create_mesh_binding_spec(input);
        if invalid_grid {
            assert!(created.is_err());
        } else {
            let mut bad = keyform(1., 10.);
            bad.positions.pop();
            assert!(MeshBindingOperation::SetKeyform {
                id: created.unwrap(),
                keyform: bad
            }
            .apply(&mut e)
            .is_err());
        }
        assert!(e.commit_to(&mut sdk).is_err());
        assert!(sdk.document().mesh_order().is_empty());
        assert_eq!(sdk.history_lengths(), (0, 0));
    }
}
