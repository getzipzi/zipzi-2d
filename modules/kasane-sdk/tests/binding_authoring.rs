use kasane_core::{
    Appearance, BindingAxis, Canvas, MeshBinding, MeshKeyform, Parameter, PreviewValues, Vec2,
};
use kasane_sdk::{prepare_png_asset, rectangle_mesh, AuthoringSession};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000401";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000402";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000403";
const PARAMETER_ID: &str = "00000000-0000-4000-8000-000000000404";
const BINDING_ID: &str = "00000000-0000-4000-8000-000000000405";

fn fixture() -> AuthoringSession {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(ASSET_ID, "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "eye",
        ASSET_ID,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let mut sdk = AuthoringSession::new(
        DOCUMENT_ID,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    sdk.edit("fixture", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    })
    .unwrap();
    sdk.drain_events();
    sdk
}

#[test]
fn metadata_publication_reuses_bindings_motions_and_identity_keys() {
    let mut sdk = fixture();
    let binding = binding(&sdk);
    let mut peer = sdk.mesh(MESH_ID).unwrap();
    peer.id = "00000000-0000-4000-8000-000000000421".into();
    peer.runtime_id = "Peer".into();
    let mut peer_binding = binding.clone();
    peer_binding.id = "00000000-0000-4000-8000-000000000422".into();
    peer_binding.mesh_id = peer.id.clone();
    let clip: kasane_core::document::MotionClip = serde_json::from_value(serde_json::json!({
        "id":"00000000-0000-4000-8000-000000000420", "name":"Motion", "duration":1., "fps":30.,
        "looping":false, "restricted_beziers":true, "fade_in":null, "fade_out":null,
        "track_order":[], "tracks":[], "events":[], "extensions":{}, "meta_extensions":{},
        "opaque_source_ids":null
    }))
    .unwrap();
    sdk.edit("dependencies", None, |edit| {
        edit.create_mesh(peer)?;
        edit.create_parameter(parameter(false))?;
        edit.create_binding(binding)?;
        edit.create_binding(peer_binding)?;
        edit.create_motion(clip)
    })
    .unwrap();
    let snapshot = sdk.read_snapshot();
    let handle = sdk.handle(kasane_sdk::ObjectKind::Mesh, MESH_ID).unwrap();
    for opacity in [0.3, 0.6, 0.9] {
        let previous = sdk.mesh(MESH_ID).unwrap().appearance.opacity;
        #[cfg(feature = "bench-instrumentation")]
        let capture = kasane_core::benchmark::Capture::start();
        let mut mesh = sdk.mesh(MESH_ID).unwrap();
        mesh.name = format!("Mesh {opacity}");
        mesh.appearance.opacity = opacity;
        let (_, receipt) = sdk
            .edit("presentation", None, |edit| edit.replace_mesh(mesh))
            .unwrap();
        #[cfg(feature = "bench-instrumentation")]
        {
            let stages = capture.finish();
            for stage in [
                "sdk.keys_in",
                "core.validate_motion",
                "core.canonicalize_binding",
                "core.canonicalize_scene_binding",
            ] {
                assert!(!stages.contains_key(stage), "repeated {stage}: {stages:?}");
            }
        }
        assert!(receipt.changed);
        assert!(receipt.changes.created.is_empty());
        assert!(receipt.changes.deleted.is_empty());
        assert_eq!(sdk.mesh(MESH_ID).unwrap().appearance.opacity, opacity);
        sdk.resolve_handle(&handle).unwrap();
        assert_eq!(
            snapshot
                .document()
                .get_mesh(MESH_ID)
                .unwrap()
                .appearance
                .opacity,
            1.
        );
        assert!(sdk.document().validate_structure().is_empty());
        sdk.undo().unwrap();
        assert_eq!(sdk.mesh(MESH_ID).unwrap().appearance.opacity, previous);
        sdk.redo().unwrap();
        assert_eq!(sdk.mesh(MESH_ID).unwrap().appearance.opacity, opacity);
        sdk.resolve_handle(&handle).unwrap();
    }
    // Keyform payload changes preserve owned axis/sample IDs too. Validate the
    // changed binding during replacement/publication, without revisiting peers.
    let mut binding = sdk.document().get_binding(BINDING_ID).unwrap().clone();
    binding.keyforms[0].appearance.opacity = 0.25;
    #[cfg(feature = "bench-instrumentation")]
    let capture = kasane_core::benchmark::Capture::start();
    let (_, receipt) = sdk
        .edit("keyform", None, |e| e.replace_binding(binding))
        .unwrap();
    #[cfg(feature = "bench-instrumentation")]
    {
        let stages = capture.finish();
        for stage in [
            "sdk.keys_in",
            "core.validate_motion",
            "core.canonicalize_scene_binding",
        ] {
            assert!(!stages.contains_key(stage), "repeated {stage}: {stages:?}");
        }
        assert_eq!(stages["core.canonicalize_binding"].calls, 2);
    }
    assert!(receipt.changed);
    assert!(receipt.changes.created.is_empty());
    assert!(receipt.changes.deleted.is_empty());
    assert!(!receipt.changes.content_changes.is_empty());
    assert!(sdk.document().validate_structure().is_empty());
    sdk.undo().unwrap();
    sdk.redo().unwrap();
    assert_eq!(
        sdk.document().get_binding(BINDING_ID).unwrap().keyforms[0]
            .appearance
            .opacity,
        0.25
    );
    sdk.resolve_handle(&handle).unwrap();
}

fn parameter(repeat: bool) -> Parameter {
    Parameter {
        id: PARAMETER_ID.into(),
        name: "open".into(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        repeat,
        ..Parameter::default()
    }
}

fn binding(sdk: &AuthoringSession) -> MeshBinding {
    let base = sdk.geometry(MESH_ID).unwrap().positions;
    let shifted = base.iter().map(|p| Vec2::new(p.x + 10.0, p.y)).collect();
    MeshBinding {
        id: BINDING_ID.into(),
        mesh_id: MESH_ID.into(),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: PARAMETER_ID.into(),
            keys: vec![0.0, 1.0],
        }],
        keyforms: vec![
            MeshKeyform {
                samples: Default::default(),
                keys: vec![0.0],
                positions: base,
                appearance: Appearance::default(),
                draw_order: None,
            },
            MeshKeyform {
                samples: Default::default(),
                keys: vec![1.0],
                positions: shifted,
                appearance: Appearance::default(),
                draw_order: None,
            },
        ],
    }
}

fn sample(sdk: &AuthoringSession, requested: f32) -> (f32, f32, bool) {
    let values = PreviewValues::from([(PARAMETER_ID.into(), requested)]);
    let frame = sdk.evaluate(&values).unwrap();
    (
        frame.parameters[0].value,
        frame.drawables[0].positions[0].x,
        frame.parameters[0].clamped,
    )
}

#[test]
fn complete_binding_interpolates_and_undoes_with_parameter() {
    let mut sdk = fixture();
    let b = binding(&sdk);
    let before = sdk.version();
    let (_, receipt) = sdk
        .edit("bind", Some(before), |edit| {
            edit.create_parameter(parameter(false))?;
            edit.create_binding(b)
        })
        .unwrap();
    assert_eq!(receipt.after.revision, before.revision + 1);
    assert_eq!(sdk.parameter_ids(), &[PARAMETER_ID]);
    assert_eq!(sdk.binding_ids(), &[BINDING_ID]);
    assert_eq!(sdk.binding_for_mesh(MESH_ID).unwrap().id, BINDING_ID);
    assert_eq!(sample(&sdk, 0.0), (0.0, 40.0, false));
    assert_eq!(sample(&sdk, 0.5), (0.5, 45.0, false));
    assert_eq!(sample(&sdk, 1.0), (1.0, 50.0, false));
    assert_eq!(sample(&sdk, 2.0), (1.0, 50.0, true));
    sdk.undo().unwrap();
    assert!(sdk.parameter_ids().is_empty());
    assert!(sdk.binding_ids().is_empty());
    sdk.redo().unwrap();
    assert_eq!(sample(&sdk, 0.5), (0.5, 45.0, false));
}

#[test]
fn incomplete_binding_aborts_parameter_creation_and_failed_range_replacement() {
    let mut sdk = fixture();
    let mut incomplete = binding(&sdk);
    incomplete.keyforms.pop();
    let version = sdk.version();
    let mut edit = sdk.begin_edit("incomplete", None).unwrap();
    edit.create_parameter(parameter(false)).unwrap();
    assert_eq!(
        edit.create_binding(incomplete).unwrap_err().code.as_ref(),
        "INCOMPLETE_KEYFORMS"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);
    assert!(sdk.parameter_ids().is_empty());

    let complete = binding(&sdk);
    sdk.edit("bind", None, |edit| {
        edit.create_parameter(parameter(false))?;
        edit.create_binding(complete)
    })
    .unwrap();
    let version = sdk.version();
    let mut narrowed = sdk.parameter(PARAMETER_ID).unwrap();
    narrowed.maximum = 0.5;
    let mut edit = sdk.begin_edit("bad range", None).unwrap();
    assert_eq!(
        edit.replace_parameter(narrowed).unwrap_err().code.as_ref(),
        "INVALID_KEYS"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.parameter(PARAMETER_ID).unwrap().maximum, 1.0);
}

#[test]
fn keyform_replacement_and_repeat_mapping_use_core_rules() {
    let mut sdk = fixture();
    let b = binding(&sdk);
    sdk.edit("bind", None, |edit| {
        edit.create_parameter(parameter(true))?;
        edit.create_binding(b)
    })
    .unwrap();
    assert_eq!(sample(&sdk, 1.25), (0.25, 42.5, false));
    let mut form = sdk.binding(BINDING_ID).unwrap().keyforms[1].clone();
    form.positions.iter_mut().for_each(|p| p.x += 10.0);
    sdk.edit("change form", None, |edit| {
        edit.set_mesh_keyform(BINDING_ID, form)
    })
    .unwrap();
    assert_eq!(sample(&sdk, 1.25), (0.25, 45.0, false));
    let mut binding = sdk.binding(BINDING_ID).unwrap();
    binding.keyforms[1]
        .positions
        .iter_mut()
        .for_each(|p| p.x += 10.0);
    sdk.edit("replace table", None, |edit| edit.replace_binding(binding))
        .unwrap();
    assert_eq!(sample(&sdk, 1.25), (0.25, 47.5, false));
}

#[path = "binding_authoring/conversions/mod.rs"]
mod conversions;

#[path = "binding_authoring/stable_domains/mod.rs"]
mod stable_domains;
